use std::pin::Pin;

use async_trait::async_trait;
use cf_chains::dot::{PolkadotHash, RuntimeVersion};
use cf_primitives::PolkadotBlockNumber;
use futures::{Stream, StreamExt, TryStreamExt};
use sp_core::H256;
use std::sync::Arc;
use subxt::{
	backend::legacy::rpc_methods::{BlockDetails, Bytes},
	events::Events,
	Config, OnlineClient, PolkadotConfig,
};
use tokio::sync::RwLock;
use tracing::warn;
use utilities::redact_endpoint_secret::SecretUrl;

use anyhow::{anyhow, bail, Result};

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
}

/// The trait that defines the stateless / non-subscription requests to Polkadot.
#[async_trait]
pub trait DotRpcApi: Send + Sync {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>>;

	async fn block(&self, block_hash: PolkadotHash)
		-> Result<Option<BlockDetails<PolkadotConfig>>>;

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Result<Option<Vec<Bytes>>>;

	async fn events(
		&self,
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
	) -> Result<Option<Events<PolkadotConfig>>>;

	async fn runtime_version(&self, at: Option<H256>) -> Result<RuntimeVersion>;

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
	) -> Result<Option<BlockDetails<PolkadotConfig>>> {
		self.http_client.block(block_hash).await
	}

	async fn runtime_version(&self, at: Option<H256>) -> Result<RuntimeVersion> {
		self.http_client.runtime_version(at).await
	}

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Result<Option<Vec<Bytes>>> {
		self.http_client.extrinsics(block_hash).await
	}

	/// Returns the events for a particular block hash.
	/// If the block for the given block hash does not exist, then this returns `Ok(None)`.
	async fn events(
		&self,
		block_hash: PolkadotHash,
		// The parent hash is used to determine the runtime version to decode the events.
		parent_hash: PolkadotHash,
	) -> Result<Option<Events<PolkadotConfig>>> {
		self.http_client.events(block_hash, parent_hash).await
	}

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		self.http_client.submit_raw_encoded_extrinsic(encoded_bytes).await
	}
}

#[derive(Clone)]
pub struct DotSubClient {
	pub ws_endpoint: SecretUrl,
	expected_genesis_hash: Option<PolkadotHash>,
}

impl DotSubClient {
	pub fn new(ws_endpoint: SecretUrl, expected_genesis_hash: Option<PolkadotHash>) -> Self {
		Self { ws_endpoint, expected_genesis_hash }
	}
}

#[async_trait]
impl DotSubscribeApi for DotSubClient {
	async fn subscribe_best_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
		let client = create_online_client(&self.ws_endpoint, self.expected_genesis_hash).await?;

		Ok(Box::pin(
			client
				.blocks()
				.subscribe_best()
				.await?
				.map(|result| result.map(|block| block.header().clone()))
				.map_err(|e| anyhow!("Error in best head stream: {e}")),
		))
	}

	async fn subscribe_finalized_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
		let client = create_online_client(&self.ws_endpoint, self.expected_genesis_hash).await?;

		Ok(Box::pin(
			client
				.blocks()
				.subscribe_finalized()
				.await?
				.map(|result| result.map(|block| block.header().clone()))
				.map_err(|e| anyhow!("Error in finalised head stream: {e}")),
		))
	}
}

/// Creates an OnlineClient from the given websocket endpoint and checks the genesis hash if
/// provided.
async fn create_online_client(
	ws_endpoint: &SecretUrl,
	expected_genesis_hash: Option<PolkadotHash>,
) -> Result<OnlineClient<PolkadotConfig>> {
	let client = OnlineClient::<PolkadotConfig>::from_url(ws_endpoint).await?;

	if let Some(expected_genesis_hash) = expected_genesis_hash {
		let genesis_hash = client.genesis_hash();
		if genesis_hash != expected_genesis_hash {
			bail!("Expected Polkadot genesis hash {expected_genesis_hash} but got {genesis_hash}");
		}
	} else {
		warn!("Skipping Polkadot genesis hash check");
	}

	Ok(client)
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
}
