use std::pin::Pin;

use crate::dot::safe_runtime_version_stream::safe_runtime_version_stream;
use async_trait::async_trait;
use cf_chains::dot::{PolkadotHash, RuntimeVersion};
use cf_primitives::PolkadotBlockNumber;
use futures::{Stream, StreamExt, TryStreamExt};
use std::sync::Arc;
use subxt::{
	events::Events,
	rpc::types::{ChainBlockExtrinsic, ChainBlockResponse},
	Config, OnlineClient, PolkadotConfig,
};
use tokio::sync::RwLock;

use anyhow::{anyhow, Result};

#[cfg(test)]
use mockall::automock;

use super::http_rpc::DotHttpRpcClient;

pub type PolkadotHeader = <PolkadotConfig as Config>::Header;

#[derive(Clone)]
pub struct DotRpcClient {
	online_client: Arc<RwLock<OnlineClient<PolkadotConfig>>>,
	http_client: DotHttpRpcClient,
	polkadot_network_ws_url: String,
}

macro_rules! refresh_connection_on_error {
    ($self:expr, $namespace:ident, $method:ident $(, $arg:expr)*) => {{
		// This is pulled out into a block to avoid a deadlock. Inlining this means that the guard, here as a temporary
		// will be dropped after the match, and so we will wait at the write lock.
		let result = { $self.online_client.read().await.$namespace().$method($($arg,)*).await };
		match result {
			Err(e) => {
				tracing::warn!(
					"Initial {} query failed with error: {e}, refreshing client and retrying", stringify!($method)
				);

				let new_client =
					OnlineClient::<PolkadotConfig>::from_url(&$self.polkadot_network_ws_url).await?;
				let result = new_client.$namespace().$method($($arg,)*).await.map_err(|e| anyhow!("Failed to query {} Polkadot with error: {e}", stringify!($method)));
				let mut online_client_guard = $self.online_client.write().await;
				*online_client_guard = new_client;
				result
			},
			Ok(ok) => Ok(ok),
		}
    }};
}

impl DotRpcClient {
	pub async fn new(polkadot_network_ws_url: &str, http_client: DotHttpRpcClient) -> Result<Self> {
		let online_client = Arc::new(RwLock::new(
			OnlineClient::<PolkadotConfig>::from_url(polkadot_network_ws_url).await?,
		));
		Ok(Self {
			online_client,
			http_client,
			polkadot_network_ws_url: polkadot_network_ws_url.to_string(),
		})
	}
}

/// This trait defines any subscription interfaces to Polkadot.
#[async_trait]
pub trait DotSubscribeApi: Send + Sync {
	async fn subscribe_best_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>>;

	async fn subscribe_finalized_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>>;

	async fn subscribe_runtime_version(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = RuntimeVersion> + Send>>>;
}

/// The trait that defines the stateless / non-subscription requests to Polkadot.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait DotRpcApi: Send + Sync {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>>;

	async fn block(
		&self,
		block_hash: PolkadotHash,
	) -> Result<Option<ChainBlockResponse<PolkadotConfig>>>;

	async fn extrinsics(
		&self,
		block_hash: PolkadotHash,
	) -> Result<Option<Vec<ChainBlockExtrinsic>>>;

	async fn events(&self, block_hash: PolkadotHash) -> Result<Option<Events<PolkadotConfig>>>;

	async fn current_runtime_version(&self) -> Result<RuntimeVersion>;

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash>;
}

// Just pass through to the underlying http client
#[async_trait]
impl DotRpcApi for DotRpcClient {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>> {
		self.http_client.block_hash(block_number).await
	}

	async fn block(
		&self,
		block_hash: PolkadotHash,
	) -> Result<Option<ChainBlockResponse<PolkadotConfig>>> {
		self.http_client.block(block_hash).await
	}

	async fn current_runtime_version(&self) -> Result<RuntimeVersion> {
		self.http_client.current_runtime_version().await
	}

	async fn extrinsics(
		&self,
		block_hash: PolkadotHash,
	) -> Result<Option<Vec<ChainBlockExtrinsic>>> {
		self.http_client.extrinsics(block_hash).await
	}

	/// Returns the events for a particular block hash.
	/// If the block for the given block hash does not exist, then this returns `Ok(None)`.
	async fn events(&self, block_hash: PolkadotHash) -> Result<Option<Events<PolkadotConfig>>> {
		self.http_client.events(block_hash).await
	}

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		self.http_client.submit_raw_encoded_extrinsic(encoded_bytes).await
	}
}

#[derive(Clone)]
pub struct DotSubClient {
	pub ws_endpoint: String,
}

impl DotSubClient {
	pub async fn new(ws_endpoint: &str) -> Result<Self> {
		Ok(Self { ws_endpoint: ws_endpoint.to_string() })
	}
}

#[async_trait]
impl DotSubscribeApi for DotSubClient {
	async fn subscribe_best_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
		let client = OnlineClient::<PolkadotConfig>::from_url(self.ws_endpoint.clone()).await?;
		Ok(Box::pin(
			client
				.blocks()
				.subscribe_best()
				.await?
				.map(|block| block.map(|block| block.header().clone()))
				.map_err(|e| anyhow!("Error in best head stream: {e}")),
		))
	}

	async fn subscribe_finalized_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
		let client = OnlineClient::<PolkadotConfig>::from_url(self.ws_endpoint.clone()).await?;
		Ok(Box::pin(
			client
				.blocks()
				.subscribe_finalized()
				.await?
				.map(|block| block.map(|block| block.header().clone()))
				.map_err(|e| anyhow!("Error in finalised head stream: {e}")),
		))
	}

	async fn subscribe_runtime_version(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = RuntimeVersion> + Send>>> {
		let client = OnlineClient::<PolkadotConfig>::from_url(self.ws_endpoint.clone()).await?;
		let subxt_v_to_sc_v = |v: subxt::rpc::types::RuntimeVersion| RuntimeVersion {
			spec_version: v.spec_version,
			transaction_version: v.transaction_version,
		};
		let current_runtime_version =
			client.rpc().runtime_version(None).await.map(subxt_v_to_sc_v)?;
		let raw_runtime_version_stream = client
			.rpc()
			.subscribe_runtime_version()
			.await?
			.map(move |item| item.map_err(anyhow::Error::new).map(subxt_v_to_sc_v));
		safe_runtime_version_stream(current_runtime_version, raw_runtime_version_stream)
			.await
			.map_err(|e| anyhow!("Failed to subscribe to Polkadot runtime version with error: {e}"))
	}
}

#[async_trait]
impl DotSubscribeApi for DotRpcClient {
	async fn subscribe_best_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
		Ok(Box::pin(
			refresh_connection_on_error!(self, blocks, subscribe_best)?
				.map(|block| block.map(|block| block.header().clone()))
				.map_err(|e| anyhow!("Error in best head stream: {e}")),
		))
	}

	async fn subscribe_finalized_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
		Ok(Box::pin(
			refresh_connection_on_error!(self, blocks, subscribe_finalized)?
				.map(|block| block.map(|block| block.header().clone()))
				.map_err(|e| anyhow!("Error in finalised head stream: {e}")),
		))
	}

	async fn subscribe_runtime_version(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = RuntimeVersion> + Send>>> {
		safe_runtime_version_stream(
			self.current_runtime_version().await?,
			refresh_connection_on_error!(self, rpc, subscribe_runtime_version)?.map(|item| {
				item.map_err(anyhow::Error::new).map(
					|subxt::rpc::types::RuntimeVersion {
					     spec_version,
					     transaction_version,
					     ..
					 }| RuntimeVersion { spec_version, transaction_version },
				)
			}),
		)
		.await
		.map_err(|e| anyhow!("Failed to subscribe to Polkadot runtime version with error: {e}"))
	}
}
