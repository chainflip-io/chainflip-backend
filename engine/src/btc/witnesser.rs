use std::sync::Arc;

use crate::{
	constants::BTC_INGRESS_BLOCK_SAFETY_MARGIN,
	state_chain_observer::client::extrinsic_api::ExtrinsicApi, witnesser::AddressMonitor,
};
use bitcoincore_rpc::bitcoin::Transaction;
use cf_chains::{
	btc::{Utxo, UtxoId},
	Bitcoin,
};
use cf_primitives::{chains::assets::btc, BitcoinAddress, BitcoinAddressFull, BitcoinAddressSeed};
use futures::StreamExt;
use pallet_cf_ingress_egress::IngressWitness;
use state_chain_runtime::BitcoinInstance;
use tokio::{select, sync::Mutex};
use tracing::{info, info_span, trace, Instrument};

use crate::{
	multisig::{ChainTag, PersistentKeyDB},
	witnesser::{
		block_head_stream_from::block_head_stream_from,
		checkpointing::{
			get_witnesser_start_block_with_checkpointing, StartCheckpointing, WitnessedUntil,
		},
		epoch_witnesser::{self},
		http_safe_stream::{safe_polling_http_head_stream, HTTP_POLL_INTERVAL},
		EpochStart,
	},
};

use super::rpc::{BtcRpcApi, BtcRpcClient};

// Takes txs and list of monitored addresses. Returns a list of txs that are relevant to the
// monitored addresses.
pub fn filter_interesting_utxos(
	txs: Vec<Transaction>,
	address_monitor: &mut AddressMonitor<BitcoinAddress, BitcoinAddressSeed>,
) -> Vec<(BitcoinAddressFull, Utxo)> {
	address_monitor.sync_addresses();
	let mut interesting_utxos = vec![];
	for tx in txs {
		for (vout, tx_out) in tx.output.iter().enumerate() {
			if tx_out.value > 0 {
				match tx_out.script_pubkey.to_bytes().try_into() {
					Ok(address) =>
						if let Some(bitcoin_address_seed) = address_monitor.contains(&address) {
							interesting_utxos.push((
								BitcoinAddressFull {
									script_pubkey: address,
									seed: bitcoin_address_seed.clone(),
								},
								Utxo {
									amount: tx_out.value,
									txid: tx
										.txid()
										.as_hash()
										.as_ref()
										.try_into()
										.expect("Is a hash"),
									vout: vout as u32,
									pubkey_x: bitcoin_address_seed.pubkey_x,
									salt: bitcoin_address_seed.salt,
								},
							));
						},
					Err(error) => {
						// This can happen, however, if it does, it won't be for one of our
						// addresses, since we can effectivley control how long the script pubkeys
						// are for our addresses. So we can log and ignore.
						tracing::warn!("Failed to convert script pubkey to bytes: {:?}", error);
					},
				}
			}
		}
	}
	interesting_utxos
}

pub async fn start<StateChainClient>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Bitcoin>>,
	state_chain_client: Arc<StateChainClient>,
	btc_rpc: BtcRpcClient,
	address_monitor: AddressMonitor<BitcoinAddress, BitcoinAddressSeed>,
	db: Arc<PersistentKeyDB>,
) -> Result<(), anyhow::Error>
where
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
{
	epoch_witnesser::start(
		Arc::new(Mutex::new(epoch_starts_receiver)),
		|_epoch_start| true,
		address_monitor,
		move |mut end_witnessing_receiver, epoch_start, mut address_monitor| {
			let db = db.clone();
			let btc_rpc = btc_rpc.clone();
			let state_chain_client = state_chain_client.clone();
			async move {
				// TODO: Look at deduplicating this
				let (from_block, witnessed_until_sender) =
					match get_witnesser_start_block_with_checkpointing::<cf_chains::Bitcoin>(
						ChainTag::Bitcoin,
						epoch_start.epoch_index,
						epoch_start.block_number,
						db,
					)
					.await
					.expect("Failed to start Btc witnesser checkpointing")
					{
						StartCheckpointing::Started((from_block, witnessed_until_sender)) =>
							(from_block, witnessed_until_sender),
						StartCheckpointing::AlreadyWitnessedEpoch =>
							return Result::<_, anyhow::Error>::Ok(address_monitor),
					};

				let mut block_number_stream_from = block_head_stream_from(
					from_block,
					safe_polling_http_head_stream(
						btc_rpc.clone(),
						HTTP_POLL_INTERVAL,
						BTC_INGRESS_BLOCK_SAFETY_MARGIN,
					)
					.await,
					move |block_number| futures::future::ready(Ok(block_number)),
				)
				.await?;

				let mut end_at_block = None;
				let mut current_block = from_block;

				loop {
					let block_number = select! {
						end_block = &mut end_witnessing_receiver => {
							end_at_block = Some(end_block.expect("end witnessing channel was dropped unexpectedly"));
							None
						}
						Some(block_number) = block_number_stream_from.next()  => {
							current_block = block_number;
							Some(block_number)
						}
					};

					if let Some(end_block) = end_at_block {
						if current_block >= end_block {
							info!("Btc block witnessers unsubscribe at block {end_block}");
							break
						}
					}

					if let Some(block_number) = block_number {
						let block = btc_rpc.block(btc_rpc.block_hash(block_number)?)?;

						trace!("Checking BTC block: {block_number} for interesting UTXOs");

						let ingress_witnesses =
							filter_interesting_utxos(block.txdata, &mut address_monitor)
								.into_iter()
								.map(|(ingress_address, utxo)| IngressWitness::<Bitcoin> {
									ingress_address,
									amount: utxo.amount.into(),
									asset: btc::Asset::Btc,
									tx_id: UtxoId {
										tx_hash: utxo.txid,
										vout_index: utxo.vout,
										pubkey_x: utxo.pubkey_x,
										salt: utxo.salt.into(),
									},
								})
								.collect();

						let _result =
							state_chain_client
								.submit_signed_extrinsic(
									pallet_cf_witnesser::Call::witness_at_epoch {
										call:
											Box::new(
												pallet_cf_ingress_egress::Call::<
													_,
													BitcoinInstance,
												>::do_ingress {
													ingress_witnesses,
												}
												.into(),
											),
										epoch_index: epoch_start.epoch_index,
									},
								)
								.await;

						witnessed_until_sender
							.send(WitnessedUntil {
								epoch_index: epoch_start.epoch_index,
								block_number,
							})
							.await
							.unwrap();
					}
				}

				Ok(address_monitor)
			}
		},
	)
	.instrument(info_span!("BTC-Witnesser"))
	.await
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use cf_chains::btc;

	use crate::{settings, state_chain_observer::client::mocks::MockStateChainClient};

	use super::*;

	#[ignore = "Requires a running BTC node"]
	#[tokio::test]
	async fn test_btc_witnesser() {
		crate::logging::init_json_logger();

		let rpc = BtcRpcClient::new(&settings::Btc {
			http_node_endpoint: "http://127.0.0.1:18443".to_string(),
			rpc_user: "kyle".to_string(),
			rpc_password: "password".to_string(),
		})
		.unwrap();

		let state_chain_client = Arc::new(MockStateChainClient::new());

		let (_script_pubkeys_sender, script_pubkeys_receiver) =
			tokio::sync::mpsc::unbounded_channel();

		let (epoch_starts_sender, epoch_starts_receiver) = async_broadcast::broadcast(1);

		let (_dir, db_path) = crate::testing::new_temp_directory_with_nonexistent_file();
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		epoch_starts_sender
			.broadcast(EpochStart {
				epoch_index: 1,
				block_number: 56,
				current: true,
				participant: true,
				data: btc::EpochStartData { change_address: Default::default() },
			})
			.await
			.unwrap();

		start(
			epoch_starts_receiver,
			state_chain_client,
			rpc,
			AddressMonitor::new(BTreeMap::new(), script_pubkeys_receiver),
			Arc::new(db),
		)
		.await
		.unwrap();
	}
}

#[cfg(test)]
mod test_utxo_filtering {
	use std::collections::BTreeMap;

	use bitcoincore_rpc::bitcoin::{PackedLockTime, Script, Transaction, TxOut};

	use super::*;

	fn fake_transaction(tx_outs: Vec<TxOut>) -> Transaction {
		Transaction { version: 2, lock_time: PackedLockTime(0), input: vec![], output: tx_outs }
	}

	#[test]
	fn filter_interesting_utxos_no_utxos() {
		let txs = vec![fake_transaction(vec![]), fake_transaction(vec![])];
		let (_monitor_ingress_sender, monitor_ingress_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		assert!(filter_interesting_utxos(
			txs,
			&mut AddressMonitor::new(BTreeMap::new(), monitor_ingress_receiver)
		)
		.is_empty());
	}

	#[test]
	fn filter_interesting_utxos_several_same_tx() {
		let monitored_pubkey = vec![0, 1, 2, 3];
		let txs = vec![
			fake_transaction(vec![
				TxOut { value: 2324, script_pubkey: Script::from(monitored_pubkey.clone()) },
				TxOut { value: 12223, script_pubkey: Script::from(vec![0, 32, 121, 9]) },
				TxOut { value: 1234, script_pubkey: Script::from(monitored_pubkey.clone()) },
			]),
			fake_transaction(vec![]),
		];
		let (_monitor_ingress_sender, monitor_ingress_receiver) =
			tokio::sync::mpsc::unbounded_channel();

		let interesting_utxos = filter_interesting_utxos(
			txs,
			&mut AddressMonitor::new(
				BTreeMap::from([(
					BitcoinAddress::try_from(monitored_pubkey).unwrap(),
					BitcoinAddressSeed { salt: 9, pubkey_x: [0; 32] },
				)]),
				monitor_ingress_receiver,
			),
		);
		assert_eq!(interesting_utxos.len(), 2);
		assert_eq!(interesting_utxos[0].1.amount, 2324);
		assert_eq!(interesting_utxos[1].1.amount, 1234);
	}

	#[test]
	fn filter_interesting_utxos_several_diff_tx() {
		let monitored_pubkey = vec![0, 1, 2, 3];
		let txs = vec![
			fake_transaction(vec![
				TxOut { value: 2324, script_pubkey: Script::from(monitored_pubkey.clone()) },
				TxOut { value: 12223, script_pubkey: Script::from(vec![0, 32, 121, 9]) },
			]),
			fake_transaction(vec![TxOut {
				value: 1234,
				script_pubkey: Script::from(monitored_pubkey.clone()),
			}]),
		];
		let (_monitor_ingress_sender, monitor_ingress_receiver) =
			tokio::sync::mpsc::unbounded_channel();

		let interesting_utxos = filter_interesting_utxos(
			txs,
			&mut AddressMonitor::new(
				BTreeMap::from([(
					BitcoinAddress::try_from(monitored_pubkey).unwrap(),
					BitcoinAddressSeed { salt: 9, pubkey_x: [0; 32] },
				)]),
				monitor_ingress_receiver,
			),
		);
		assert_eq!(interesting_utxos.len(), 2);
		assert_eq!(interesting_utxos[0].1.amount, 2324);
		assert_eq!(interesting_utxos[1].1.amount, 1234);
	}

	#[test]
	fn filter_out_value_0() {
		let monitored_pubkey = vec![0, 1, 2, 3];
		let txs = vec![fake_transaction(vec![
			TxOut { value: 2324, script_pubkey: Script::from(monitored_pubkey.clone()) },
			TxOut { value: 0, script_pubkey: Script::from(monitored_pubkey.clone()) },
		])];
		let (_monitor_ingress_sender, monitor_ingress_receiver) =
			tokio::sync::mpsc::unbounded_channel();

		let interesting_utxos = filter_interesting_utxos(
			txs,
			&mut AddressMonitor::new(
				BTreeMap::from([(
					BitcoinAddress::try_from(monitored_pubkey).unwrap(),
					BitcoinAddressSeed { salt: 9, pubkey_x: [0; 32] },
				)]),
				monitor_ingress_receiver,
			),
		);
		assert_eq!(interesting_utxos.len(), 1);
		assert_eq!(interesting_utxos[0].1.amount, 2324);
	}
}
