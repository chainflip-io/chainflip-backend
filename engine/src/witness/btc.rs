mod btc_chain_tracking;
mod btc_deposits;
pub mod btc_source;

use std::sync::Arc;

use bitcoin::{BlockHash, Transaction};
use cf_chains::btc::{self, deposit_address::DepositAddress, BlockNumber, CHANGE_ADDRESS_SALT};
use cf_primitives::EpochIndex;
use futures_core::Future;
use secp256k1::hashes::Hash;
use utilities::task_scope::Scope;

use crate::{
	btc::retry_rpc::{BtcRetryRpcApi, BtcRetryRpcClient},
	db::PersistentKeyDB,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};
use btc_source::BtcSource;

use super::common::{
	chain_source::{extension::ChainSourceExt, Header},
	epoch_source::{EpochSourceBuilder, Vault},
};

use anyhow::Result;

// safety margin of 5 implies 6 block confirmations
const SAFETY_MARGIN: usize = 5;

pub async fn process_egress<ProcessCall, ProcessingFut, ExtraInfo, ExtraHistoricInfo>(
	epoch: Vault<cf_chains::Bitcoin, ExtraInfo, ExtraHistoricInfo>,
	header: Header<u64, BlockHash, (Vec<Transaction>, Vec<(btc::Hash, BlockNumber)>)>,
	process_call: ProcessCall,
) where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let (txs, monitored_tx_hashes) = header.data;

	let monitored_tx_hashes = monitored_tx_hashes.iter().map(|(tx_hash, _)| tx_hash);

	for tx_hash in success_witnesses(monitored_tx_hashes, &txs) {
		process_call(
			state_chain_runtime::RuntimeCall::BitcoinBroadcaster(
				pallet_cf_broadcast::Call::transaction_succeeded {
					tx_out_id: tx_hash,
					signer_id: DepositAddress::new(
						epoch.info.0.public_key.current,
						CHANGE_ADDRESS_SALT,
					)
					.script_pubkey(),
					tx_fee: Default::default(),
					tx_metadata: (),
				},
			),
			epoch.index,
		)
		.await;
	}
}

pub async fn start<
	StateChainClient,
	StateChainStream,
	ProcessCall,
	ProcessingFut,
	PrewitnessCall,
	PrewitnessFut,
>(
	scope: &Scope<'_, anyhow::Error>,
	btc_client: BtcRetryRpcClient,
	process_call: ProcessCall,
	prewitness_call: PrewitnessCall,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	unfinalised_state_chain_stream: impl StateChainStreamApi<false>,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StateChainStreamApi + Clone + 'static + Send + Sync,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
	PrewitnessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> PrewitnessFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	PrewitnessFut: Future<Output = ()> + Send + 'static,
{
	let btc_source = BtcSource::new(btc_client.clone()).shared(scope);

	btc_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
		.chain_tracking(state_chain_client.clone(), btc_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let vaults = epoch_source.vaults().await;

	let strictly_monotonic_source = btc_source
		.strictly_monotonic()
		.then({
			let btc_client = btc_client.clone();
			move |header| {
				let btc_client = btc_client.clone();
				async move {
					let block = btc_client.block(header.hash).await;
					(header.data, block.txdata)
				}
			}
		})
		.shared(scope);

	// Pre-witnessing stream.
	strictly_monotonic_source
		.clone()
		.chunk_by_vault(vaults.clone(), scope)
		.deposit_addresses(scope, unfinalised_state_chain_stream, state_chain_client.clone())
		.await
		.btc_deposits(prewitness_call)
		.logging("pre-witnessing")
		.spawn(scope);

	// Full witnessing stream.
	strictly_monotonic_source
		.lag_safety(SAFETY_MARGIN)
		.logging("safe block produced")
		.chunk_by_vault(vaults, scope)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.btc_deposits(process_call.clone())
		.egress_items(scope, state_chain_stream, state_chain_client.clone())
		.await
		.then({
			let process_call = process_call.clone();
			move |epoch, header| process_egress(epoch, header, process_call.clone())
		})
		.continuous("Bitcoin".to_string(), db)
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}

fn success_witnesses<'a>(
	monitored_tx_hashes: impl Iterator<Item = &'a btc::Hash> + Clone,
	txs: &Vec<Transaction>,
) -> Vec<btc::Hash> {
	let mut successful_witnesses = Vec::new();

	for tx in txs {
		let mut monitored = monitored_tx_hashes.clone();
		let tx_hash = tx.txid().as_raw_hash().to_byte_array();
		if monitored.any(|&monitored_hash| monitored_hash == tx_hash) {
			successful_witnesses.push(tx_hash);
		}
	}
	successful_witnesses
}

#[cfg(test)]
mod tests {

	use super::*;
	use bitcoin::{
		absolute::{Height, LockTime},
		ScriptBuf, Transaction, TxOut,
	};

	fn fake_transaction(tx_outs: Vec<TxOut>) -> Transaction {
		Transaction {
			version: 2,
			lock_time: LockTime::Blocks(Height::from_consensus(0).unwrap()),
			input: vec![],
			output: tx_outs,
		}
	}

	#[test]
	fn witnesses_tx_hash_successfully() {
		let txs = vec![
			fake_transaction(vec![]),
			fake_transaction(vec![TxOut {
				value: 2324,
				script_pubkey: ScriptBuf::from(vec![0, 32, 121, 9]),
			}]),
			fake_transaction(vec![TxOut {
				value: 232232,
				script_pubkey: ScriptBuf::from(vec![32, 32, 121, 9]),
			}]),
			fake_transaction(vec![TxOut {
				value: 232232,
				script_pubkey: ScriptBuf::from(vec![33, 2, 1, 9]),
			}]),
		];

		let tx_hashes =
			txs.iter().map(|tx| tx.txid().to_raw_hash().to_byte_array()).collect::<Vec<_>>();

		// we're not monitoring for index 2, and they're out of order.
		let mut monitored_hashes = vec![tx_hashes[3], tx_hashes[0], tx_hashes[1]];

		let mut success_witnesses = success_witnesses(monitored_hashes.iter(), &txs);
		success_witnesses.sort();
		monitored_hashes.sort();

		assert_eq!(success_witnesses, monitored_hashes);
	}
}
