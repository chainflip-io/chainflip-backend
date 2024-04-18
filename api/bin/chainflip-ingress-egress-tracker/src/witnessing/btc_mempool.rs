use crate::{Storable, Store};
use bitcoin::{BlockHash, Network, ScriptBuf, Transaction, Txid};
use cf_chains::btc::BitcoinNetwork;
use cf_primitives::ForeignChain;
use chainflip_engine::{
	btc::rpc::{BtcRpcApi, BtcRpcClient},
	settings::HttpBasicAuthEndpoint,
};
use serde::Serialize;
use std::{
	collections::{HashMap, HashSet},
	str::FromStr,
	time::Duration,
};
use tracing::{error, info};
use utilities::task_scope;

#[derive(Clone)]
pub struct QueryResult {
	confirmations: u32,
	destination: String,
	value: u64,
	tx_hash: Txid,
}

impl Serialize for QueryResult {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		use serde::ser::SerializeMap;

		let mut map = serializer.serialize_map(Some(3))?;
		map.serialize_entry("confirmations", &self.confirmations)?;
		map.serialize_entry("tx_hash", &self.tx_hash)?;
		map.serialize_entry("value", &format!("0x{:x}", self.value))?;
		map.end()
	}
}

impl Storable for QueryResult {
	fn get_key(&self) -> String {
		let chain = ForeignChain::Bitcoin.to_string();
		format!("mempool:{chain}:{}", self.destination)
	}

	fn get_expiry_duration(&self) -> Duration {
		Duration::from_secs(60)
	}
}

fn script_to_address(script: &ScriptBuf, btc_network: Network) -> Option<String> {
	bitcoin::Address::from_script(script, btc_network)
		.map(|addr| addr.to_string())
		.ok()
}

#[derive(Clone)]
struct Cache {
	best_block_hash: BlockHash,
	transactions: HashMap<String, QueryResult>,
	known_tx_hashes: HashSet<Txid>,
}

impl Default for Cache {
	fn default() -> Self {
		Self {
			best_block_hash: BlockHash::from_str(
				"0000000000000000000000000000000000000000000000000000000000000000",
			)
			.unwrap(),
			transactions: Default::default(),
			known_tx_hashes: Default::default(),
		}
	}
}

const SAFETY_MARGIN: u32 = 10;
const REFRESH_INTERVAL: u64 = 10;

async fn update_cache<T: BtcRpcApi>(
	btc: &T,
	previous_cache: Cache,
	btc_network: Network,
) -> anyhow::Result<Cache> {
	let all_mempool_transactions: Vec<Txid> = btc.get_raw_mempool().await?;

	let mut new_transactions: HashMap<String, QueryResult> = Default::default();
	let mut new_known_tx_hashes: HashSet<Txid> = Default::default();
	let previous_mempool: HashMap<Txid, QueryResult> = previous_cache
		.clone()
		.transactions
		.into_iter()
		.filter_map(|(_, query_result)| {
			if query_result.confirmations == 0 {
				Some((query_result.tx_hash, query_result))
			} else {
				None
			}
		})
		.collect();
	let unknown_mempool_transactions: Vec<Txid> = all_mempool_transactions
		.into_iter()
		.filter(|tx_hash| {
			if let Some(known_transaction) = previous_mempool.get(tx_hash) {
				new_known_tx_hashes.insert(*tx_hash);
				new_transactions
					.insert(known_transaction.destination.clone(), known_transaction.clone());
			} else if previous_cache.known_tx_hashes.contains(tx_hash) {
				new_known_tx_hashes.insert(*tx_hash);
			} else {
				return true
			}
			false
		})
		.collect();

	tracing::debug!(
		"Number of unknown mempool transactions: {}",
		unknown_mempool_transactions.len()
	);

	let transactions: Vec<Transaction> =
		btc.get_raw_transactions(unknown_mempool_transactions).await?;

	tracing::debug!("Number of raw transactions returned: {}", transactions.len());

	for tx in transactions {
		let txid = tx.txid();
		for txout in tx.output {
			new_known_tx_hashes.insert(txid);

			if let Some(addr) = script_to_address(&txout.script_pubkey, btc_network) {
				new_transactions.insert(
					addr.clone(),
					QueryResult {
						destination: addr,
						confirmations: 0,
						value: txout.value,
						tx_hash: txid,
					},
				);
			}
		}
	}
	let block_hash = btc.best_block_hash().await?;

	if previous_cache.best_block_hash == block_hash {
		for entry in previous_cache.transactions {
			if entry.1.confirmations > 0 {
				new_transactions.insert(entry.0, entry.1);
			}
		}
	} else {
		info!("New block found: {}", block_hash);
		let mut block_hash_to_query = block_hash;
		for confirmations in 1..SAFETY_MARGIN {
			let block = btc.block(block_hash_to_query).await?;
			for tx in block.txdata {
				let tx_hash = tx.txid;
				for txout in tx.vout {
					if let Some(addr) = script_to_address(&txout.script_pubkey, btc_network) {
						new_transactions.insert(
							addr.clone(),
							QueryResult {
								destination: addr,
								confirmations,
								value: txout.value.to_sat(),
								tx_hash,
							},
						);
					}
				}
			}
			block_hash_to_query = block.header.previous_block_hash.unwrap();
		}
	}
	Ok(Cache {
		best_block_hash: block_hash,
		transactions: new_transactions,
		known_tx_hashes: new_known_tx_hashes,
	})
}

pub fn start<S: Store>(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	endpoint: HttpBasicAuthEndpoint,
	mut store: S,
	btc_network: BitcoinNetwork,
) {
	scope.spawn({
		async move {
			let btc_network = match btc_network {
				BitcoinNetwork::Mainnet => Network::Bitcoin,
				BitcoinNetwork::Testnet => Network::Testnet,
				BitcoinNetwork::Regtest => Network::Regtest,
			};

			let client = BtcRpcClient::new(endpoint, None).unwrap().await;
			let mut interval = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL));
			interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
			let mut cache = Cache::default();
			loop {
				interval.tick().await;
				match update_cache(&client, cache.clone(), btc_network).await {
					Ok(updated_cache) => {
						cache = updated_cache;

						for query_result in cache.transactions.values() {
							store.save_singleton(query_result).await?;
						}
					},
					Err(err) => {
						error!("Error when querying Bitcoin chain: {}", err);
					},
				}
			}
		}
	});
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::anyhow;
	use bitcoin::{
		absolute::{Height, LockTime},
		address::{self},
		block::Version,
		hash_types::TxMerkleNode,
		hashes::Hash,
		secp256k1::rand::{self, Rng},
		Amount, TxOut,
	};
	use chainflip_engine::btc::rpc::{
		BlockHeader, Difficulty, VerboseBlock, VerboseTransaction, VerboseTxOut,
	};
	use std::collections::BTreeMap;

	#[derive(Clone)]
	struct MockBtcRpc {
		mempool: Vec<Transaction>,
		latest_block_hash: BlockHash,
		blocks: BTreeMap<BlockHash, VerboseBlock>,
	}

	#[async_trait::async_trait]
	impl BtcRpcApi for MockBtcRpc {
		async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock> {
			self.blocks.get(&block_hash).cloned().ok_or(anyhow!("Block missing"))
		}
		async fn best_block_hash(&self) -> anyhow::Result<BlockHash> {
			Ok(self.latest_block_hash)
		}
		async fn get_raw_mempool(&self) -> anyhow::Result<Vec<Txid>> {
			Ok(self.mempool.iter().map(|x| x.txid()).collect())
		}

		async fn get_raw_transactions(
			&self,
			tx_hashes: Vec<Txid>,
		) -> anyhow::Result<Vec<Transaction>> {
			let mut result: Vec<Transaction> = Default::default();
			for hash in tx_hashes {
				for tx in self.mempool.clone() {
					if tx.txid() == hash {
						result.push(tx)
					}
				}
			}
			Ok(result)
		}

		async fn block_hash(
			&self,
			_block_number: cf_chains::btc::BlockNumber,
		) -> anyhow::Result<BlockHash> {
			unimplemented!()
		}

		async fn send_raw_transaction(&self, _transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
			unimplemented!()
		}

		async fn next_block_fee_rate(&self) -> anyhow::Result<Option<cf_chains::btc::BtcAmount>> {
			unimplemented!()
		}

		async fn average_block_fee_rate(
			&self,
			_block_hash: BlockHash,
		) -> anyhow::Result<cf_chains::btc::BtcAmount> {
			unimplemented!()
		}

		async fn block_header(&self, _block_hash: BlockHash) -> anyhow::Result<BlockHeader> {
			unimplemented!()
		}
	}

	fn i_to_block_hash(i: u8) -> BlockHash {
		BlockHash::from_byte_array([i; 32])
	}

	fn header_with_prev_hash(i: u8) -> BlockHeader {
		let hash = i_to_block_hash(i + 1);
		BlockHeader {
			version: Version::from_consensus(0),
			previous_block_hash: Some(i_to_block_hash(i)),
			merkle_root: TxMerkleNode::from_byte_array([0u8; 32]),
			time: 0,
			bits: Default::default(),
			nonce: 0,
			hash,
			confirmations: 1,
			height: 2000,
			version_hex: Default::default(),
			median_time: Default::default(),
			difficulty: Difficulty::Number(1.0),
			chainwork: Default::default(),
			n_tx: Default::default(),
			next_block_hash: None,
			strippedsize: None,
			size: None,
			weight: None,
		}
	}

	fn init_blocks() -> BTreeMap<BlockHash, VerboseBlock> {
		let mut blocks: BTreeMap<BlockHash, VerboseBlock> = Default::default();
		for i in 1..16 {
			blocks.insert(
				i_to_block_hash(i),
				VerboseBlock { header: header_with_prev_hash(i - 1), txdata: vec![] },
			);
		}
		blocks
	}

	pub fn verbose_transaction(
		tx_outs: Vec<VerboseTxOut>,
		fee: Option<Amount>,
	) -> VerboseTransaction {
		let random_number: u8 = rand::thread_rng().gen();
		let txid = Txid::from_byte_array([random_number; 32]);
		VerboseTransaction {
			txid,
			locktime: LockTime::Blocks(Height::from_consensus(0).unwrap()),
			vin: vec![],
			vout: tx_outs,
			fee,
			// not important, we just need to set it to a value.
			hash: txid,
			size: Default::default(),
			vsize: Default::default(),
			weight: Default::default(),
			hex: Default::default(),
		}
	}

	pub fn verbose_vouts(vals_and_scripts: Vec<(u64, ScriptBuf)>) -> Vec<VerboseTxOut> {
		vals_and_scripts
			.into_iter()
			.enumerate()
			.map(|(n, (value, script_pub_key))| VerboseTxOut {
				value: Amount::from_sat(value),
				n: n as u64,
				script_pubkey: script_pub_key,
			})
			.collect()
	}

	// This creates one tx out in one transaction for each item in txdata
	fn block_prev_hash_tx_outs(i: u8, txdata: Vec<(Amount, String)>) -> VerboseBlock {
		VerboseBlock {
			header: header_with_prev_hash(i),
			txdata: txdata
				.into_iter()
				.map(|(value, destination)| {
					verbose_transaction(
						verbose_vouts(vec![(
							value.to_sat(),
							bitcoin::Address::from_str(&destination)
								.unwrap()
								.payload
								.script_pubkey(),
						)]),
						None,
					)
				})
				.collect(),
		}
	}

	fn tx_with_outs(tx_outs: Vec<TxOut>) -> Transaction {
		Transaction {
			output: tx_outs,
			version: 0,
			lock_time: LockTime::from_consensus(0),
			input: Default::default(),
		}
	}

	#[tokio::test]
	async fn multiple_outputs_in_one_tx() {
		let address1 = "3KhCRZchNv46uHwBXUZo4ALCUCjGT1v7fd".to_string();
		let address2 = "1F1tAaz5x1HUXrCNLbtMDqcw6o5GNn4xqX".to_string();

		let a1_script = address::Address::from_str(&address1).unwrap().payload.script_pubkey();
		let a2_script = address::Address::from_str(&address2).unwrap().payload.script_pubkey();

		let mempool = vec![tx_with_outs(vec![
			TxOut {
				value: Amount::from_btc(0.8).unwrap().to_sat(),
				script_pubkey: a1_script.clone(),
			},
			TxOut {
				value: Amount::from_btc(1.2).unwrap().to_sat(),
				script_pubkey: a2_script.clone(),
			},
		])];
		let blocks = init_blocks();
		let btc = MockBtcRpc { mempool, latest_block_hash: i_to_block_hash(15), blocks };
		let cache: Cache = Default::default();
		let cache = update_cache(&btc, cache, Network::Bitcoin).await.unwrap();
		assert_eq!(cache.transactions.len(), 2);
		assert!(cache.transactions.contains_key(&address1));
		assert!(cache.transactions.contains_key(&address2));
	}

	#[tokio::test]
	async fn mempool_updates() {
		let address1 = "3KhCRZchNv46uHwBXUZo4ALCUCjGT1v7fd".to_string();
		let address2 = "1F1tAaz5x1HUXrCNLbtMDqcw6o5GNn4xqX".to_string();
		let address3 = "bc1qrtwkf6jdda74ngjv6zgmxvx4jkckxkl2dafpm3".to_string();

		let a1_script = address::Address::from_str(&address1).unwrap().payload.script_pubkey();
		let a2_script = address::Address::from_str(&address2).unwrap().payload.script_pubkey();
		let a3_script = address::Address::from_str(&address3).unwrap().payload.script_pubkey();

		let mempool = vec![
			tx_with_outs(vec![TxOut {
				value: Amount::from_btc(0.8).unwrap().to_sat(),
				script_pubkey: a1_script.clone(),
			}]),
			tx_with_outs(vec![TxOut {
				value: Amount::from_btc(0.8).unwrap().to_sat(),
				script_pubkey: a2_script.clone(),
			}]),
		];
		let blocks = init_blocks();
		let mut rpc: MockBtcRpc =
			MockBtcRpc { mempool: mempool.clone(), latest_block_hash: i_to_block_hash(15), blocks };
		let cache: Cache = Default::default();
		let cache = update_cache(&rpc, cache.clone(), Network::Bitcoin).await.unwrap();
		assert_eq!(cache.transactions.len(), 2);
		assert!(cache.transactions.contains_key(&address1));
		assert!(cache.transactions.contains_key(&address2));

		rpc.mempool.append(&mut vec![tx_with_outs(vec![TxOut {
			value: Amount::from_btc(0.8).unwrap().to_sat(),
			script_pubkey: a3_script.clone(),
		}])]);

		let cache = update_cache(&rpc, cache.clone(), Network::Bitcoin).await.unwrap();
		assert_eq!(cache.transactions.len(), 3);
		assert!(cache.transactions.contains_key(&address1));
		assert!(cache.transactions.contains_key(&address2));
		assert!(cache.transactions.contains_key(&address3));

		rpc.mempool.remove(0);
		let cache = update_cache(&rpc, cache.clone(), Network::Bitcoin).await.unwrap();
		assert_eq!(cache.transactions.len(), 2);
		assert!(cache.transactions.contains_key(&address2));
		assert!(cache.transactions.contains_key(&address3));
	}

	#[tokio::test]
	async fn blocks() {
		let address1 = "bc1qrtwkf6jdda74ngjv6zgmxvx4jkckxkl2dafpm3".to_string();

		let mempool = vec![];

		let mut blocks: BTreeMap<BlockHash, VerboseBlock> = Default::default();
		for i in 1..19 {
			blocks.insert(i_to_block_hash(i), block_prev_hash_tx_outs(i - 1, vec![]));
		}

		blocks.insert(
			i_to_block_hash(15),
			block_prev_hash_tx_outs(14, vec![(Amount::from_btc(12.5).unwrap(), address1.clone())]),
		);
		let mut btc =
			MockBtcRpc { mempool: mempool.clone(), latest_block_hash: i_to_block_hash(15), blocks };
		let cache: Cache = Default::default();
		let cache = update_cache(&btc, cache.clone(), Network::Bitcoin).await.unwrap();
		assert_eq!(cache.transactions.get(&address1).unwrap().confirmations, 1);

		btc.latest_block_hash = i_to_block_hash(16);
		let cache = update_cache(&btc, cache.clone(), Network::Bitcoin).await.unwrap();
		assert_eq!(cache.transactions.get(&address1).unwrap().confirmations, 2);
	}

	#[tokio::test]
	async fn report_oldest_tx_only() {
		let address1 = "bc1qrtwkf6jdda74ngjv6zgmxvx4jkckxkl2dafpm3".to_string();
		let a1_script = address::Address::from_str(&address1).unwrap().payload.script_pubkey();

		let tx_value: Amount = Amount::from_btc(12.5).unwrap();

		let mempool = vec![tx_with_outs(vec![TxOut {
			value: Amount::from_btc(0.8).unwrap().to_sat(),
			script_pubkey: a1_script.clone(),
		}])];

		let mut blocks = init_blocks();

		blocks.insert(
			i_to_block_hash(13),
			block_prev_hash_tx_outs(12, vec![(tx_value, address1.clone())]),
		);

		let btc =
			MockBtcRpc { mempool: mempool.clone(), latest_block_hash: i_to_block_hash(15), blocks };
		let cache: Cache = Default::default();
		let cache = update_cache(&btc, cache.clone(), Network::Bitcoin).await.unwrap();
		assert_eq!(cache.transactions.get(&address1).unwrap().confirmations, 3);
		assert_eq!(cache.transactions.get(&address1).unwrap().value, tx_value.to_sat());
	}
}
