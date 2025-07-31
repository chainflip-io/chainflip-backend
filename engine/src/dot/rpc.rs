// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;

use async_trait::async_trait;
use cf_chains::dot::RuntimeVersion;
use cf_primitives::PolkadotBlockNumber;
use cf_utilities::redact_endpoint_secret::SecretUrl;
use futures::{Stream, StreamExt, TryStreamExt};
use std::sync::Arc;
use subxt::{
	backend::{legacy::rpc_methods::{BlockDetails, Bytes}, rpc::RpcClient},
	events::Events,
	ext::subxt_rpcs::{self, LegacyRpcMethods},
	OnlineClient, PolkadotConfig,
};
use tokio::sync::RwLock;
use tracing::warn;

use super::http_rpc::DotHttpRpcClient;
use crate::dot::{PolkadotHash, PolkadotHeader};
use anyhow::{anyhow, bail, Result};

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
					OnlineClient::<PolkadotConfig>::from_insecure_url(&$self.polkadot_network_ws_url).await?;
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
		if subxt_rpcs::utils::validate_url_is_secure(polkadot_network_ws_url).is_err() {
			warn!("Using insecure Polkadot websocket endpoint: {polkadot_network_ws_url}");
		}

		let online_client = Arc::new(RwLock::new(
			OnlineClient::<PolkadotConfig>::from_insecure_url(polkadot_network_ws_url).await?,
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

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash>;
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

	async fn runtime_version(&self, at: Option<PolkadotHash>) -> Result<RuntimeVersion>;

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

	async fn runtime_version(&self, at: Option<PolkadotHash>) -> Result<RuntimeVersion> {
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

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		let rpc_methods = LegacyRpcMethods::<PolkadotConfig>::new(RpcClient::from_url(self.ws_endpoint.clone()).await?);

		{
			let hash: PolkadotHash = sp_core::blake2_256(&encoded_bytes).into();

			let encoded_bytes_c = encoded_bytes.clone();

			// TEMP submit and watch for completion!
			let mut result =
				rpc_methods.author_submit_and_watch_extrinsic(&encoded_bytes_c).await?;


			tokio::task::spawn(async move {
				while let Some(event) = result.next().await {
					let event = event?;
					match event {
						subxt_rpcs::methods::legacy::TransactionStatus::Finalized(hash) => {
							tracing::info!("dot extrinsic was finalized with hash ({hash:?}), for extrinsic {encoded_bytes:?}");
							return Ok(hash)
						},
						// subxt_rpcs::methods::legacy::TransactionStatus::Future => todo!(),
						// subxt_rpcs::methods::legacy::TransactionStatus::Ready => todo!(),
						// subxt_rpcs::methods::legacy::TransactionStatus::Broadcast(items) => todo!(),
						// subxt_rpcs::methods::legacy::TransactionStatus::InBlock(_) => todo!(),
						subxt_rpcs::methods::legacy::TransactionStatus::Retracted(_) |
						subxt_rpcs::methods::legacy::TransactionStatus::FinalityTimeout(_) |
						subxt_rpcs::methods::legacy::TransactionStatus::Usurped(_) |
						subxt_rpcs::methods::legacy::TransactionStatus::Dropped |
						subxt_rpcs::methods::legacy::TransactionStatus::Invalid => {
							tracing::error!("error for dot extrinsic ({event:?}), for extrinsic {encoded_bytes:?}");
							return Err(anyhow!("error for dot extrinsic ({event:?}), for extrinsic {encoded_bytes:?}"));
						},
						state => tracing::info!("submission of dot extrinsic reached state {state:?}, for extrinsic {encoded_bytes:?}")
					}
				}

				Err(anyhow!("rpc subscription for submission of extrinsic {encoded_bytes:?} terminatd unexpectedly!"))
			});

			Ok(hash)
		}
	}
}

/// Creates an OnlineClient from the given websocket endpoint and checks the genesis hash if
/// provided.
async fn create_online_client(
	ws_endpoint: &SecretUrl,
	expected_genesis_hash: Option<PolkadotHash>,
) -> Result<OnlineClient<PolkadotConfig>> {
	if subxt_rpcs::utils::validate_url_is_secure(ws_endpoint.as_ref()).is_err() {
		warn!("Using insecure Polkadot websocket endpoint: {ws_endpoint}");
	}

	let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_endpoint).await?;

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

// #[async_trait]
// impl DotSubscribeApi for DotRpcClient {
// 	async fn subscribe_best_heads(
// 		&self,
// 	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
// 		Ok(Box::pin(
// 			refresh_connection_on_error!(self, blocks, subscribe_best)?
// 				.map(|block| block.map(|block| block.header().clone()))
// 				.map_err(|e| anyhow!("Error in best head stream: {e}")),
// 		))
// 	}

// 	async fn subscribe_finalized_heads(
// 		&self,
// 	) -> Result<Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>> {
// 		Ok(Box::pin(
// 			refresh_connection_on_error!(self, blocks, subscribe_finalized)?
// 				.map(|block| block.map(|block| block.header().clone()))
// 				.map_err(|e| anyhow!("Error in finalised head stream: {e}")),
// 		))
// 	}
// }
