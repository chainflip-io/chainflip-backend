use crate::{
	btc::{
		retry_rpc::BtcRetryRpcClient,
		rpc::{BlockHeader, BtcRpcApi, MempoolInfo, MempoolTransaction, VerboseBlock},
	},
	caching_request::CachingRequest,
};
use bitcoin::{BlockHash, Txid};
use cf_chains::btc::{BlockNumber, BtcAmount};
use cf_utilities::task_scope::Scope;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct BtcCachingClient {
	uncached_client: BtcRetryRpcClient,

	block: CachingRequest<BlockHash, VerboseBlock, BtcRetryRpcClient>,
	block_hash: CachingRequest<BlockNumber, BlockHash, BtcRetryRpcClient>,
	send_raw_transaction: CachingRequest<Vec<u8>, Txid, BtcRetryRpcClient>,
	next_block_fee: CachingRequest<(), Option<BtcAmount>, BtcRetryRpcClient>,
	avg_fee_rate: CachingRequest<BlockHash, BtcAmount, BtcRetryRpcClient>,
	block_header: CachingRequest<BlockHash, BlockHeader, BtcRetryRpcClient>,
	best_block_hash: CachingRequest<(), BlockHash, BtcRetryRpcClient>,

	pub cache_invalidation_senders: Vec<mpsc::Sender<()>>,
}

impl BtcCachingClient {
	pub(crate) fn new(scope: &Scope<'_, anyhow::Error>, client: BtcRetryRpcClient) -> Self {
		let (block, block_cache) =
			CachingRequest::<BlockHash, VerboseBlock, BtcRetryRpcClient>::new(
				scope,
				client.clone(),
			);
		let (block_hash, block_hash_cache) =
			CachingRequest::<BlockNumber, BlockHash, BtcRetryRpcClient>::new(scope, client.clone());
		let (send_raw_transaction, send_raw_transaction_cache) =
			CachingRequest::<Vec<u8>, Txid, BtcRetryRpcClient>::new(scope, client.clone());
		let (next_block_fee, next_block_fee_cache) =
			CachingRequest::<(), Option<BtcAmount>, BtcRetryRpcClient>::new(scope, client.clone());
		let (avg_fee_rate, avg_fee_rate_cache) =
			CachingRequest::<BlockHash, BtcAmount, BtcRetryRpcClient>::new(scope, client.clone());
		let (block_header, block_header_cache) =
			CachingRequest::<BlockHash, BlockHeader, BtcRetryRpcClient>::new(scope, client.clone());
		let (best_block_hash, best_block_hash_cache) =
			CachingRequest::<(), BlockHash, BtcRetryRpcClient>::new(scope, client.clone());
		BtcCachingClient {
			uncached_client: client,
			block,
			block_hash,
			send_raw_transaction,
			next_block_fee,
			avg_fee_rate,
			block_header,
			best_block_hash,
			cache_invalidation_senders: vec![
				block_cache,
				block_hash_cache,
				send_raw_transaction_cache,
				next_block_fee_cache,
				avg_fee_rate_cache,
				block_header_cache,
				best_block_hash_cache,
			],
		}
	}
}

#[async_trait::async_trait]
impl BtcRpcApi for BtcCachingClient {
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock> {
		self.block
			.get_or_fetch(
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
			.get_or_fetch(
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
			.get_or_fetch(
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
			.get_or_fetch(
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
			.get_or_fetch(
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
			.get_or_fetch(
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
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_header(block_hash).await })
				}),
				block_hash,
			)
			.await
	}

	async fn mempool_info(&self) -> anyhow::Result<MempoolInfo> {
		self.uncached_client.mempool_info().await
	}

	async fn raw_mempool(&self) -> anyhow::Result<Vec<Txid>> {
		self.uncached_client.raw_mempool().await
	}

	async fn mempool_entries(&self, txids: Vec<Txid>) -> anyhow::Result<Vec<MempoolTransaction>> {
		self.uncached_client.mempool_entries(txids).await
	}
}
