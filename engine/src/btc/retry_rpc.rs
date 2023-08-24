use bitcoin::{Block, BlockHash, Txid};
use futures_core::Future;
use utilities::task_scope::Scope;

use crate::{
	retrier::{RequestLog, RetrierClient},
	witness::common::chain_source::{ChainClient, Header},
};
use cf_chains::Bitcoin;
use core::time::Duration;

use super::rpc::{BlockHeader, BtcRpcApi, BtcRpcClient};

pub struct BtcRetryRpcClient<BtcRpcClientFut: Future<Output = BtcRpcClient> + Send + 'static> {
	retry_client: RetrierClient<BtcRpcClientFut, BtcRpcClient>,
}

impl<BtcRpcClientFut: Future<Output = BtcRpcClient> + Send + 'static> Clone
	for BtcRetryRpcClient<BtcRpcClientFut>
{
	fn clone(&self) -> Self {
		Self { retry_client: self.retry_client.clone() }
	}
}

const BITCOIN_RPC_TIMEOUT: Duration = Duration::from_millis(2000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

impl<BtcRpcClientFut: Future<Output = BtcRpcClient> + Send + 'static>
	BtcRetryRpcClient<BtcRpcClientFut>
{
	pub fn new(scope: &Scope<'_, anyhow::Error>, btc_client: BtcRpcClientFut) -> Self {
		Self {
			retry_client: RetrierClient::new(
				scope,
				"btc_rpc",
				btc_client,
				BITCOIN_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
		}
	}
}

#[async_trait::async_trait]
pub trait BtcRetryRpcApi: Clone {
	async fn block(&self, block_hash: BlockHash) -> Block;

	async fn block_hash(&self, block_number: cf_chains::btc::BlockNumber) -> BlockHash;

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid>;

	async fn next_block_fee_rate(&self) -> Option<cf_chains::btc::BtcAmount>;

	async fn average_block_fee_rate(&self, block_hash: BlockHash) -> cf_chains::btc::BtcAmount;

	async fn best_block_header(&self) -> BlockHeader;
}

#[async_trait::async_trait]
impl<BtcRpcClientFut: Future<Output = BtcRpcClient> + Send + 'static> BtcRetryRpcApi
	for BtcRetryRpcClient<BtcRpcClientFut>
{
	async fn block(&self, block_hash: BlockHash) -> Block {
		self.retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block(block_hash).await })
				}),
				RequestLog::new("block".to_string(), Some(format!("{block_hash}"))),
			)
			.await
	}

	async fn block_hash(&self, block_number: cf_chains::btc::BlockNumber) -> BlockHash {
		self.retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_hash(block_number).await })
				}),
				RequestLog::new("block_hash".to_string(), Some(format!("{block_number}"))),
			)
			.await
	}

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
		let log = RequestLog::new(
			"send_raw_transaction".to_string(),
			Some(format!("{transaction_bytes:?}")),
		);
		self.retry_client
			.request_with_limit(
				Box::pin(move |client| {
					let transaction_bytes = transaction_bytes.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.send_raw_transaction(transaction_bytes).await })
				}),
				log,
				5,
			)
			.await
	}

	async fn next_block_fee_rate(&self) -> Option<cf_chains::btc::BtcAmount> {
		self.retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.next_block_fee_rate().await })
				}),
				RequestLog::new("next_block_fee_rate".to_string(), None),
			)
			.await
	}

	async fn average_block_fee_rate(&self, block_hash: BlockHash) -> cf_chains::btc::BtcAmount {
		self.retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.average_block_fee_rate(block_hash).await })
				}),
				RequestLog::new(
					"average_block_fee_rate".to_string(),
					Some(format!("{block_hash}")),
				),
			)
			.await
	}

	async fn best_block_header(&self) -> BlockHeader {
		self.retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						let best_block_hash = client.best_block_hash().await?;
						let header = client.block_header(best_block_hash).await?;
						assert_eq!(header.hash, best_block_hash);
						Ok(header)
					})
				}),
				RequestLog::new("best_block_header".to_string(), None),
			)
			.await
	}
}

#[async_trait::async_trait]
impl<BtcRpcClientFut: Future<Output = BtcRpcClient> + Send + 'static> ChainClient
	for BtcRetryRpcClient<BtcRpcClientFut>
{
	type Index = <Bitcoin as cf_chains::Chain>::ChainBlockNumber;
	type Hash = BlockHash;
	type Data = ();

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.retry_client
			.request(
				Box::pin(move |client| {
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
				}),
				RequestLog::new("header_at_index".to_string(), Some(format!("{index}"))),
			)
			.await
	}
}
