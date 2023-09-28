mod btc_chain_tracking;
mod btc_deposits;
mod btc_source;

use std::sync::Arc;

use bitcoin::Transaction;
use cf_chains::btc::{deposit_address::DepositAddress, CHANGE_ADDRESS_SALT};
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

use super::common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder};

use anyhow::Result;

const SAFETY_MARGIN: usize = 6;

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
		.shared(scope)
		.chunk_by_time(epoch_source.clone())
		.chain_tracking(state_chain_client.clone(), btc_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let vaults = epoch_source.vaults().await;

	// Pre-witnessing stream.
	let strictly_monotonic_source = btc_source.strictly_monotonic().shared(scope);
	strictly_monotonic_source
		.clone()
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
		.shared(scope)
		.chunk_by_vault(vaults.clone())
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.btc_deposits(prewitness_call)
		.spawn(scope);

	let btc_client = btc_client.clone();

	// Full witnessing stream.
	strictly_monotonic_source
		.lag_safety(SAFETY_MARGIN)
		.logging("safe block produced")
		.then(move |header| {
			let btc_client = btc_client.clone();
			async move {
				let block = btc_client.block(header.hash).await;
				(header.data, block.txdata)
			}
		})
		.shared(scope)
		.chunk_by_vault(vaults)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.btc_deposits(process_call.clone())
		.egress_items(scope, state_chain_stream, state_chain_client.clone())
		.await
		.then(move |epoch, header| {
			let process_call = process_call.clone();
			async move {
				let (txs, monitored_tx_hashes) = header.data;

				for tx_hash in success_witnesses(&monitored_tx_hashes, &txs) {
					process_call(
						state_chain_runtime::RuntimeCall::BitcoinBroadcaster(
							pallet_cf_broadcast::Call::transaction_succeeded {
								tx_out_id: tx_hash,
								signer_id: DepositAddress::new(
									epoch.info.0.public_key.current,
									CHANGE_ADDRESS_SALT,
								)
								.script_pubkey(),
								// TODO: Ideally we can submit an empty type here. For
								// Bitcoin and some other chains fee tracking is not
								// necessary. PRO-370.
								tx_fee: Default::default(),
							},
						),
						epoch.index,
					)
					.await;
				}
			}
		})
		.continuous("Bitcoin".to_string(), db)
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}

fn success_witnesses(monitored_tx_hashes: &[[u8; 32]], txs: &Vec<Transaction>) -> Vec<[u8; 32]> {
	let mut successful_witnesses = Vec::new();
	for tx in txs {
		let tx_hash = tx.txid().as_raw_hash().to_byte_array();
		if monitored_tx_hashes.contains(&tx_hash) {
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
		];

		let tx_hashes = txs
			.iter()
			.map(|tx| tx.txid().to_raw_hash().to_byte_array())
			// Only watch for the first 2.
			.take(2)
			.collect::<Vec<_>>();

		let success_witnesses = success_witnesses(&tx_hashes, &txs);

		assert_eq!(success_witnesses.len(), 2);
		assert_eq!(success_witnesses[0], tx_hashes[0]);
		assert_eq!(success_witnesses[1], tx_hashes[1]);
	}
}
