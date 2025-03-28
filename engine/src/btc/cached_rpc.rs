use crate::{
	btc::{
		retry_rpc::BtcRetryRpcClient,
		rpc::{BlockHeader, BtcRpcApi, VerboseBlock},
	},
	caching_client::CachingClient,
};
use bitcoin::{BlockHash, Txid};
use cf_chains::btc::{BlockNumber, BtcAmount};
use cf_utilities::{loop_select, task_scope::Scope};
use futures::FutureExt;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct BtcCachingClient {
	block: CachingClient<BlockHash, VerboseBlock, BtcRetryRpcClient>,
	block_hash: CachingClient<BlockNumber, BlockHash, BtcRetryRpcClient>,
	send_raw_transaction: CachingClient<Vec<u8>, Txid, BtcRetryRpcClient>,
	next_block_fee: CachingClient<(), Option<BtcAmount>, BtcRetryRpcClient>,
	avg_fee_rate: CachingClient<BlockHash, BtcAmount, BtcRetryRpcClient>,
	block_header: CachingClient<BlockHash, BlockHeader, BtcRetryRpcClient>,
	best_block_hash: CachingClient<(), BlockHash, BtcRetryRpcClient>,

	pub cache_invalidation_sender: mpsc::Sender<()>,
}

impl BtcCachingClient {
	pub(crate) fn new(scope: &Scope<'_, anyhow::Error>, client: BtcRetryRpcClient) -> Self {
		let (cache_invalidation_sender, mut cache_invalidation_receiver) = mpsc::channel::<()>(1);

		let cached_client = BtcCachingClient {
			block: CachingClient::<BlockHash, VerboseBlock, BtcRetryRpcClient>::new(
				scope,
				client.clone(),
			),
			block_hash: CachingClient::<BlockNumber, BlockHash, BtcRetryRpcClient>::new(
				scope,
				client.clone(),
			),
			send_raw_transaction: CachingClient::<Vec<u8>, Txid, BtcRetryRpcClient>::new(
				scope,
				client.clone(),
			),
			next_block_fee: CachingClient::<(), Option<BtcAmount>, BtcRetryRpcClient>::new(
				scope,
				client.clone(),
			),
			avg_fee_rate: CachingClient::<BlockHash, BtcAmount, BtcRetryRpcClient>::new(
				scope,
				client.clone(),
			),
			block_header: CachingClient::<BlockHash, BlockHeader, BtcRetryRpcClient>::new(
				scope,
				client.clone(),
			),
			best_block_hash: CachingClient::<(), BlockHash, BtcRetryRpcClient>::new(scope, client),
			cache_invalidation_sender,
		};
		let client = cached_client.clone();
		scope.spawn(
			async move {
				loop_select!(
					if let Some(_) = cache_invalidation_receiver.recv() => {
						client.clear_cache().await;
					},
				);
				Ok(())
			}
			.boxed(),
		);
		cached_client
	}

	async fn clear_cache(&self) {
		self.block.cache_invalidation_sender.send(()).await.unwrap();
		self.block_hash.cache_invalidation_sender.send(()).await.unwrap();
		self.send_raw_transaction.cache_invalidation_sender.send(()).await.unwrap();
		self.next_block_fee.cache_invalidation_sender.send(()).await.unwrap();
		self.avg_fee_rate.cache_invalidation_sender.send(()).await.unwrap();
		self.block_header.cache_invalidation_sender.send(()).await.unwrap();
		self.best_block_hash.cache_invalidation_sender.send(()).await.unwrap();
	}
}

#[async_trait::async_trait]
impl BtcRpcApi for BtcCachingClient {
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock> {
		self.block
			.get(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block(block_hash).await })
				}),
				block_hash,
			)
			.await
	}

	async fn block_hash(&self, block_number: BlockNumber) -> anyhow::Result<BlockHash> {
		self.block_hash
			.get(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_hash(block_number).await })
				}),
				block_number,
			)
			.await
	}

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
		let transaction_bytess = transaction_bytes.clone();
		self.send_raw_transaction
			.get(
				Box::pin(move |client| {
					let transaction_bytes = transaction_bytes.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.send_raw_transaction(transaction_bytes).await })
				}),
				transaction_bytess,
			)
			.await
	}

	async fn next_block_fee_rate(&self) -> anyhow::Result<Option<BtcAmount>> {
		self.next_block_fee
			.get(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.next_block_fee_rate().await })
				}),
				(),
			)
			.await
	}

	async fn average_block_fee_rate(&self, block_hash: BlockHash) -> anyhow::Result<BtcAmount> {
		self.avg_fee_rate
			.get(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.average_block_fee_rate(block_hash).await })
				}),
				block_hash,
			)
			.await
	}

	async fn best_block_hash(&self) -> anyhow::Result<BlockHash> {
		self.best_block_hash
			.get(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.best_block_hash().await })
				}),
				(),
			)
			.await
	}

	async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader> {
		self.block_header
			.get(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_header(block_hash).await })
				}),
				block_hash,
			)
			.await
	}
}
