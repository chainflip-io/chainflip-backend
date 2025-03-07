use crate::btc::rpc::{BlockHeader, BtcRpcApi, BtcRpcClient, VerboseBlock};
use bitcoin::{BlockHash, Txid};
use cf_chains::btc::{BlockNumber, BtcAmount};
use cf_utilities::{task_scope, task_scope::Scope};
use futures_util::FutureExt;
use std::{
	collections::{HashMap, HashSet},
	hash::Hash,
	sync::Arc,
};
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum RequestKey {
	BlockHash(u64),
	Block(BlockHash),
}

#[derive(Debug, Clone)]
enum ResponseValue {
	BlockHash(BlockHash),
	Block(VerboseBlock),
}
#[derive(Clone)]
pub struct BtcCachingClient {
	sender: mpsc::UnboundedSender<RequestKey>,
	client: BtcRpcClient,
	cache: Arc<RwLock<HashMap<RequestKey, ResponseValue>>>,
	in_flight: Arc<RwLock<HashSet<RequestKey>>>,
}

impl BtcCachingClient {
	pub async fn new(scope: &Scope<'_, anyhow::Error>, client: BtcRpcClient) -> Self {
		let (sender, mut receiver) = mpsc::unbounded_channel::<RequestKey>();
		let cache = Arc::new(RwLock::new(HashMap::new()));
		let in_flight = Arc::new(RwLock::new(HashSet::default()));

		scope.spawn({
			let client_copy = client.clone();
			let cache_copy = Arc::clone(&cache);
			let in_flight_copy = Arc::clone(&in_flight);
			task_scope::task_scope(|scope| {
				async move {
					while let Some(request) = receiver.recv().await {
						let client_copy = client_copy.clone();
						let cache_copy = Arc::clone(&cache_copy);
						let in_flight_copy = Arc::clone(&in_flight_copy);
						match request {
							RequestKey::BlockHash(number) => scope.spawn(async move {
								let block_hash = client_copy.block_hash(number).await;
								let response = ResponseValue::BlockHash(
									block_hash.expect("Missing blockhash"),
								);

								{
									let mut cache = cache_copy.write().await;
									cache.insert(request, response);
								}
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								Ok(())
							}),
							RequestKey::Block(hash) => scope.spawn(async move {
								let block = client_copy.block(hash).await?;
								let response = ResponseValue::Block(block);

								{
									let mut cache = cache_copy.write().await;
									cache.insert(request, response);
								}
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								Ok(())
							}),
						}
					}
					Ok(())
				}
				.boxed()
			})
		});

		Self { sender, client, cache, in_flight }
	}

	async fn get(&self, key: RequestKey) -> Result<ResponseValue, anyhow::Error> {
		{
			let cache = self.cache.read().await;
			if let Some(value) = cache.get(&key) {
				return Ok(value.clone());
			}
		}

		let should_create_request = {
			let in_flight = self.in_flight.read().await;
			!in_flight.contains(&key)
		};

		if should_create_request {
			let _ = self.sender.send(key);

			// Now add it to in_flight with a separate lock acquisition
			let mut in_flight = self.in_flight.write().await;

			// Double-check that no other thread added it while we were creating the future
			if !in_flight.contains(&key) {
				in_flight.insert(key);
			}
		}

		// Wait for the result
		self.wait_for_result(key).await
	}

	async fn wait_for_result(&self, key: RequestKey) -> Result<ResponseValue, anyhow::Error> {
		loop {
			// Check if the result is already in cache
			{
				let cache_read = self.cache.read().await;
				if let Some(value) = cache_read.get(&key) {
					return Ok(value.clone());
				}
			}

			// Check if we need to continue waiting
			let still_in_flight = {
				let in_flight_read = self.in_flight.read().await;
				in_flight_read.contains(&key)
			};

			if !still_in_flight {
				// The request is no longer in-flight, but the result isn't in the cache.
				return Err(anyhow::anyhow!("Request completed but result not in cache"));
			}

			// The request is still in-flight, yield to allow it to progress
			tokio::task::yield_now().await;
		}
	}

	pub async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock> {
		let key = RequestKey::Block(block_hash);
		match self.get(key).await? {
			ResponseValue::Block(block) => Ok(block),
			_ => Err(anyhow::anyhow!("Unexpected response type")),
		}
	}

	pub async fn block_hash(&self, block_number: BlockNumber) -> BlockHash {
		let key = RequestKey::BlockHash(block_number);
		match self.get(key).await.expect("Failed to get block hash") {
			ResponseValue::BlockHash(hash) => hash,
			_ => panic!("Unexpected response type"),
		}
	}

	// Directly pass through non-cached methods to the underlying client
	pub async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
		self.client.send_raw_transaction(transaction_bytes).await
	}

	pub async fn next_block_fee_rate(&self) -> Option<BtcAmount> {
		self.client.next_block_fee_rate().await.expect("No fee available")
	}

	pub async fn average_block_fee_rate(&self, block_hash: BlockHash) -> BtcAmount {
		self.client.average_block_fee_rate(block_hash).await.expect("No fee available")
	}

	pub async fn best_block_header(&self) -> anyhow::Result<BlockHeader> {
		let best_block_hash = self.client.best_block_hash().await?;
		self.client.block_header(best_block_hash).await
	}

	pub async fn block_header(&self, block_number: BlockNumber) -> anyhow::Result<BlockHeader> {
		let hash = self.client.block_hash(block_number).await.expect("No blockHash");
		self.client.block_header(hash).await
	}
}
