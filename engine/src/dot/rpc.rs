use std::pin::Pin;

use crate::dot::safe_runtime_version_stream::safe_runtime_version_stream;
use async_trait::async_trait;
use cf_chains::dot::{PolkadotHash, RuntimeVersion};
use cf_primitives::PolkadotBlockNumber;
use futures::{Stream, StreamExt, TryStreamExt};
use std::sync::Arc;
use subxt::{
	error::BlockError,
	events::{Events, EventsClient},
	rpc::types::{Bytes, ChainBlockExtrinsic, ChainBlockResponse},
	rpc_params, Config, OnlineClient, PolkadotConfig,
};
use tokio::sync::RwLock;

use anyhow::{anyhow, Result};

#[cfg(test)]
use mockall::automock;

pub type PolkadotHeader = <PolkadotConfig as Config>::Header;

#[derive(Clone)]
pub struct DotRpcClient {
	online_client: Arc<RwLock<OnlineClient<PolkadotConfig>>>,
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
	pub async fn new(polkadot_network_ws_url: &str) -> Result<Self> {
		let online_client = Arc::new(RwLock::new(
			OnlineClient::<PolkadotConfig>::from_url(polkadot_network_ws_url).await?,
		));
		Ok(Self { online_client, polkadot_network_ws_url: polkadot_network_ws_url.to_string() })
	}

	async fn metadata(&self, block_hash: PolkadotHash) -> Result<subxt::Metadata> {
		refresh_connection_on_error!(self, rpc, metadata, Some(block_hash))
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

#[async_trait]
impl DotRpcApi for DotRpcClient {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>> {
		refresh_connection_on_error!(self, rpc, block_hash, Some(block_number.into()))
	}

	async fn block(
		&self,
		block_hash: PolkadotHash,
	) -> Result<Option<ChainBlockResponse<PolkadotConfig>>> {
		Ok(self.online_client.read().await.rpc().block(Some(block_hash)).await?)
	}

	async fn current_runtime_version(&self) -> Result<RuntimeVersion> {
		refresh_connection_on_error!(self, rpc, runtime_version, None).map(|rv| RuntimeVersion {
			spec_version: rv.spec_version,
			transaction_version: rv.transaction_version,
		})
	}

	async fn extrinsics(
		&self,
		block_hash: PolkadotHash,
	) -> Result<Option<Vec<ChainBlockExtrinsic>>> {
		refresh_connection_on_error!(self, rpc, block, Some(block_hash))
			.map(|o| o.map(|b| b.block.extrinsics))
	}

	/// Returns the events for a particular block hash.
	/// If the block for the given block hash does not exist, then this returns `Ok(None)`.
	async fn events(&self, block_hash: PolkadotHash) -> Result<Option<Events<PolkadotConfig>>> {
		let chain_runtime_version = self.current_runtime_version().await?;

		let client = self.online_client.read().await;

		let client_runtime_version = client.runtime_version();

		// We set the metadata and runtime version we need to decode this block's events.
		// The metadata from the OnlineClient is used within the EventsClient to decode the
		// events.
		if chain_runtime_version.spec_version != client_runtime_version.spec_version ||
			chain_runtime_version.transaction_version !=
				client_runtime_version.transaction_version
		{
			let new_metadata = self.metadata(block_hash).await?;

			client.set_runtime_version(subxt::rpc::types::RuntimeVersion {
				spec_version: chain_runtime_version.spec_version,
				transaction_version: chain_runtime_version.transaction_version,
				other: Default::default(),
			});
			client.set_metadata(new_metadata);
		}

		// If we've succeeded in getting the current runtime version then we assume
		// the connection is stable (or has just been refreshed), no need to retry again.
		match EventsClient::new(client.clone()).at(Some(block_hash)).await {
			Ok(events) => Ok(Some(events)),
			Err(e) => match e {
				subxt::Error::Block(BlockError::BlockHashNotFound(_)) => Ok(None),
				_ => Err(e.into()),
			},
		}
	}

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		let encoded_bytes: Bytes = encoded_bytes.into();
		refresh_connection_on_error!(
			self,
			rpc,
			request,
			"author_submitExtrinsic",
			rpc_params![encoded_bytes.clone()]
		)
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

#[cfg(test)]
mod tests {

	use crate::dot::DotBroadcaster;

	use super::*;

	#[tokio::test]
	#[ignore = "Testing raw broadcast to live network"]
	async fn broadcast_tx() {
		let dot_broadcaster = DotBroadcaster::new(DotRpcClient::new("URL").await.unwrap());

		// Can get these bytes from the `create_test_extrinsic()` in state-chain/chains/src/dot.rs
		// Will have to ensure the nonce for the account is correct and westend versions are correct
		// for the transaction to be valid
		let balances_signed_encoded_bytes: Vec<u8> = vec![
			61, 2, 132, 0, 86, 204, 74, 248, 255, 159, 185, 124, 96, 50, 10, 228, 61, 53, 189, 131,
			27, 20, 240, 183, 6, 95, 51, 133, 219, 13, 191, 76, 181, 216, 118, 111, 1, 248, 49, 73,
			4, 246, 220, 141, 169, 139, 169, 179, 156, 141, 168, 221, 129, 139, 217, 69, 138, 202,
			21, 226, 229, 249, 205, 183, 253, 121, 63, 133, 124, 0, 52, 146, 100, 192, 219, 76,
			144, 138, 123, 47, 117, 101, 73, 139, 71, 255, 94, 99, 144, 186, 185, 34, 46, 165, 13,
			183, 107, 235, 223, 12, 139, 0, 48, 0, 4, 0, 0, 190, 185, 195, 240, 174, 91, 218, 121,
			141, 211, 182, 95, 227, 69, 253, 249, 3, 25, 70, 132, 157, 137, 37, 174, 123, 231, 62,
			233, 64, 124, 103, 55, 7, 0, 158, 41, 38, 8,
		];

		let tx_hash = dot_broadcaster.send(balances_signed_encoded_bytes).await.unwrap();
		println!("Tx hash: {tx_hash:?}");
	}
}
