use std::pin::Pin;

use crate::dot::safe_runtime_version_stream::safe_runtime_version_stream;
use async_trait::async_trait;
use cf_chains::dot::{PolkadotHash, RuntimeVersion};
use cf_primitives::PolkadotBlockNumber;
use futures::{Stream, StreamExt, TryStreamExt};
use subxt::{
	events::{Events, EventsClient},
	rpc::types::{Bytes, ChainBlock},
	rpc_params, Config, OnlineClient, PolkadotConfig,
};

use anyhow::{anyhow, Result};

#[cfg(test)]
use mockall::automock;

type PolkadotHeader = <PolkadotConfig as Config>::Header;

#[derive(Clone)]
pub struct DotRpcClient {
	online_client: OnlineClient<PolkadotConfig>,
}

impl DotRpcClient {
	pub async fn new(polkadot_network_ws_url: &str) -> Result<Self> {
		let online_client =
			OnlineClient::<PolkadotConfig>::from_url(polkadot_network_ws_url).await?;
		Ok(Self { online_client })
	}
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait DotRpcApi: Send + Sync {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>>;

	async fn block(&self, block_hash: PolkadotHash) -> Result<Option<ChainBlock<PolkadotConfig>>>;

	async fn events(&self, block_hash: PolkadotHash) -> Result<Events<PolkadotConfig>>;

	async fn current_runtime_version(&self) -> Result<RuntimeVersion>;

	async fn subscribe_finalized_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>>;

	async fn subscribe_runtime_version(
		&self,
		logger: &slog::Logger,
	) -> Result<Pin<Box<dyn Stream<Item = RuntimeVersion> + Send>>>;

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash>;
}

#[async_trait]
impl DotRpcApi for DotRpcClient {
	async fn subscribe_finalized_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
		Ok(Box::pin(
			self.online_client
				.blocks()
				.subscribe_finalized()
				.await
				.map_err(|e| anyhow!("Error initialising finalised head stream: {e}"))?
				.map(|block| block.map(|block| block.header().clone()))
				.map_err(|e| anyhow!("Error in finalised head stream: {e}")),
		))
	}

	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>> {
		self.online_client
			.rpc()
			.block_hash(Some(block_number.into()))
			.await
			.map_err(|e| anyhow!("Failed to query Polkadot block hash with error: {e}"))
	}

	async fn current_runtime_version(&self) -> Result<RuntimeVersion> {
		let runtime_version = self
			.online_client
			.rpc()
			.runtime_version(None)
			.await
			.map_err(|e| anyhow!("Failed to query Polkadot runtime version with error: {e}"))?;

		Ok(RuntimeVersion {
			spec_version: runtime_version.spec_version,
			transaction_version: runtime_version.transaction_version,
		})
	}

	async fn subscribe_runtime_version(
		&self,
		logger: &slog::Logger,
	) -> Result<Pin<Box<dyn Stream<Item = RuntimeVersion> + Send>>> {
		safe_runtime_version_stream(
			self.current_runtime_version().await?,
			self.online_client
				.rpc()
				.subscribe_runtime_version()
				.await
				.map_err(|e| anyhow!("Error initialising runtime version stream: {e}"))?
				.map(|item| {
					item.map_err(anyhow::Error::new).map(
						|subxt::rpc::types::RuntimeVersion {
						     spec_version,
						     transaction_version,
						     ..
						 }| RuntimeVersion { spec_version, transaction_version },
					)
				}),
			logger,
		)
		.await
		.map_err(|e| anyhow!("Failed to subscribe to Polkadot runtime version with error: {e}"))
	}

	async fn block(&self, block_hash: PolkadotHash) -> Result<Option<ChainBlock<PolkadotConfig>>> {
		self.online_client
			.rpc()
			.block(Some(block_hash))
			.await
			.map(|r| r.map(|r| r.block))
			.map_err(|e| anyhow!("Failed to query for block with error: {e}"))
	}

	async fn events(&self, block_hash: PolkadotHash) -> Result<Events<PolkadotConfig>> {
		let chain_runtime_version = self
			.online_client
			.rpc()
			.runtime_version(Some(block_hash))
			.await
			.map_err(|e| anyhow!("Failed to query runtime version with error: {e}"))?;

		// We set the metadata and runtime version we need to decode this block's events.
		// The metadata from the OnlineClient is used within the EventsClient to decode the
		// events.
		if self.online_client.runtime_version() != chain_runtime_version {
			let new_metadata = self
				.online_client
				.rpc()
				.metadata(Some(block_hash))
				.await
				.map_err(|e| anyhow!("Failed to query metadata with error: {e}"))?;

			self.online_client.set_runtime_version(chain_runtime_version);
			self.online_client.set_metadata(new_metadata);
		}

		EventsClient::new(self.online_client.clone())
			.at(Some(block_hash))
			.await
			.map_err(|e| anyhow!("Failed to query events for block {block_hash}, with error: {e}"))
	}

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		let encoded_bytes: Bytes = encoded_bytes.into();
		self.online_client
			.rpc()
			.request::<PolkadotHash>("author_submitExtrinsic", rpc_params![encoded_bytes])
			.await
			.map_err(|e| anyhow!("Raw Polkadot extrinsic submission failed with error: {e}"))
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
