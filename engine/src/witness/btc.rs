mod btc_chain_tracking;
mod btc_source;

use std::{collections::HashMap, sync::Arc};

use bitcoin::Transaction;
use cf_chains::{
	btc::{deposit_address::DepositAddress, ScriptPubkey, UtxoId, CHANGE_ADDRESS_SALT},
	Bitcoin,
};
use cf_primitives::chains::assets::btc;
use futures_core::Future;
use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness};
use secp256k1::hashes::Hash;
use state_chain_runtime::BitcoinInstance;
use utilities::task_scope::Scope;

use crate::{
	btc::{
		retry_rpc::{BtcRetryRpcApi, BtcRetryRpcClient},
		rpc::BtcRpcClient,
	},
	db::PersistentKeyDB,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};
use btc_source::BtcSource;

use super::common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder};

use anyhow::Result;

const SAFETY_MARGIN: usize = 6;

pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	btc_client: BtcRetryRpcClient<impl Future<Output = BtcRpcClient> + Send>,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StateChainStreamApi + Clone + 'static + Send + Sync,
{
	let btc_source = BtcSource::new(btc_client.clone()).shared(scope);

	btc_source
		.clone()
		.chunk_by_time(epoch_source.clone())
		.chain_tracking(state_chain_client.clone(), btc_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let btc_client = btc_client.clone();
	btc_source
		.strictly_monotonic()
		.lag_safety(SAFETY_MARGIN)
		.logging("safe block produced")
		.then(move |header| {
			let btc_client = btc_client.clone();
			async move {
				let block = btc_client.block(header.hash).await;
				(header.data, block.txdata)
			}
		})
		.chunk_by_vault(epoch_source.vaults().await)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.then({
			let state_chain_client = state_chain_client.clone();
			move |epoch, header| {
				let state_chain_client = state_chain_client.clone();
				async move {
					// TODO: Make addresses a Map of some kind?
					let (((), txs), addresses) = header.data;

					let script_addresses = script_addresses(addresses);

					let deposit_witnesses = deposit_witnesses(&txs, script_addresses);

					// Submit all deposit witnesses for the block.
					if !deposit_witnesses.is_empty() {
						state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_ingress_egress::Call::<_, BitcoinInstance>::process_deposits {
									deposit_witnesses,
									block_height: header.index,
								}
								.into(),
							),
							epoch_index: epoch.index,
						})
						.await;
					}
					txs
				}
			}
		})
		.egress_items(scope, state_chain_stream, state_chain_client.clone())
		.await
		.then(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			async move {
				let (txs, monitored_tx_hashes) = header.data;

				for tx_hash in success_witnesses(&monitored_tx_hashes, &txs) {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(state_chain_runtime::RuntimeCall::BitcoinBroadcaster(
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
							)),
							epoch_index: epoch.index,
						})
						.await;
				}
			}
		})
		.continuous("Bitcoin".to_string(), db)
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}

fn deposit_witnesses(
	txs: &Vec<Transaction>,
	script_addresses: HashMap<Vec<u8>, ScriptPubkey>,
) -> Vec<DepositWitness<Bitcoin>> {
	let mut deposit_witnesses = Vec::new();
	for tx in txs {
		let tx_hash = tx.txid().as_raw_hash().to_byte_array();
		for (vout, tx_out) in (0..).zip(&tx.output) {
			if tx_out.value > 0 {
				let tx_script_pubkey_bytes = tx_out.script_pubkey.to_bytes();
				if let Some(bitcoin_script) = script_addresses.get(&tx_script_pubkey_bytes) {
					deposit_witnesses.push(DepositWitness {
						deposit_address: bitcoin_script.clone(),
						asset: btc::Asset::Btc,
						amount: tx_out.value,
						deposit_details: UtxoId { tx_id: tx_hash, vout },
					});
				}
			}
		}
	}
	deposit_witnesses
}

fn script_addresses(
	addresses: Vec<DepositChannelDetails<state_chain_runtime::Runtime, BitcoinInstance>>,
) -> HashMap<Vec<u8>, ScriptPubkey> {
	addresses
		.into_iter()
		.map(|channel| {
			assert_eq!(channel.deposit_channel.asset, btc::Asset::Btc);
			let script_pubkey = channel.deposit_channel.address;
			(script_pubkey.bytes(), script_pubkey)
		})
		.collect()
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
	use cf_chains::{
		btc::{deposit_address::DepositAddress, ScriptPubkey},
		DepositChannel,
	};

	fn fake_transaction(tx_outs: Vec<TxOut>) -> Transaction {
		Transaction {
			version: 2,
			lock_time: LockTime::Blocks(Height::from_consensus(0).unwrap()),
			input: vec![],
			output: tx_outs,
		}
	}

	fn fake_details(
		address: ScriptPubkey,
	) -> DepositChannelDetails<state_chain_runtime::Runtime, BitcoinInstance> {
		DepositChannelDetails::<_, BitcoinInstance> {
			opened_at: 1,
			expires_at: 10,
			deposit_channel: DepositChannel {
				channel_id: 1,
				address,
				asset: btc::Asset::Btc,
				state: DepositAddress::new([0; 32], 1),
			},
		}
	}

	#[test]
	fn deposit_witnesses_no_utxos_no_monitored() {
		let txs = vec![fake_transaction(vec![]), fake_transaction(vec![])];
		let deposit_witnesses = deposit_witnesses(&txs, HashMap::new());
		assert!(deposit_witnesses.is_empty());
	}

	#[test]
	fn filter_out_value_0() {
		let btc_deposit_script: ScriptPubkey = DepositAddress::new([0; 32], 9).script_pubkey();

		const UTXO_WITNESSED_1: u64 = 2324;
		let txs = vec![fake_transaction(vec![
			TxOut { value: 2324, script_pubkey: ScriptBuf::from(btc_deposit_script.bytes()) },
			TxOut { value: 0, script_pubkey: ScriptBuf::from(btc_deposit_script.bytes()) },
		])];

		let deposit_witnesses =
			deposit_witnesses(&txs, script_addresses(vec![(fake_details(btc_deposit_script))]));
		assert_eq!(deposit_witnesses.len(), 1);
		assert_eq!(deposit_witnesses[0].amount, UTXO_WITNESSED_1);
	}

	#[test]
	fn deposit_witnesses_several_same_tx() {
		const UTXO_WITNESSED_1: u64 = 2324;
		const UTXO_WITNESSED_2: u64 = 1234;

		let btc_deposit_script: ScriptPubkey = DepositAddress::new([0; 32], 9).script_pubkey();

		let txs = vec![
			fake_transaction(vec![
				TxOut {
					value: UTXO_WITNESSED_1,
					script_pubkey: ScriptBuf::from(btc_deposit_script.bytes()),
				},
				TxOut { value: 12223, script_pubkey: ScriptBuf::from(vec![0, 32, 121, 9]) },
				TxOut {
					value: UTXO_WITNESSED_2,
					script_pubkey: ScriptBuf::from(btc_deposit_script.bytes()),
				},
			]),
			fake_transaction(vec![]),
		];

		let deposit_witnesses =
			deposit_witnesses(&txs, script_addresses(vec![fake_details(btc_deposit_script)]));
		assert_eq!(deposit_witnesses.len(), 2);
		assert_eq!(deposit_witnesses[0].amount, UTXO_WITNESSED_1);
		assert_eq!(deposit_witnesses[1].amount, UTXO_WITNESSED_2);
	}

	#[test]
	fn deposit_witnesses_several_diff_tx() {
		let btc_deposit_script: ScriptPubkey = DepositAddress::new([0; 32], 9).script_pubkey();

		const UTXO_WITNESSED_1: u64 = 2324;
		const UTXO_WITNESSED_2: u64 = 1234;
		let txs = vec![
			fake_transaction(vec![
				TxOut { value: 2324, script_pubkey: ScriptBuf::from(btc_deposit_script.bytes()) },
				TxOut { value: 12223, script_pubkey: ScriptBuf::from(vec![0, 32, 121, 9]) },
			]),
			fake_transaction(vec![TxOut {
				value: 1234,
				script_pubkey: ScriptBuf::from(btc_deposit_script.bytes()),
			}]),
		];

		let deposit_witnesses =
			deposit_witnesses(&txs, script_addresses(vec![fake_details(btc_deposit_script)]));
		assert_eq!(deposit_witnesses.len(), 2);
		assert_eq!(deposit_witnesses[0].amount, UTXO_WITNESSED_1);
		assert_eq!(deposit_witnesses[1].amount, UTXO_WITNESSED_2);
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
