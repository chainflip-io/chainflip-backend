use async_trait::async_trait;
use std::sync::Arc;

use crate::{
	constants::BTC_INGRESS_BLOCK_SAFETY_MARGIN,
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::{
		block_witnesser::{
			BlockStream, BlockWitnesser, BlockWitnesserGenerator, BlockWitnesserGeneratorWrapper,
		},
		epoch_process_runner::start_epoch_process_runner,
		AddressMonitor, ChainBlockNumber,
	},
};
use bitcoincore_rpc::bitcoin::{hashes::Hash, Transaction};
use cf_chains::{
	address::ScriptPubkeyBytes,
	btc::{
		ingress_address::derive_btc_ingress_bitcoin_script, BitcoinScriptBounded,
		BitcoinTrackedData, UtxoId, CHANGE_ADDRESS_SALT,
	},
	Bitcoin,
};
use cf_primitives::{chains::assets::btc, EpochIndex};
use futures::StreamExt;
use pallet_cf_environment::ChangeUtxoWitness;
use pallet_cf_ingress_egress::IngressWitness;
use state_chain_runtime::BitcoinInstance;
use tokio::sync::Mutex;
use tracing::{info_span, trace, Instrument};

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
	address_monitor: &mut AddressMonitor<
		BitcoinScriptBounded,
		ScriptPubkeyBytes,
		BitcoinScriptBounded,
	>,
	change_pubkey: &cf_chains::btc::AggKey,
) -> (Vec<IngressWitness<Bitcoin>>, Vec<ChangeUtxoWitness>) {
	address_monitor.sync_addresses();
	let mut ingress_witnesses = vec![];
	let mut change_witnesses = vec![];
	for tx in txs {
		for (vout, tx_out) in (0u32..).zip(tx.output.clone()) {
			if tx_out.value > 0 {
				let tx_hash = tx.txid().as_hash().into_inner();
				let script_pubkey_bytes = tx_out.script_pubkey.to_bytes();
				if let Some(bitcoin_script) = address_monitor.get(&script_pubkey_bytes) {
					ingress_witnesses.push(IngressWitness {
						ingress_address: bitcoin_script,
						asset: btc::Asset::Btc,
						amount: tx_out.value,
						tx_id: UtxoId { tx_hash, vout },
					});
				} else if script_pubkey_bytes ==
					derive_btc_ingress_bitcoin_script(
						change_pubkey.pubkey_x,
						CHANGE_ADDRESS_SALT,
					)
					.serialize()
				{
					change_witnesses.push(ChangeUtxoWitness {
						amount: tx_out.value,
						change_pubkey: *change_pubkey,
						utxo_id: UtxoId { tx_hash, vout },
					});
				}
			}
		}
	}
	(ingress_witnesses, change_witnesses)
}

pub async fn start<StateChainClient>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Bitcoin>>,
	state_chain_client: Arc<StateChainClient>,
	btc_rpc: BtcRpcClient,
	address_monitor: AddressMonitor<BitcoinScriptBounded, ScriptPubkeyBytes, BitcoinScriptBounded>,
	db: Arc<PersistentKeyDB>,
) -> Result<(), anyhow::Error>
where
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
{
	start_epoch_process_runner(
		Arc::new(Mutex::new(epoch_starts_receiver)),
		BlockWitnesserGeneratorWrapper {
			db,
			generator: BtcWitnesserGenerator { state_chain_client, btc_rpc },
		},
		address_monitor,
	)
	.instrument(info_span!("BTC-Witnesser"))
	.await
	.map_err(|_| anyhow::anyhow!("Btc witnesser failed"))
}

struct BtcBlockWitnesser<StateChainClient> {
	state_chain_client: Arc<StateChainClient>,
	btc_rpc: BtcRpcClient,
	epoch_index: EpochIndex,
	change_pubkey: cf_chains::btc::AggKey,
}

#[async_trait]
impl<StateChainClient> BlockWitnesser for BtcBlockWitnesser<StateChainClient>
where
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
{
	type Chain = Bitcoin;
	type Block = ChainBlockNumber<Self::Chain>;
	type StaticState =
		AddressMonitor<BitcoinScriptBounded, ScriptPubkeyBytes, BitcoinScriptBounded>;

	async fn process_block(
		&mut self,
		block_number: ChainBlockNumber<Bitcoin>,
		address_monitor: &mut Self::StaticState,
	) -> anyhow::Result<()> {
		let block = self.btc_rpc.block(self.btc_rpc.block_hash(block_number)?)?;

		trace!("Checking BTC block: {block_number} for interesting UTXOs");

		let (ingress_witnesses, change_witnesses) =
			filter_interesting_utxos(block.txdata, address_monitor, &self.change_pubkey);

		if !ingress_witnesses.is_empty() {
			let _result = self
				.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(
						pallet_cf_ingress_egress::Call::<_, BitcoinInstance>::do_ingress {
							ingress_witnesses,
						}
						.into(),
					),
					epoch_index: self.epoch_index,
				})
				.await;
		}

		if !change_witnesses.is_empty() {
			let _result = self
				.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(
						pallet_cf_environment::Call::add_bitcoin_change_utxos { change_witnesses }
							.into(),
					),
					epoch_index: self.epoch_index,
				})
				.await;
		}

		if let Some(fee_rate_sats_per_byte) = self.btc_rpc.next_block_fee_rate()? {
			let _result = self
				.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(state_chain_runtime::RuntimeCall::BitcoinChainTracking(
						pallet_cf_chain_tracking::Call::update_chain_state {
							state: BitcoinTrackedData {
								block_height: block_number,
								fee_rate_sats_per_byte,
							},
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
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
{
	state_chain_client: Arc<StateChainClient>,
	btc_rpc: BtcRpcClient,
}

#[async_trait]
impl<StateChainClient> BlockWitnesserGenerator for BtcWitnesserGenerator<StateChainClient>
where
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
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
			change_pubkey: epoch.data.change_pubkey,
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
		crate::logging::init_json_logger();

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
			AddressMonitor::new(BTreeSet::new()).1,
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
	use bitcoincore_rpc::bitcoin::{PackedLockTime, Script, Transaction, TxOut};

	fn fake_transaction(tx_outs: Vec<TxOut>) -> Transaction {
		Transaction { version: 2, lock_time: PackedLockTime(0), input: vec![], output: tx_outs }
	}

	#[test]
	fn filter_interesting_utxos_no_utxos() {
		let txs = vec![fake_transaction(vec![]), fake_transaction(vec![])];

		assert!(filter_interesting_utxos(
			txs,
			&mut AddressMonitor::new(BTreeSet::new()).1,
			&Default::default(),
		)
		.0
		.is_empty());
	}

	#[test]
	fn filter_interesting_utxos_several_same_tx() {
		const UTXO_WITNESSED_1: u64 = 2324;
		const UTXO_WITNESSED_2: u64 = 1234;

		let btc_ingress_script: BitcoinScriptBounded =
			derive_btc_ingress_bitcoin_script([0; 32], 9).try_into().unwrap();

		let txs = vec![
			fake_transaction(vec![
				TxOut {
					value: UTXO_WITNESSED_1,
					script_pubkey: Script::from(btc_ingress_script.data.to_vec()),
				},
				TxOut { value: 12223, script_pubkey: Script::from(vec![0, 32, 121, 9]) },
				TxOut {
					value: UTXO_WITNESSED_2,
					script_pubkey: Script::from(btc_ingress_script.data.to_vec()),
				},
			]),
			fake_transaction(vec![]),
		];

		let (ingress_witnesses, _) = filter_interesting_utxos(
			txs,
			&mut AddressMonitor::new(BTreeSet::from([btc_ingress_script])).1,
			&Default::default(),
		);
		assert_eq!(ingress_witnesses.len(), 2);
		assert_eq!(ingress_witnesses[0].amount, UTXO_WITNESSED_1);
		assert_eq!(ingress_witnesses[1].amount, UTXO_WITNESSED_2);
	}

	#[test]
	fn filter_interesting_utxos_several_diff_tx() {
		let btc_ingress_script: BitcoinScriptBounded =
			derive_btc_ingress_bitcoin_script([0; 32], 9).try_into().unwrap();

		const UTXO_WITNESSED_1: u64 = 2324;
		const UTXO_WITNESSED_2: u64 = 1234;
		let txs = vec![
			fake_transaction(vec![
				TxOut {
					value: 2324,
					script_pubkey: Script::from(btc_ingress_script.data.to_vec()),
				},
				TxOut { value: 12223, script_pubkey: Script::from(vec![0, 32, 121, 9]) },
			]),
			fake_transaction(vec![TxOut {
				value: 1234,
				script_pubkey: Script::from(btc_ingress_script.data.to_vec()),
			}]),
		];

		let (ingress_witnesses, _change_witnesses) = filter_interesting_utxos(
			txs,
			&mut AddressMonitor::new(BTreeSet::from([btc_ingress_script])).1,
			&Default::default(),
		);
		assert_eq!(ingress_witnesses.len(), 2);
		assert_eq!(ingress_witnesses[0].amount, UTXO_WITNESSED_1);
		assert_eq!(ingress_witnesses[1].amount, UTXO_WITNESSED_2);
	}

	#[test]
	fn filter_out_value_0() {
		let btc_ingress_script: BitcoinScriptBounded =
			derive_btc_ingress_bitcoin_script([0; 32], 9).try_into().unwrap();

		const UTXO_WITNESSED_1: u64 = 2324;
		let txs = vec![fake_transaction(vec![
			TxOut { value: 2324, script_pubkey: Script::from(btc_ingress_script.data.to_vec()) },
			TxOut { value: 0, script_pubkey: Script::from(btc_ingress_script.data.to_vec()) },
		])];

		let (ingress_witnesses, _change_witnesses) = filter_interesting_utxos(
			txs,
			&mut AddressMonitor::new(BTreeSet::from([btc_ingress_script])).1,
			&Default::default(),
		);
		assert_eq!(ingress_witnesses.len(), 1);
		assert_eq!(ingress_witnesses[0].amount, UTXO_WITNESSED_1);
	}
}
