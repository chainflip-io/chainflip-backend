use bitcoin::{Block, BlockHash, Txid};
use utilities::task_scope::Scope;

use crate::{
	rpc_retrier::RpcRetrierClient,
	witness::chain_source::{ChainClient, Header},
};
use core::time::Duration;

use super::rpc::{BlockHeader, BtcRpcApi, BtcRpcClient};

pub struct BtcRetryRpcClient {
	retry_client: RpcRetrierClient<BtcRpcClient>,
}

const BITCOIN_RPC_TIMEOUT: Duration = Duration::from_millis(1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

impl BtcRetryRpcClient {
	pub fn new(scope: &Scope<'_, anyhow::Error>, btc_client: BtcRpcClient) -> Self {
		Self {
			retry_client: RpcRetrierClient::new(
				scope,
				btc_client,
				BITCOIN_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
		}
	}
}

#[async_trait::async_trait]
pub trait BtcRetryRpcApi {
	async fn block(&self, block_hash: BlockHash) -> Block;

	async fn block_hash(&self, block_number: cf_chains::btc::BlockNumber) -> BlockHash;

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> Txid;

	async fn next_block_fee_rate(&self) -> Option<cf_chains::btc::BtcAmount>;

	async fn best_block_hash(&self) -> BlockHash;

	async fn block_header(&self, block_hash: BlockHash) -> BlockHeader;
}

#[async_trait::async_trait]
impl BtcRetryRpcApi for BtcRetryRpcClient {
	async fn block(&self, block_hash: BlockHash) -> Block {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block(block_hash).await })
			}))
			.await
	}

	async fn block_hash(&self, block_number: cf_chains::btc::BlockNumber) -> BlockHash {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block_hash(block_number).await })
			}))
			.await
	}

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> Txid {
		self.retry_client
			.request(Box::pin(move |client| {
				let transaction_bytes = transaction_bytes.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.send_raw_transaction(transaction_bytes).await })
			}))
			.await
	}

	async fn next_block_fee_rate(&self) -> Option<cf_chains::btc::BtcAmount> {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.next_block_fee_rate().await })
			}))
			.await
	}

	async fn best_block_hash(&self) -> BlockHash {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.best_block_hash().await })
			}))
			.await
	}

	async fn block_header(&self, block_hash: BlockHash) -> BlockHeader {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block_header(block_hash).await })
			}))
			.await
	}
}

#[async_trait::async_trait]
impl ChainClient for BtcRetryRpcClient {
	type Index = u64;

	type Hash = BlockHash;

	type Data = ();

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move {
					let block_hash = client.block_hash(index).await?;
					let block_header = client.block_header(block_hash).await?;
					assert_eq!(block_header.height, index);

					Ok(Header {
						index,
						hash: block_hash,
						parent_hash: block_header.previous_block_hash,
						data: (),
					})
				})
			}))
			.await
	}
}
