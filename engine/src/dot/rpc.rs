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
use cf_chains::dot::{PolkadotAccountId, RuntimeVersion};
use cf_primitives::{chains::assets::hub::Asset as HubAsset, PolkadotBlockNumber};
use cf_utilities::redact_endpoint_secret::SecretUrl;
use futures::{future, stream, Future, Stream, StreamExt, TryStreamExt};
use subxt::{
	backend::{
		legacy::{
			rpc_methods::{BlockDetails, Bytes},
			LegacyRpcMethods,
		},
		rpc::RpcClient,
	},
	config::{substrate::BlakeTwo256, Hasher},
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
	) -> Result<Pin<Box<dyn Stream<Item = Result<(PolkadotHash, PolkadotHeader)>> + Send>>>;

	async fn subscribe_finalized_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<(PolkadotHash, PolkadotHeader)>> + Send>>>;
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

	async fn liquid_account_balance(
		&self,
		account_id: PolkadotAccountId,
		asset: HubAsset,
		block_hash: PolkadotHash,
	) -> Result<u128>;
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
	#[expect(clippy::result_large_err)]
	async fn subscribe_best_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<(PolkadotHash, PolkadotHeader)>> + Send>>> {
		let client = create_online_client(&self.ws_endpoint, self.expected_genesis_hash).await?;

		Ok(Box::pin(
			client
				.blocks()
				.subscribe_best()
				.await?
				.map(|result| result.map(|block| (block.hash(), block.header().clone())))
				.map_err(|e| anyhow!("Error in best head stream: {e}")),
		))
	}

	/// Subscribes to finalized heads, filling in any gaps between notifications.
	///
	/// We don't use subxt's `subscribe_finalized` because its gap filling falls back to the *best*
	/// header if a block hash lookup returns `None`, which would emit an unfinalized block.
	async fn subscribe_finalized_heads(
		&self,
	) -> Result<Pin<Box<dyn Stream<Item = Result<(PolkadotHash, PolkadotHeader)>> + Send>>> {
		if subxt_rpcs::utils::validate_url_is_secure(self.ws_endpoint.as_ref()).is_err() {
			warn!("Using insecure Polkadot websocket endpoint: {}", self.ws_endpoint);
		}
		let methods = LegacyRpcMethods::<PolkadotConfig>::new(
			RpcClient::from_insecure_url(&self.ws_endpoint).await?,
		);

		if let Some(expected_genesis_hash) = self.expected_genesis_hash {
			let genesis_hash = methods.genesis_hash().await?;
			if genesis_hash != expected_genesis_hash {
				bail!(
					"Expected Polkadot genesis hash {expected_genesis_hash} but got {genesis_hash}"
				);
			}
		} else {
			warn!("Skipping Polkadot genesis hash check");
		}

		let heads = methods
			.chain_subscribe_finalized_heads()
			.await?
			.map_err(|e| anyhow!("Error in finalised head stream: {e}"));

		Ok(Box::pin(fill_in_gaps(heads, move |block_number| {
			let methods = methods.clone();
			async move {
				let hash = methods
					.chain_get_block_hash(Some(block_number.into()))
					.await?
					.ok_or_else(|| anyhow!("No hash for finalised block {block_number}"))?;
				let header = methods
					.chain_get_header(Some(hash))
					.await?
					.ok_or_else(|| anyhow!("No header for finalised block {hash:?}"))?;
				Ok((hash, header))
			}
		})))
	}
}

/// Emits the headers of any blocks skipped between consecutive `heads`, fetched via
/// `fetch_header`, before each head. Failed fetches are emitted as errors.
fn fill_in_gaps<Fut>(
	heads: impl Stream<Item = Result<PolkadotHeader>> + Send,
	fetch_header: impl Fn(PolkadotBlockNumber) -> Fut + Clone + Send + 'static,
) -> impl Stream<Item = Result<(PolkadotHash, PolkadotHeader)>> + Send
where
	Fut: Future<Output = Result<(PolkadotHash, PolkadotHeader)>> + Send,
{
	let mut last_block_number: Option<PolkadotBlockNumber> = None;
	heads.flat_map(move |result| {
		let header = match result {
			Ok(header) => header,
			Err(e) => return stream::once(future::ready(Err(e))).left_stream(),
		};
		let gap = last_block_number.map_or(header.number, |n| n.saturating_add(1))..header.number;
		last_block_number = last_block_number.max(Some(header.number));

		stream::iter(gap)
			.then(fetch_header.clone())
			.chain(stream::once(future::ready(Ok((BlakeTwo256.hash_of(&header), header)))))
			.right_stream()
	})
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

#[cfg(test)]
mod tests {
	use super::*;
	use subxt::config::substrate::Digest;

	fn header(number: PolkadotBlockNumber) -> PolkadotHeader {
		PolkadotHeader {
			parent_hash: Default::default(),
			number,
			state_root: Default::default(),
			extrinsics_root: Default::default(),
			digest: Digest::default(),
		}
	}

	async fn fill(
		heads: Vec<PolkadotBlockNumber>,
		missing: &'static [PolkadotBlockNumber],
	) -> Vec<Option<PolkadotBlockNumber>> {
		fill_in_gaps(stream::iter(heads.into_iter().map(|n| Ok(header(n)))), move |n| async move {
			if missing.contains(&n) {
				Err(anyhow!("missing"))
			} else {
				Ok((Default::default(), header(n)))
			}
		})
		.map(|result| result.ok().map(|(_, header)| header.number))
		.collect()
		.await
	}

	#[test]
	fn computed_head_hash_matches_chain() {
		// Assethub mainnet block 21314306, as returned by `chain_getHeader`.
		let header: PolkadotHeader = serde_json::from_str(r#"{
			"parentHash": "0x2269e85cbed09a5e523f04745071a73a588b5b6e9e929e9427b4ddc5dd7fc507",
			"number": "0x1453b02",
			"stateRoot": "0xc68a4eea9897ac8024d404aacd6a699cb3566df588b5513f3bae8f9e9b78f3a5",
			"extrinsicsRoot": "0xa93660e6a6fc3b46dcf60721bc2fc000852a5c2a64785256c7b157f22142908f",
			"digest": { "logs": [
				"0x06434d4c53100100020c",
				"0x06434d4c530c020001",
				"0x066175726120ec9b720400000000",
				"0x0452505352900dbb6cd3f11d8907c93ab57138e1f125d5ee9c2fe46162cda1f399e36ee37f07320ded07",
				"0x056175726101019e8f11a3f0936f2e770335282a682c55237d75318b17c4651448004d9f963c5d235ef430f4ee958c3138327ad5a6025e027b0427409e3b4dcd1454292d6cef03"
			] }
		}"#)
		.unwrap();

		assert_eq!(
			format!("{:?}", BlakeTwo256.hash_of(&header)),
			"0x4b7f5e012c00f0e6930c1f4e010ebfd3c7866145ee743e593981faf16decefda"
		);
	}

	#[tokio::test]
	async fn fills_in_gaps_between_finalized_heads() {
		assert_eq!(fill(vec![10, 11, 14, 15], &[]).await, [10, 11, 12, 13, 14, 15].map(Some));
	}

	#[tokio::test]
	async fn missing_blocks_are_errors_not_other_headers() {
		assert_eq!(fill(vec![10, 14], &[12]).await, [Some(10), Some(11), None, Some(13), Some(14)]);
	}

	#[tokio::test]
	async fn does_not_refetch_on_repeated_or_lower_heads() {
		assert_eq!(fill(vec![10, 10, 9, 11], &[]).await, [10, 10, 9, 11].map(Some));
	}
}
