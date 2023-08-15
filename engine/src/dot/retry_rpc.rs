use crate::{
	dot::PolkadotConfig,
	witness::common::chain_source::{ChainClient, Header},
};
use cf_chains::{
	dot::{PolkadotHash, RuntimeVersion},
	Polkadot,
};
use cf_primitives::PolkadotBlockNumber;
use core::time::Duration;
use futures_core::Stream;
use sp_core::H256;
use std::pin::Pin;
use subxt::{config::Header as SubxtHeader, events::Events, rpc::types::ChainBlockExtrinsic};
use utilities::task_scope::Scope;

use crate::retrier::{RequestLog, RetrierClient};

use super::{
	http_rpc::DotHttpRpcClient,
	rpc::{DotSubClient, PolkadotHeader},
};

use crate::dot::rpc::DotRpcApi;

#[derive(Clone)]
pub struct DotRetryRpcClient {
	rpc_retry_client: RetrierClient<DotHttpRpcClient>,
	sub_retry_client: RetrierClient<DotSubClient>,
}

const POLKADOT_RPC_TIMEOUT: Duration = Duration::from_millis(2000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 20;

impl DotRetryRpcClient {
	pub fn new(
		scope: &Scope<'_, anyhow::Error>,
		dot_rpc_client: DotHttpRpcClient,
		dot_sub_client: DotSubClient,
	) -> Self {
		Self {
			rpc_retry_client: RetrierClient::new(
				scope,
				"dot_rpc",
				dot_rpc_client,
				POLKADOT_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			sub_retry_client: RetrierClient::new(
				scope,
				"dot_subscribe",
				dot_sub_client,
				POLKADOT_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
		}
	}
}

#[async_trait::async_trait]
pub trait DotRetryRpcApi {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Option<PolkadotHash>;

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Vec<ChainBlockExtrinsic>;

	async fn events(&self, block_hash: PolkadotHash) -> Option<Events<PolkadotConfig>>;

	async fn runtime_version(&self, block_hash: Option<H256>) -> RuntimeVersion;

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> PolkadotHash;
}

#[async_trait::async_trait]
impl DotRetryRpcApi for DotRetryRpcClient {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Option<PolkadotHash> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_hash(block_number).await })
				}),
				RequestLog::new("block_hash".to_string(), Some(format!("{block_number}"))),
			)
			.await
	}

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Vec<ChainBlockExtrinsic> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.extrinsics(block_hash).await?.ok_or(anyhow::anyhow!(
						"Block not found when querying for extrinsics at block hash {block_hash:?}"
					))
					})
				}),
				RequestLog::new("extrinsics".to_string(), Some(format!("{block_hash:?}"))),
			)
			.await
	}

	async fn events(&self, block_hash: PolkadotHash) -> Option<Events<PolkadotConfig>> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.events(block_hash).await })
				}),
				RequestLog::new("events".to_string(), Some(format!("{block_hash:?}"))),
			)
			.await
	}

	async fn runtime_version(&self, block_hash: Option<H256>) -> RuntimeVersion {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.runtime_version(block_hash).await })
				}),
				RequestLog::new("runtime_version".to_string(), None),
			)
			.await
	}

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> PolkadotHash {
		let log = RequestLog::new(
			"submit_raw_encoded_extrinsic".to_string(),
			Some(format!("{encoded_bytes:?}")),
		);
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					let encoded_bytes = encoded_bytes.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(
						async move { client.submit_raw_encoded_extrinsic(encoded_bytes).await },
					)
				}),
				log,
			)
			.await
	}
}

#[async_trait::async_trait]
pub trait DotRetrySubscribeApi {
	async fn subscribe_best_heads(
		&self,
	) -> Pin<Box<dyn Stream<Item = anyhow::Result<PolkadotHeader>> + Send>>;

	async fn subscribe_finalized_heads(
		&self,
	) -> Pin<Box<dyn Stream<Item = anyhow::Result<PolkadotHeader>> + Send>>;
}

use crate::dot::rpc::DotSubscribeApi;

#[async_trait::async_trait]
impl DotRetrySubscribeApi for DotRetryRpcClient {
	async fn subscribe_best_heads(
		&self,
	) -> Pin<Box<dyn Stream<Item = anyhow::Result<PolkadotHeader>> + Send>> {
		self.sub_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.subscribe_best_heads().await })
				}),
				RequestLog::new("subscribe_best_head".to_string(), None),
			)
			.await
	}

	async fn subscribe_finalized_heads(
		&self,
	) -> Pin<Box<dyn Stream<Item = anyhow::Result<PolkadotHeader>> + Send>> {
		self.sub_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.subscribe_finalized_heads().await })
				}),
				RequestLog::new("subscribe_finalized_heads".to_string(), None),
			)
			.await
	}
}

#[async_trait::async_trait]
impl ChainClient for DotRetryRpcClient {
	type Index = <Polkadot as cf_chains::Chain>::ChainBlockNumber;
	type Hash = PolkadotHash;
	type Data = Events<PolkadotConfig>;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						let block_hash = client
							.block_hash(index)
							.await?
							// TODO: Make these just return Result?
							.ok_or(anyhow::anyhow!("No block hash found for index {index}"))?;
						let header = client
							.block(block_hash)
							.await?
							.ok_or(anyhow::anyhow!("No block found for block hash {block_hash:?}"))?
							.block
							.header;

						assert_eq!(index, header.number);

						let events = client.events(block_hash).await?.ok_or(anyhow::anyhow!(
							"No events found for block hash {block_hash:?}"
						))?;
						Ok(Header {
							index,
							hash: header.hash(),
							parent_hash: Some(header.parent_hash),
							data: events,
						})
					})
				}),
				RequestLog::new("header_at_index".to_string(), Some(format!("{index}"))),
			)
			.await
	}
}

#[cfg(test)]
mod tests {
	use futures_util::FutureExt;

	use utilities::task_scope::task_scope;

	use super::*;

	#[tokio::test]
	#[ignore = "Requires network connection and will last forever with failing extrinsic submission"]
	async fn my_test() {
		task_scope(|scope| {
			async move {
				let dot_http_rpc_client =
					DotHttpRpcClient::new("http://127.0.0.1:9945").await.unwrap();

				let dot_sub_client = DotSubClient::new("ws://127.0.0.1:9945");
				let dot_retry_rpc_client =
					DotRetryRpcClient::new(scope, dot_http_rpc_client, dot_sub_client);

				let hash = dot_retry_rpc_client.block_hash(1).await.unwrap();
				println!("Block hash: {}", hash);

				let extrinsics = dot_retry_rpc_client.extrinsics(hash).await;
				println!("extrinsics: {:?}", extrinsics);

				let events = dot_retry_rpc_client.events(hash).await;
				println!("Events: {:?}", events);

				let runtime_version = dot_retry_rpc_client.runtime_version(None).await;
				println!("Runtime version: {:?}", runtime_version);

				let hash = dot_retry_rpc_client
					.submit_raw_encoded_extrinsic(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9])
					.await;
				println!("Extrinsic hash: {}", hash);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
