use crate::{
	constants::BTC_INGRESS_BLOCK_SAFETY_MARGIN,
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witnesser::{
		block_witnesser::{
			BlockStream, BlockWitnesser, BlockWitnesserGenerator, BlockWitnesserGeneratorWrapper,
		},
		epoch_process_runner::start_epoch_process_runner,
		ChainBlockNumber, ItemMonitor,
	},
};
use anyhow::anyhow;
use async_trait::async_trait;
use bitcoin::{hashes::Hash, Transaction};
use cf_chains::{
	btc::{deposit_address::DepositAddress, ScriptPubkey, CHANGE_ADDRESS_SALT},
	Bitcoin,
};
use cf_primitives::EpochIndex;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info_span, trace, Instrument};

use crate::{
	db::PersistentKeyDB,
	witnesser::{
		block_head_stream_from::block_head_stream_from,
		http_safe_stream::{safe_polling_http_head_stream, HTTP_POLL_INTERVAL},
		EpochStart,
	},
};

use super::rpc::{BtcRpcApi, BtcRpcClient};

// Takes txs and list of monitored addresses. Returns a list of txs that are relevant to the
// monitored addresses.
pub fn filter_interesting_utxos(
	txs: Vec<Transaction>,
	tx_hash_monitor: &mut ItemMonitor<[u8; 32], [u8; 32], ()>,
) -> Vec<[u8; 32]> {
	tx_hash_monitor.sync_items();
	let mut tx_success_witnesses = vec![];

	debug!("looking for hashes: {:?}", tx_hash_monitor);

	for tx in txs {
		let tx_hash = tx.txid().as_raw_hash().to_byte_array();
		if tx_hash_monitor.contains(&tx_hash) {
			// TODO: We shouldn't need to add a fee
			tx_success_witnesses.push(tx_hash);
		}
	}
	tx_success_witnesses
}

pub async fn start<StateChainClient>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Bitcoin>>,
	state_chain_client: Arc<StateChainClient>,
	btc_rpc: BtcRpcClient,
	tx_hash_monitor: ItemMonitor<[u8; 32], [u8; 32], ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<(), anyhow::Error>
where
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
{
	start_epoch_process_runner(
		None,
		Arc::new(Mutex::new(epoch_starts_receiver)),
		BlockWitnesserGeneratorWrapper {
			db,
			generator: BtcWitnesserGenerator { state_chain_client, btc_rpc },
		},
		tx_hash_monitor,
	)
	.instrument(info_span!("BTC-Witnesser"))
	.await
	.map_err(|_| anyhow!("Btc witnesser failed"))
}

struct BtcBlockWitnesser<StateChainClient> {
	state_chain_client: Arc<StateChainClient>,
	btc_rpc: BtcRpcClient,
	epoch_index: EpochIndex,
	// Who we report as the signer to the SC. This should always the address of the
	// current agg key.
	current_pubkey: ScriptPubkey,
}

#[async_trait]
impl<StateChainClient> BlockWitnesser for BtcBlockWitnesser<StateChainClient>
where
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
{
	type Chain = Bitcoin;
	type Block = ChainBlockNumber<Self::Chain>;
	type StaticState = ItemMonitor<[u8; 32], [u8; 32], ()>;

	async fn process_block(
		&mut self,
		block_number: ChainBlockNumber<Bitcoin>,
		tx_hash_monitor: &mut Self::StaticState,
	) -> anyhow::Result<()> {
		let block = self.btc_rpc.block(self.btc_rpc.block_hash(block_number).await?).await?;

		trace!("Checking BTC block: {block_number} for interesting UTXOs");

		let tx_success_witnesses = filter_interesting_utxos(block.txdata, tx_hash_monitor);

		debug!(
			"Found {} successful transactions in block {block_number}.",
			tx_success_witnesses.len(),
		);

		for tx_hash in tx_success_witnesses {
			self.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(state_chain_runtime::RuntimeCall::BitcoinBroadcaster(
						pallet_cf_broadcast::Call::transaction_succeeded {
							tx_out_id: tx_hash,
							signer_id: self.current_pubkey.clone(),
							// TODO: Ideally we can submit an empty type here. For Bitcoin
							// and some other chains fee tracking is not necessary. PRO-370.
							tx_fee: Default::default(),
						},
					)),
					epoch_index: self.epoch_index,
				})
				.await;
		}

		Ok(())
	}
}

struct BtcWitnesserGenerator<StateChainClient>
where
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
{
	state_chain_client: Arc<StateChainClient>,
	btc_rpc: BtcRpcClient,
}

#[async_trait]
impl<StateChainClient> BlockWitnesserGenerator for BtcWitnesserGenerator<StateChainClient>
where
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
{
	type Witnesser = BtcBlockWitnesser<StateChainClient>;

	fn create_witnesser(
		&self,
		epoch: EpochStart<<Self::Witnesser as BlockWitnesser>::Chain>,
	) -> Self::Witnesser {
		BtcBlockWitnesser {
			state_chain_client: self.state_chain_client.clone(),
			btc_rpc: self.btc_rpc.clone(),
			epoch_index: epoch.epoch_index,
			current_pubkey: DepositAddress::new(
				epoch.data.change_pubkey.current,
				CHANGE_ADDRESS_SALT,
			)
			.script_pubkey(),
		}
	}

	async fn get_block_stream(
		&mut self,
		from_block: ChainBlockNumber<<Self::Witnesser as BlockWitnesser>::Chain>,
	) -> anyhow::Result<BlockStream<<Self::Witnesser as BlockWitnesser>::Block>> {
		let block_stream = block_head_stream_from(
			from_block,
			safe_polling_http_head_stream(
				self.btc_rpc.clone(),
				HTTP_POLL_INTERVAL,
				BTC_INGRESS_BLOCK_SAFETY_MARGIN,
			)
			.await,
			move |block_number| futures::future::ready(Ok(block_number)),
		)
		.await?
		.map(Ok);

		Ok(Box::pin(block_stream))
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeSet;

	use cf_chains::btc;

	use crate::{settings, state_chain_observer::client::mocks::MockStateChainClient};

	use super::*;

	#[ignore = "Requires a running BTC node"]
	#[tokio::test]
	async fn test_btc_witnesser() {
		let rpc = BtcRpcClient::new(&settings::Btc {
			http_node_endpoint: "http://127.0.0.1:18443".to_string(),
			rpc_user: "user".to_string(),
			rpc_password: "password".to_string(),
		})
		.unwrap();

		let state_chain_client = Arc::new(MockStateChainClient::new());

		let (epoch_starts_sender, epoch_starts_receiver) = async_broadcast::broadcast(1);

		let (_dir, db_path) = utilities::testing::new_temp_directory_with_nonexistent_file();
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		epoch_starts_sender
			.broadcast(EpochStart {
				epoch_index: 1,
				block_number: 56,
				current: true,
				participant: true,
				data: btc::EpochStartData { change_pubkey: Default::default() },
			})
			.await
			.unwrap();

		start(
			epoch_starts_receiver,
			state_chain_client,
			rpc,
			ItemMonitor::new(BTreeSet::new()).1,
			Arc::new(db),
		)
		.await
		.unwrap();
	}
}

#[cfg(test)]
mod test_utxo_filtering {
	use std::collections::BTreeSet;

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
		];
		let tx_hashes =
			txs.iter().map(|tx| tx.txid().to_raw_hash().to_byte_array()).collect::<Vec<_>>();

		let success_witnesses = filter_interesting_utxos(
			txs,
			&mut ItemMonitor::new(BTreeSet::from_iter(tx_hashes.clone())).1,
		);

		assert_eq!(success_witnesses.len(), 2);
		assert_eq!(success_witnesses[0], tx_hashes[0]);
		assert_eq!(success_witnesses[1], tx_hashes[1]);
	}
}
