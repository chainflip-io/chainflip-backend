use std::collections::BTreeSet;

use bitcoincore_rpc::{
	bitcoin::{Block, BlockHash, Transaction, TxOut},
	Auth, Client, RpcApi,
};

use anyhow::Result;

use crate::settings;

pub struct BtcRpcClient {
	client: Client,
}

impl BtcRpcClient {
	pub fn new(btc_settings: &settings::Btc) -> Result<Self> {
		let auth = Auth::UserPass(btc_settings.rpc_user.clone(), btc_settings.rpc_password.clone());
		let client = Client::new(&btc_settings.http_node_endpoint, auth)?;
		Ok(Self { client })
	}
}

pub trait BtcRpcApi: Send + Sync {
	fn best_block_hash(&self) -> Result<BlockHash>;

	fn best_block_number(&self) -> Result<u64>;

	fn block(&self, block_hash: BlockHash) -> Result<Block>;
}

impl BtcRpcApi for BtcRpcClient {
	fn best_block_hash(&self) -> Result<BlockHash> {
		Ok(self.client.get_best_block_hash()?)
	}

	fn best_block_number(&self) -> Result<u64> {
		Ok(self.client.get_block_count()?)
	}

	fn block(&self, block_hash: BlockHash) -> Result<Block> {
		Ok(self.client.get_block(&block_hash)?)
	}
}

// Takes txs and list of monitored addresses. Returns a list of txs that are relevant to the
// monitored addresses.
fn filter_relevant_utxos(
	txs: Vec<Transaction>,
	monitored_script_pubkeys: BTreeSet<Vec<u8>>,
) -> Vec<TxOut> {
	let mut relevant_utxos = vec![];
	for tx in txs {
		for tx_out in &tx.output {
			if tx_out.value > 0 {
				let script_pubkey_bytes = tx_out.script_pubkey.to_bytes();
				if monitored_script_pubkeys.contains(&script_pubkey_bytes) {
					relevant_utxos.push(tx_out.clone());
				}
			}
		}
	}
	relevant_utxos
}

#[cfg(test)]
mod tests {
	use bitcoincore_rpc::bitcoin::{PackedLockTime, Script};

	use super::*;

	fn fake_transaction(tx_outs: Vec<TxOut>) -> Transaction {
		Transaction { version: 2, lock_time: PackedLockTime(0), input: vec![], output: tx_outs }
	}

	#[test]
	fn filter_relevant_utxos_no_utxos() {
		let txs = vec![fake_transaction(vec![]), fake_transaction(vec![])];
		let monitored_script_pubkeys = BTreeSet::new();
		assert!(filter_relevant_utxos(txs, monitored_script_pubkeys).is_empty());
	}

	#[test]
	fn filter_relevant_utxos_several_same_tx() {
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
		let relevant_utxos = filter_relevant_utxos(txs, monitored_script_pubkeys);
		assert_eq!(relevant_utxos.len(), 2);
		assert_eq!(relevant_utxos[0].value, 2324);
		assert_eq!(relevant_utxos[1].value, 1234);
	}

	#[test]
	fn filter_relevant_utxos_several_diff_tx() {
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
		let relevant_utxos = filter_relevant_utxos(txs, monitored_script_pubkeys);
		assert_eq!(relevant_utxos.len(), 2);
		assert_eq!(relevant_utxos[0].value, 2324);
		assert_eq!(relevant_utxos[1].value, 1234);
	}

	#[test]
	fn filter_out_value_0() {
		let monitored_pubkey = vec![0, 1, 2, 3];
		let txs = vec![fake_transaction(vec![
			TxOut { value: 2324, script_pubkey: Script::from(monitored_pubkey.clone()) },
			TxOut { value: 0, script_pubkey: Script::from(monitored_pubkey.clone()) },
		])];
		let monitored_script_pubkeys = BTreeSet::from([monitored_pubkey]);
		let relevant_utxos = filter_relevant_utxos(txs, monitored_script_pubkeys);
		assert_eq!(relevant_utxos.len(), 1);
		assert_eq!(relevant_utxos[0].value, 2324);
	}
}
