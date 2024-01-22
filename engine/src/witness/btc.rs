mod btc_chain_tracking;
mod btc_deposits;
pub mod btc_source;

use std::sync::Arc;

use bitcoin::BlockHash;
use cf_chains::btc::{self, deposit_address::DepositAddress, BlockNumber, CHANGE_ADDRESS_SALT};
use cf_primitives::{EpochIndex, NetworkEnvironment};
use futures_core::Future;
use secp256k1::hashes::Hash;
use utilities::task_scope::Scope;

use crate::{
	btc::{
		retry_rpc::{BtcRetryRpcApi, BtcRetryRpcClient},
		rpc::VerboseTransaction,
	},
	db::PersistentKeyDB,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED, UNFINALIZED},
	},
};
use btc_source::BtcSource;

use super::common::{
	chain_source::{extension::ChainSourceExt, Header},
	epoch_source::{EpochSourceBuilder, Vault},
};

use anyhow::Result;

pub async fn process_egress<ProcessCall, ProcessingFut, ExtraInfo, ExtraHistoricInfo>(
	epoch: Vault<cf_chains::Bitcoin, ExtraInfo, ExtraHistoricInfo>,
	header: Header<u64, BlockHash, (Vec<VerboseTransaction>, Vec<(btc::Hash, BlockNumber)>)>,
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

	for (tx_hash, tx) in success_witnesses(monitored_tx_hashes, txs) {
		process_call(
			state_chain_runtime::RuntimeCall::BitcoinBroadcaster(
				pallet_cf_broadcast::Call::transaction_succeeded {
					tx_out_id: tx_hash,
					signer_id: DepositAddress::new(
						epoch.info.0.public_key.current,
						CHANGE_ADDRESS_SALT,
					)
					.script_pubkey(),
					tx_fee: tx.fee.unwrap_or_default().to_sat(),
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
	unfinalised_state_chain_stream: impl StreamApi<UNFINALIZED>,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StreamApi<FINALIZED> + Clone,
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
	let btc_source = BtcSource::new(btc_client.clone()).strictly_monotonic().shared(scope);

	btc_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
		.chain_tracking(state_chain_client.clone(), btc_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let vaults = epoch_source.vaults().await;

	let block_source = btc_source
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
	block_source
		.clone()
		.chunk_by_vault(vaults.clone(), scope)
		.deposit_addresses(scope, unfinalised_state_chain_stream, state_chain_client.clone())
		.await
		.btc_deposits(prewitness_call)
		.logging("pre-witnessing")
		.spawn(scope);

	let btc_safety_margin = match state_chain_client
		.storage_value::<pallet_cf_ingress_egress::WitnessSafetyMargin<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>(state_chain_stream.cache().hash)
		.await?
	{
		Some(margin) => margin,
		None => {
			use chainflip_node::chain_spec::{berghain, devnet, perseverance};
			match state_chain_client
				.storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<state_chain_runtime::Runtime>>(
					state_chain_stream.cache().hash,
				)
				.await?
			{
				NetworkEnvironment::Mainnet => berghain::BITCOIN_SAFETY_MARGIN,
				NetworkEnvironment::Testnet => perseverance::BITCOIN_SAFETY_MARGIN,
				NetworkEnvironment::Development => devnet::BITCOIN_SAFETY_MARGIN,
			}
		},
	};

	tracing::info!("Safety margin for Bitcoin is set to {btc_safety_margin} blocks.",);

	// Full witnessing stream.
	block_source
		.lag_safety(btc_safety_margin as usize)
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
	txs: Vec<VerboseTransaction>,
) -> Vec<(btc::Hash, VerboseTransaction)> {
	let mut successful_witnesses = Vec::new();

	for tx in txs {
		let mut monitored = monitored_tx_hashes.clone();
		let tx_hash = tx.txid.as_raw_hash().to_byte_array();

		if monitored.any(|&monitored_hash| monitored_hash == tx_hash) {
			successful_witnesses.push((tx_hash, tx));
		}
	}
	successful_witnesses
}

#[cfg(test)]
mod tests {

	use bitcoin::Amount;

	use super::*;
	use crate::witness::btc::btc_deposits::tests::{fake_transaction, fake_verbose_vouts};

	#[test]
	fn witnesses_tx_hash_successfully() {
		const FEE_0: u64 = 1;
		const FEE_1: u64 = 111;
		const FEE_2: u64 = 222;
		const FEE_3: u64 = 333;
		let txs = vec![
			fake_transaction(vec![], Some(Amount::from_sat(FEE_0))),
			fake_transaction(
				fake_verbose_vouts(vec![(2324, vec![0, 32, 121, 9])]),
				Some(Amount::from_sat(FEE_1)),
			),
			fake_transaction(
				fake_verbose_vouts(vec![(232232, vec![32, 32, 121, 9])]),
				Some(Amount::from_sat(FEE_2)),
			),
			fake_transaction(
				fake_verbose_vouts(vec![(232232, vec![32, 32, 121, 9])]),
				Some(Amount::from_sat(FEE_3)),
			),
		];

		let tx_hashes =
			txs.iter().map(|tx| tx.txid.to_raw_hash().to_byte_array()).collect::<Vec<_>>();

		// we're not monitoring for index 2, and they're out of order.
		let monitored_hashes = [tx_hashes[3], tx_hashes[0], tx_hashes[1]];

		let sorted_monitored_hashes = vec![tx_hashes[0], tx_hashes[1], tx_hashes[3]];

		let (success_witness_hashes, txs): (Vec<_>, Vec<_>) =
			success_witnesses(monitored_hashes.iter(), txs).into_iter().unzip();
		assert_eq!(sorted_monitored_hashes, success_witness_hashes);
		assert_eq!(txs[0].fee.unwrap().to_sat(), FEE_0);
		assert_eq!(txs[1].fee.unwrap().to_sat(), FEE_1);
		// we weren't monitoring for 2, so the last fee should be FEE_3.
		assert_eq!(txs[2].fee.unwrap().to_sat(), FEE_3);
	}
}
