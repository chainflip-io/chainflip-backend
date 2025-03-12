use crate::btc::rpc::{BlockHeader, BtcRpcApi, VerboseBlock};
use bitcoin::{BlockHash, Txid};
use cf_chains::btc::{BlockNumber, BtcAmount};
use cf_utilities::{task_scope, task_scope::Scope};
use futures_util::FutureExt;
use std::{collections::HashMap, hash::Hash, marker::PhantomData, sync::Arc};
use tokio::sync::{broadcast, mpsc, RwLock};
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RequestKey {
	BlockHash(u64),
	Block(BlockHash),
	SendRawTransaction(Vec<u8>),
	NextBlockFeeRate(),
	AvgBlockFeeRate(BlockHash),
	BlockHeader(BlockHash),
	BestBlockHash(),
}

#[derive(Debug, Clone)]
pub enum RequestValue {
	BlockHash(BlockHash),
	Block(VerboseBlock),
	SendRawTransaction(Txid),
	NextBlockFeeRate(Option<BtcAmount>),
	AvgBlockFeeRate(BtcAmount),
	BlockHeader(BlockHeader),
	BestBlockHash(BlockHash),
}
#[derive(Clone)]
pub struct BtcCachingClient<T>
where
	T: BtcRpcApi + Clone + Send + Sync + 'static,
{
	sender: mpsc::UnboundedSender<(RequestKey, broadcast::Sender<RequestValue>)>,
	pub cache: Arc<RwLock<HashMap<RequestKey, RequestValue>>>,
	in_flight: Arc<RwLock<HashMap<RequestKey, broadcast::Receiver<RequestValue>>>>,
	_phantom_data: PhantomData<T>,
}

impl<Client: BtcRpcApi + Clone + Send + Sync + 'static> BtcCachingClient<Client> {
	pub async fn new(scope: &Scope<'_, anyhow::Error>, client: Client) -> Self {
		let (sender, mut receiver) =
			mpsc::unbounded_channel::<(RequestKey, broadcast::Sender<RequestValue>)>();
		let cache = Arc::new(RwLock::new(HashMap::new()));
		let in_flight = Arc::new(RwLock::new(HashMap::default()));

		let cache_copy = Arc::clone(&cache);
		let in_flight_copy = Arc::clone(&in_flight);
		scope.spawn({
			task_scope::task_scope(|scope| {
				async move {
					while let Some((request, sender)) = receiver.recv().await {
						let client = client.clone();

						let cache_copy = Arc::clone(&cache_copy);
						let in_flight_copy = Arc::clone(&in_flight_copy);
						match request {
							RequestKey::BlockHash(number) => scope.spawn(async move {
								let block_hash = client.block_hash(number).await;
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								if block_hash.is_ok() {
									let response = RequestValue::BlockHash(block_hash.unwrap());
									{
										let mut cache = cache_copy.write().await;
										cache.insert(request, response.clone());
									}
									let _ = sender.send(response);
								}
								Ok(())
							}),
							RequestKey::Block(hash) => scope.spawn(async move {
								let block = client.block(hash).await;
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								if block.is_ok() {
									let response = RequestValue::Block(block.unwrap());
									{
										let mut cache = cache_copy.write().await;
										cache.insert(request, response.clone());
									}
									let _ = sender.send(response);
								}
								Ok(())
							}),
							//TODO! fix how we handle this case which contains Vec<_> which doesn't
							// impl Copy
							RequestKey::SendRawTransaction(_) => scope.spawn(async move {
								let tx = match request.clone() {
									RequestKey::SendRawTransaction(tx) => tx,
									_ => {
										vec![]
									},
								};
								let tx_id = client.send_raw_transaction(tx.clone()).await;
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								if tx_id.is_ok() {
									let response = RequestValue::SendRawTransaction(tx_id.unwrap());
									{
										let mut cache = cache_copy.write().await;
										cache.insert(request.clone(), response.clone());
									}
									let _ = sender.send(response);
								}
								Ok(())
							}),
							RequestKey::NextBlockFeeRate() => scope.spawn(async move {
								let fee = client.next_block_fee_rate().await;
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								if fee.is_ok() {
									let response = RequestValue::NextBlockFeeRate(fee.unwrap());
									{
										let mut cache = cache_copy.write().await;
										cache.insert(request, response.clone());
									}
									let _ = sender.send(response);
								}
								Ok(())
							}),
							RequestKey::AvgBlockFeeRate(hash) => scope.spawn(async move {
								let fee = client.average_block_fee_rate(hash).await;
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								if fee.is_ok() {
									let response = RequestValue::AvgBlockFeeRate(fee.unwrap());
									{
										let mut cache = cache_copy.write().await;
										cache.insert(request, response.clone());
									}
									let _ = sender.send(response);
								}
								Ok(())
							}),
							RequestKey::BlockHeader(hash) => scope.spawn(async move {
								let header = client.block_header(hash).await;
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								if header.is_ok() {
									let response = RequestValue::BlockHeader(header.unwrap());
									{
										let mut cache = cache_copy.write().await;
										cache.insert(request, response.clone());
									}
									let _ = sender.send(response);
								}
								Ok(())
							}),
							RequestKey::BestBlockHash() => scope.spawn(async move {
								let hash = client.best_block_hash().await;
								{
									let mut in_flight = in_flight_copy.write().await;
									in_flight.remove(&request);
								}
								if hash.is_ok() {
									let response = RequestValue::BestBlockHash(hash.unwrap());
									{
										let mut cache = cache_copy.write().await;
										cache.insert(request, response.clone());
									}
									let _ = sender.send(response);
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

		Self { sender, cache, in_flight, _phantom_data: Default::default() }
	}

	async fn get(&self, key: RequestKey) -> Result<RequestValue, anyhow::Error> {
		{
			let cache = self.cache.read().await;
			if let Some(value) = cache.get(&key) {
				return Ok(value.clone());
			}
		}
		let mut receiver = {
			let mut in_flight = self.in_flight.write().await;
			if in_flight.contains_key(&key) {
				in_flight.get(&key).unwrap().resubscribe()
			} else {
				let (tx, rx) = broadcast::channel(1);
				in_flight.insert(key.clone(), tx.subscribe());
				let _ = self.sender.send((key, tx));

				rx
			}
		};
		receiver.recv().await.map_err(|err| anyhow::Error::new(err))
	}
	pub async fn best_block_header(&self) -> anyhow::Result<BlockHeader> {
		let best_block_hash = self.best_block_hash().await?;
		self.block_header(best_block_hash).await
	}
}

#[async_trait::async_trait]
impl<Client: BtcRpcApi + Clone + Send + Sync + 'static> BtcRpcApi for BtcCachingClient<Client> {
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock> {
		let key = RequestKey::Block(block_hash);
		match self.get(key).await? {
			RequestValue::Block(block) => Ok(block),
			_ => Err(anyhow::anyhow!("Unexpected response type")),
		}
	}

	async fn block_hash(&self, block_number: BlockNumber) -> anyhow::Result<BlockHash> {
		let key = RequestKey::BlockHash(block_number);
		match self.get(key).await? {
			RequestValue::BlockHash(hash) => Ok(hash),
			_ => Err(anyhow::anyhow!("Unexpected response type")),
		}
	}

	// Directly pass through non-cached methods to the underlying client
	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
		let key = RequestKey::SendRawTransaction(transaction_bytes);
		match self.get(key).await? {
			RequestValue::SendRawTransaction(hash) => Ok(hash),
			_ => Err(anyhow::anyhow!("Unexpected response type")),
		}
		// self.client.send_raw_transaction(transaction_bytes).await
	}

	async fn next_block_fee_rate(&self) -> anyhow::Result<Option<BtcAmount>> {
		let key = RequestKey::NextBlockFeeRate();
		match self.get(key).await? {
			RequestValue::NextBlockFeeRate(fee) => Ok(fee),
			_ => Err(anyhow::anyhow!("Unexpected response type")),
		}
		// self.client.next_block_fee_rate().await.expect("No fee available")
	}

	async fn average_block_fee_rate(&self, block_hash: BlockHash) -> anyhow::Result<BtcAmount> {
		let key = RequestKey::AvgBlockFeeRate(block_hash);
		match self.get(key).await? {
			RequestValue::AvgBlockFeeRate(fee) => Ok(fee),
			_ => Err(anyhow::anyhow!("Unexpected response type")),
		}
		// self.client.average_block_fee_rate(block_hash).await
	}

	async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader> {
		let key = RequestKey::BlockHeader(block_hash);
		match self.get(key).await? {
			RequestValue::BlockHeader(header) => Ok(header),
			_ => Err(anyhow::anyhow!("Unexpected response type")),
		}
		// self.client.block_header(block_hash).await
	}

	async fn best_block_hash(&self) -> anyhow::Result<BlockHash> {
		let key = RequestKey::BestBlockHash();
		match self.get(key).await? {
			RequestValue::BestBlockHash(hash) => Ok(hash),
			_ => Err(anyhow::anyhow!("Unexpected response type")),
		}
		// Ok(self.client.best_block_hash().await?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use async_trait::async_trait;
	use bitcoin::{hashes::Hash, BlockHash};
	use cf_utilities::task_scope;
	use mockall::{mock, predicate::*};

	// Mock BtcRpcClient
	mock! {
		pub BtcRpcClient {}

		#[async_trait]
		impl BtcRpcApi for BtcRpcClient {
			async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock>;
			async fn block_hash(&self, block_number: BlockNumber) -> anyhow::Result<BlockHash>;
			async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid>;
			async fn next_block_fee_rate(&self) -> anyhow::Result<Option<BtcAmount>>;
			async fn average_block_fee_rate(&self, block_hash: BlockHash) -> anyhow::Result<BtcAmount>;
			async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader>;
			async fn best_block_hash(&self) -> anyhow::Result<BlockHash>;
		}

		impl Clone for BtcRpcClient {
			fn clone(&self) -> Self;
		}
	}

	#[tokio::test]
	async fn test_cache_hit() {
		let mut mock_rpc = MockBtcRpcClient::new();

		let block_hash = BlockHash::from_byte_array([0; 32]);
		let block_number = 100u64;

		// NB! every time we receive a new request we clone the client an pass it to the future
		// the cloned mock doesn't share state or expectation so we need to set the expectation for
		// the cloned version as well which is very annoying since we cannot accurately measure
		// how many times a function was called across different copy of the same client
		// A possible workaround is to expect clone() to be called a fixed number of times (same as
		// the requests we perform, if we hit the cache no clone() is called) Still this type of
		// testing means if we change the implementation we need to update the tests which is not
		// what we want!!!
		mock_rpc.expect_clone().times(1).returning(move || {
			let mut copy = MockBtcRpcClient::new();
			copy.expect_block_hash()
				.with(eq(block_number))
				.times(1) // Should only be called once
				.returning(move |_| Ok(block_hash));
			copy
		});

		let _ = task_scope::task_scope(|scope| {
			async move {
				let client = BtcCachingClient::new(&scope, mock_rpc).await;

				// First request (misses cache, should call mock)
				let res1 = client.block_hash(100).await.unwrap();
				assert_eq!(res1, block_hash);

				// Second request (should hit cache, so no RPC call)
				let res2 = client.block_hash(100).await.unwrap();
				assert_eq!(res2, block_hash);
				Ok(())
			}
			.boxed()
		})
		.await;
	}

	#[tokio::test]
	async fn test_cache_clear() {
		let mut mock_rpc = MockBtcRpcClient::new();
		let block_hash = BlockHash::from_slice(&[2; 32]).unwrap();
		let block_number = 100u64;

		mock_rpc.expect_clone().times(2).returning(move || {
			let mut copy = MockBtcRpcClient::new();
			copy.expect_block_hash()
				.with(eq(block_number))
				.times(1) // Should only be called once
				.returning(move |_| Ok(block_hash));
			copy
		});

		let _ = task_scope::task_scope(|scope| {
			async move {
				let client = BtcCachingClient::new(&scope, mock_rpc).await;
				// Populate cache
				client.block_hash(block_number).await.unwrap();
				{
					let cache = client.cache.read().await;
					assert!(cache.contains_key(&RequestKey::BlockHash(block_number)));
				}

				// Clear cache
				{
					let mut cache = client.cache.write().await;
					cache.clear();
					assert!(cache.is_empty());
				}
				client.block_hash(block_number).await.unwrap();

				Ok(())
			}
			.boxed()
		})
		.await;
	}
}
