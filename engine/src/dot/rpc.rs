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
use subxt::{
	backend::legacy::rpc_methods::{BlockDetails, Bytes},
	events::Events,
	ext::subxt_rpcs::{self},
	OnlineClient, PolkadotConfig,
};
use tracing::warn;

use crate::dot::{PolkadotHash, PolkadotHeader};
use anyhow::{anyhow, bail, Result};

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

	async fn runtime_version(&self, at: Option<PolkadotHash>) -> Result<RuntimeVersion>;

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash>;
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
