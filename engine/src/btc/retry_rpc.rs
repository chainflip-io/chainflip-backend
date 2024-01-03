use bitcoin::{BlockHash, Txid};
use utilities::task_scope::Scope;

use crate::{
	retrier::{Attempt, RequestLog, RetrierClient},
	settings::{HttpBasicAuthEndpoint, NodeContainer},
	witness::common::chain_source::{ChainClient, Header},
};
use cf_chains::{btc::BitcoinNetwork, Bitcoin};
use core::time::Duration;

use anyhow::Result;

use super::rpc::{BlockHeader, BtcRpcApi, BtcRpcClient, VerboseBlock};

#[derive(Clone)]
pub struct BtcRetryRpcClient {
	retry_client: RetrierClient<BtcRpcClient>,
}

const BITCOIN_RPC_TIMEOUT: Duration = Duration::from_millis(225);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

const MAX_BROADCAST_RETRIES: Attempt = 2;

impl BtcRetryRpcClient {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<HttpBasicAuthEndpoint>,
		expected_btc_network: BitcoinNetwork,
	) -> Result<Self> {
		let rpc_client = BtcRpcClient::new(nodes.primary, Some(expected_btc_network))?;

		let backup_rpc_client = nodes
			.backup
			.map(|backup_endpoint| BtcRpcClient::new(backup_endpoint, Some(expected_btc_network)))
			.transpose()?;

		Ok(Self {
			retry_client: RetrierClient::new(
				scope,
				"btc_rpc",
				rpc_client,
				backup_rpc_client,
				BITCOIN_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
		})
	}
}

#[async_trait::async_trait]
pub trait BtcRetryRpcApi: Clone {
	async fn block(&self, block_hash: BlockHash) -> VerboseBlock;

	async fn block_hash(&self, block_number: cf_chains::btc::BlockNumber) -> BlockHash;

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid>;

	async fn next_block_fee_rate(&self) -> Option<cf_chains::btc::BtcAmount>;

	async fn average_block_fee_rate(&self, block_hash: BlockHash) -> cf_chains::btc::BtcAmount;

	async fn best_block_header(&self) -> BlockHeader;
}

#[async_trait::async_trait]
impl BtcRetryRpcApi for BtcRetryRpcClient {
	async fn block(&self, block_hash: BlockHash) -> VerboseBlock {
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
				MAX_BROADCAST_RETRIES,
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
impl ChainClient for BtcRetryRpcClient {
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

#[cfg(test)]
pub mod mocks {

	use super::*;
	use mockall::mock;

	mock! {
		pub BtcRetryRpcClient {}

		impl Clone for BtcRetryRpcClient {
			fn clone(&self) -> Self;
		}

		#[async_trait::async_trait]
		impl BtcRetryRpcApi for BtcRetryRpcClient {
			async fn block(&self, block_hash: BlockHash) -> VerboseBlock;

			async fn block_hash(&self, block_number: cf_chains::btc::BlockNumber) -> BlockHash;

			async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid>;

			async fn next_block_fee_rate(&self) -> Option<cf_chains::btc::BtcAmount>;

			async fn average_block_fee_rate(&self, block_hash: BlockHash) -> cf_chains::btc::BtcAmount;

			async fn best_block_header(&self) -> BlockHeader;
		}
	}
}

#[cfg(test)]
mod tests {
	use utilities::testing::logging::init_test_logger;

	use super::*;

	#[tokio::test]
	#[ignore = "requires local node, useful for manual testing"]
	async fn test_btc_retrier() {
		init_test_logger();

		use futures::FutureExt;
		use utilities::task_scope::task_scope;

		task_scope(|scope| {
			async move {
				let retry_client = BtcRetryRpcClient::new(
					scope,
					NodeContainer::<HttpBasicAuthEndpoint> {
						primary: HttpBasicAuthEndpoint {
							http_endpoint: "https://btc-mainnet.euc1-rpc:443".into(),
							basic_auth_user: "flip".to_string(),
							basic_auth_password: "44c55233c95d59345059f2ce839042ef".to_string(),
						},
						backup: None,
					},
					BitcoinNetwork::Mainnet,
				)
				.await
				.unwrap();

				let rpc_client = BtcRpcClient::new(
					HttpBasicAuthEndpoint {
						http_endpoint: "https://btc-mainnet.euc1-rpc:443".into(),
						basic_auth_user: "flip".to_string(),
						basic_auth_password: "44c55233c95d59345059f2ce839042ef".to_string(),
					},
					None,
				)
				.unwrap()
				.await;

				for i in 822000..824150 {
					// let best_header = retry_client.best_block_header().await;
					let hash = retry_client.block_hash(i).await;
					// let header = rpc_client.block_header(hash).await.unwrap();

					match tokio::time::timeout(BITCOIN_RPC_TIMEOUT, rpc_client.block_header(hash))
						.await
					{
						Ok(res_header) => match res_header {
							Ok(header) => {
								println!("Header: {:?}", header)
							},
							Err(e) => {
								panic!("Error in response {}!", e);
							},
						},
						Err(_) => {
							panic!("Timed out!");
						},
					}
				}

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
