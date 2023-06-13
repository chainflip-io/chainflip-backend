use bitcoin::{Block, BlockHash, Txid};
use utilities::task_scope::Scope;

use crate::rpc_retrier::RpcRetrierClient;
use core::time::Duration;

use super::rpc::{BtcRpcApi, BtcRpcClient};

pub struct BtcRetryRpcClient {
	retry_client: RpcRetrierClient<BtcRpcClient>,
}

const BITCOIN_RPC_TIMEOUT: Duration = Duration::from_millis(1000);

impl BtcRetryRpcClient {
	pub fn new(scope: &Scope<'_, anyhow::Error>, btc_client: BtcRpcClient) -> Self {
		Self { retry_client: RpcRetrierClient::new(scope, btc_client, BITCOIN_RPC_TIMEOUT) }
	}
}

#[async_trait::async_trait]
pub trait BtcRetryRpcApi {
	async fn block(&self, block_hash: BlockHash) -> Block;

	async fn block_hash(&self, block_number: cf_chains::btc::BlockNumber) -> BlockHash;

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> Txid;

	async fn next_block_fee_rate(&self) -> Option<cf_chains::btc::BtcAmount>;
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
}
