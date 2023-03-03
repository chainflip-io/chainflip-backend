use std::{collections::BTreeSet, sync::Arc};

use bitcoincore_rpc::{
	bitcoin::{Block, BlockHash, Transaction, TxOut},
	Auth, Client, RpcApi,
};

use anyhow::Result;
use cf_chains::btc::BlockNumber;

use crate::{settings, witnesser::LatestBlockNumber};

use super::ScriptPubKey;

#[derive(Clone)]
pub struct BtcRpcClient {
	client: Arc<Client>,
}

impl BtcRpcClient {
	pub fn new(btc_settings: &settings::Btc) -> Result<Self> {
		Ok(Self {
			client: Arc::new(Client::new(
				&btc_settings.http_node_endpoint,
				Auth::UserPass(btc_settings.rpc_user.clone(), btc_settings.rpc_password.clone()),
			)?),
		})
	}
}

pub trait BtcRpcApi: Send + Sync {
	fn best_block_hash(&self) -> Result<BlockHash>;

	fn block(&self, block_hash: BlockHash) -> Result<Block>;

	fn block_hash(&self, block_number: BlockNumber) -> Result<BlockHash>;
}

impl BtcRpcApi for BtcRpcClient {
	fn best_block_hash(&self) -> Result<BlockHash> {
		Ok(self.client.get_best_block_hash()?)
	}

	fn block(&self, block_hash: BlockHash) -> Result<Block> {
		Ok(self.client.get_block(&block_hash)?)
	}

	fn block_hash(&self, block_number: BlockNumber) -> Result<BlockHash> {
		Ok(self.client.get_block_hash(block_number)?)
	}
}

#[async_trait::async_trait]
impl LatestBlockNumber for BtcRpcClient {
	type BlockNumber = BlockNumber;

	async fn latest_block_number(&self) -> Result<BlockNumber> {
		Ok(self.client.get_block_count()?)
	}
}

// Takes txs and list of monitored addresses. Returns a list of txs that are relevant to the
// monitored addresses.
pub fn filter_interesting_utxos(
	txs: Vec<Transaction>,
	monitored_script_pubkeys: &BTreeSet<ScriptPubKey>,
) -> Vec<TxOut> {
	let mut interesting_utxos = vec![];
	for tx in txs {
		for tx_out in &tx.output {
			if tx_out.value > 0 {
				let script_pubkey_bytes = tx_out.script_pubkey.to_bytes();
				if monitored_script_pubkeys.contains(&script_pubkey_bytes) {
					interesting_utxos.push(tx_out.clone());
				}
			}
		}
	}
	interesting_utxos
}

#[cfg(test)]
mod tests {
	use bitcoincore_rpc::bitcoin::{PackedLockTime, Script};

	use super::*;

	fn fake_transaction(tx_outs: Vec<TxOut>) -> Transaction {
		Transaction { version: 2, lock_time: PackedLockTime(0), input: vec![], output: tx_outs }
	}

	#[test]
	fn filter_interesting_utxos_no_utxos() {
		let txs = vec![fake_transaction(vec![]), fake_transaction(vec![])];
		let monitored_script_pubkeys = BTreeSet::new();
		assert!(filter_interesting_utxos(txs, &monitored_script_pubkeys).is_empty());
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
		let monitored_script_pubkeys = BTreeSet::from([monitored_pubkey]);
		let interesting_utxos = filter_interesting_utxos(txs, &monitored_script_pubkeys);
		assert_eq!(interesting_utxos.len(), 2);
		assert_eq!(interesting_utxos[0].value, 2324);
		assert_eq!(interesting_utxos[1].value, 1234);
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
		let monitored_script_pubkeys = BTreeSet::from([monitored_pubkey]);
		let interesting_utxos = filter_interesting_utxos(txs, &monitored_script_pubkeys);
		assert_eq!(interesting_utxos.len(), 2);
		assert_eq!(interesting_utxos[0].value, 2324);
		assert_eq!(interesting_utxos[1].value, 1234);
	}

	#[test]
	fn filter_out_value_0() {
		let monitored_pubkey = vec![0, 1, 2, 3];
		let txs = vec![fake_transaction(vec![
			TxOut { value: 2324, script_pubkey: Script::from(monitored_pubkey.clone()) },
			TxOut { value: 0, script_pubkey: Script::from(monitored_pubkey.clone()) },
		])];
		let monitored_script_pubkeys = BTreeSet::from([monitored_pubkey]);
		let interesting_utxos = filter_interesting_utxos(txs, &monitored_script_pubkeys);
		assert_eq!(interesting_utxos.len(), 1);
		assert_eq!(interesting_utxos[0].value, 2324);
	}
}
