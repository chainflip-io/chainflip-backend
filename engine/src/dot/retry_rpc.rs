use crate::{
	common::option_inner,
	retrier::{Attempt, RetryLimitReturn},
	settings::{NodeContainer, WsHttpEndpoints},
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
use subxt::{
	backend::legacy::rpc_methods::Bytes, config::Header as SubxtHeader, events::Events,
	PolkadotConfig,
};
use utilities::task_scope::Scope;

use crate::retrier::{RequestLog, RetrierClient};

use anyhow::{anyhow, Result};

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

const POLKADOT_RPC_TIMEOUT: Duration = Duration::from_millis(4 * 1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 20;

const MAX_BROADCAST_RETRIES: Attempt = 2;

impl DotRetryRpcClient {
	pub fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<WsHttpEndpoints>,
		expected_genesis_hash: PolkadotHash,
	) -> Result<Self> {
		Self::new_inner(scope, nodes, Some(expected_genesis_hash))
	}

	fn new_inner(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<WsHttpEndpoints>,
		// The genesis hash is optional to facilitate testing
		expected_genesis_hash: Option<PolkadotHash>,
	) -> Result<Self> {
		let f_create_clients = |endpoints: WsHttpEndpoints| {
			Result::<_, anyhow::Error>::Ok((
				DotHttpRpcClient::new(endpoints.http_endpoint, expected_genesis_hash)?,
				DotSubClient::new(endpoints.ws_endpoint, expected_genesis_hash),
			))
		};

		let (rpc_client, sub_client) = f_create_clients(nodes.primary)?;

		let (backup_rpc_client, backup_sub_client) =
			option_inner(nodes.backup.map(f_create_clients).transpose()?);

		Ok(DotRetryRpcClient {
			rpc_retry_client: RetrierClient::new(
				scope,
				"dot_rpc",
				rpc_client,
				backup_rpc_client,
				POLKADOT_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			sub_retry_client: RetrierClient::new(
				scope,
				"dot_subscribe",
				futures::future::ready(sub_client),
				backup_sub_client.map(futures::future::ready),
				POLKADOT_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
		})
	}
}

#[async_trait::async_trait]
pub trait DotRetryRpcApi: Clone {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Option<PolkadotHash>;

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Vec<Bytes>;

	async fn events<R: RetryLimitReturn>(
		&self,
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
		retry_limit: R,
	) -> R::ReturnType<Option<Events<PolkadotConfig>>>;

	async fn runtime_version(&self, block_hash: Option<H256>) -> RuntimeVersion;

	async fn submit_raw_encoded_extrinsic(
		&self,
		encoded_bytes: Vec<u8>,
	) -> anyhow::Result<PolkadotHash>;
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

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Vec<Bytes> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.extrinsics(block_hash).await?.ok_or(anyhow!(
						"Block not found when querying for extrinsics at block hash {block_hash:?}"
					))
					})
				}),
				RequestLog::new("extrinsics".to_string(), Some(format!("{block_hash:?}"))),
			)
			.await
	}

	async fn events<R: RetryLimitReturn>(
		&self,
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
		retry_limit: R,
	) -> R::ReturnType<Option<Events<PolkadotConfig>>> {
		self.rpc_retry_client
			.request_with_limit(
				Box::pin(move |client: DotHttpRpcClient| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.events(block_hash, parent_hash).await })
				}),
				RequestLog::new("events".to_string(), Some(format!("{block_hash:?}"))),
				retry_limit,
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

	async fn submit_raw_encoded_extrinsic(
		&self,
		encoded_bytes: Vec<u8>,
	) -> anyhow::Result<PolkadotHash> {
		let log = RequestLog::new(
			"submit_raw_encoded_extrinsic".to_string(),
			Some(format!("0x{}", hex::encode(&encoded_bytes[..]))),
		);
		self.rpc_retry_client
			.request_with_limit(
				Box::pin(move |client| {
					let encoded_bytes = encoded_bytes.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(
						async move { client.submit_raw_encoded_extrinsic(encoded_bytes).await },
					)
				}),
				log,
				MAX_BROADCAST_RETRIES,
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
				RequestLog::new("subscribe_best_heads".to_string(), None),
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
							.ok_or(anyhow!("No block hash found for index {index}"))?;
						let header = client
							.block(block_hash)
							.await?
							.ok_or(anyhow!("No block found for block hash {block_hash:?}"))?
							.block
							.header;

						assert_eq!(index, header.number);

						let events = client
							.events(block_hash, header.parent_hash)
							.await?
							.ok_or(anyhow!("No events found for block hash {block_hash:?}"))?;
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
pub mod mocks {
	use super::*;
	use mockall::mock;

	mock! {
		pub DotHttpRpcClient {}

		impl Clone for DotHttpRpcClient {
			fn clone(&self) -> Self;
		}

		#[async_trait::async_trait]
		impl DotRetryRpcApi for DotHttpRpcClient {
			async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Option<PolkadotHash>;

			async fn extrinsics(&self, block_hash: PolkadotHash) -> Vec<Bytes>;

			async fn events<R: RetryLimitReturn>(&self, block_hash: PolkadotHash, parent_hash: PolkadotHash, retry_limit: R) -> R::ReturnType<Option<Events<PolkadotConfig>>>;

			async fn runtime_version(&self, block_hash: Option<H256>) -> RuntimeVersion;

			async fn submit_raw_encoded_extrinsic(
				&self,
				encoded_bytes: Vec<u8>,
			) -> anyhow::Result<PolkadotHash>;
		}

	}
}

#[cfg(test)]
mod tests {
	use futures_util::FutureExt;

	use utilities::task_scope::task_scope;

	use crate::retrier::NoRetryLimit;

	use super::*;

	#[tokio::test]
	#[ignore = "Requires network connection and will last forever with failing extrinsic submission"]
	async fn my_test() {
		task_scope(|scope| {
			async move {
				let dot_retry_rpc_client = DotRetryRpcClient::new_inner(
					scope,
					NodeContainer {
						primary: WsHttpEndpoints {
							http_endpoint: "http://127.0.0.1:9945".into(),
							ws_endpoint: "ws://127.0.0.1:9945".into(),
						},
						backup: None,
					},
					None,
				)
				.unwrap();

				let hash = dot_retry_rpc_client.block_hash(1).await.unwrap();
				println!("Block hash: {}", hash);

				let extrinsics = dot_retry_rpc_client.extrinsics(hash).await;
				println!("extrinsics: {:?}", extrinsics);

				let events = dot_retry_rpc_client.events(hash, hash, NoRetryLimit).await;
				println!("Events: {:?}", events);

				let runtime_version = dot_retry_rpc_client.runtime_version(None).await;
				println!("Runtime version: {:?}", runtime_version);

				let hash = dot_retry_rpc_client
					.submit_raw_encoded_extrinsic(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9])
					.await
					.unwrap();
				println!("Extrinsic hash: {}", hash);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
