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

use cf_chains::dot::RuntimeVersion;
use cf_primitives::PolkadotBlockNumber;
use http::uri::Uri;
use subxt::{
	backend::{
		legacy::{
			rpc_methods::{BlockDetails, Bytes},
			LegacyRpcMethods,
		},
		rpc::RpcClient,
	},
	error::BlockError,
	events::{Events, EventsClient},
	OnlineClient, PolkadotConfig,
};
use url::Url;

use anyhow::{anyhow, Result};
use cf_utilities::{make_periodic_tick, redact_endpoint_secret::SecretUrl};
use codec::Decode;
use tracing::{error, warn};

use crate::constants::RPC_RETRY_CONNECTION_INTERVAL;

use super::rpc::DotRpcApi;

use crate::dot::PolkadotHash;

/// Adds a default port to the url based on the scheme (http, https, ws, wss),
/// if none exists. Otherwise preservers existing port.
///
/// This function assumes that `url` is already validated, i.e.:
///  - It's accepted by `Url::parse()`
///  - It includes a host part
///  - It does not have a fragment part
fn ensure_port(url: SecretUrl) -> Result<SecretUrl> {
	// we use url::Url to get the default port for our scheme
	let targetport = Url::parse(url.as_ref())
		.expect("SecretUrl was validated by being passed into `Url::parse`.")
		.port_or_known_default()
		.ok_or(anyhow::anyhow!("Unknown scheme and no port given."))?;

	// We use http::uri::Uri to ensure that the default port is added if none exists
	// We split the uri into components, insert the port, and reconstruct.
	let mut parts = url
		.as_ref()
		.parse::<Uri>()
		.map_err(|err| anyhow!("Unexpected error when trying to append port to url: {err}"))?
		.into_parts();

	// Update the authority part of the uri by mapping over it.
	let authority = parts.authority.clone().expect("SecretUrl contains authority.");
	if authority.port().is_none() {
		parts.authority =
			Some(format!("{}:{}", authority.as_str(), targetport).parse().map_err(|err| {
				anyhow!("Unexpected error when trying to append port to url: {err}")
			})?);
	};

	// Reconstruct uri.
	Uri::from_parts(parts)
		.map_err(|err| anyhow!("Unexpected error when trying to append port to url: {err}"))
		.map(|uri| uri.to_string().into())
}

#[derive(Clone)]
pub struct DotHttpRpcClientBuilder {
	url: SecretUrl,
	expected_genesis_hash: Option<PolkadotHash>,
}

#[derive(Clone)]
pub struct DotHttpRpcClient {
	online_client: OnlineClient<PolkadotConfig>,
	rpc_methods: LegacyRpcMethods<PolkadotConfig>,
}

impl DotHttpRpcClientBuilder {
	pub fn new(url: SecretUrl, expected_genesis_hash: Option<PolkadotHash>) -> Result<Self> {
		// Currently, the jsonrpsee library used by the PolkadotHttpClient expects
		// a port number to be always present in the url. Here we ensure this,
		// adding the default port if none is present.
		Ok(Self { url: ensure_port(url)?, expected_genesis_hash })
	}

	pub async fn connect(self) -> DotHttpRpcClient {
		// We don't want to return an error here. Returning an error means that we'll exit the
		// CFE. So on client creation we wait until we can be successfully connected to the
		// Polkadot node. So the other chains are unaffected
		let mut poll_interval = make_periodic_tick(RPC_RETRY_CONNECTION_INTERVAL, true);
		let (rpc_client, online_client) = loop {
			poll_interval.tick().await;

			let maybe_online_client: anyhow::Result<_> =
				match RpcClient::from_url(self.url.clone()).await {
					Ok(rpc_client) =>
						OnlineClient::<PolkadotConfig>::from_rpc_client(rpc_client.clone())
							.await
							.map(move |a| (rpc_client, a))
							.map_err(|e| e.into()),
					Err(e) => Err(e.into()),
				};

			match maybe_online_client {
				Ok((rpc_client, online_client)) => {
					if let Some(expected_genesis_hash) = self.expected_genesis_hash {
						let genesis_hash = online_client.genesis_hash();
						if genesis_hash == expected_genesis_hash {
							break (rpc_client, online_client)
						} else {
							error!(
								"Connected to Polkadot node at {} but the genesis hash {genesis_hash} does not match the expected genesis hash {expected_genesis_hash}. Please check your CFE configuration file.",
								self.url
							)
						}
					} else {
						warn!("Skipping Polkadot genesis hash check");
						break (rpc_client, online_client)
					}
				},
				Err(e) => {
					error!(
						"Failed to connect to Polkadot node at {} with error: {e}. \
							Please check your CFE configuration file. Retrying in {:?}...",
						self.url,
						poll_interval.period()
					);
				},
			}
		};
		DotHttpRpcClient { online_client, rpc_methods: LegacyRpcMethods::new(rpc_client) }
	}
}

impl DotHttpRpcClient {
	#[cfg(test)]
	pub async fn new(url: SecretUrl, expected_genesis_hash: Option<PolkadotHash>) -> Result<Self> {
		Ok(DotHttpRpcClientBuilder::new(url, expected_genesis_hash)?.connect().await)
	}

	pub async fn metadata(&self, block_hash: PolkadotHash) -> Result<subxt::Metadata> {
		let resp = self.rpc_methods.state_get_metadata(Some(block_hash)).await?;
		let metadata = subxt::Metadata::decode(&mut &resp.into_raw()[..])?;
		Ok(metadata)
	}
}

#[async_trait::async_trait]
impl DotRpcApi for DotHttpRpcClient {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>> {
		Ok(self.rpc_methods.chain_get_block_hash(Some(block_number.into())).await?)
	}

	async fn block(
		&self,
		block_hash: PolkadotHash,
	) -> Result<Option<BlockDetails<PolkadotConfig>>> {
		Ok(self.rpc_methods.chain_get_block(Some(block_hash)).await?)
	}

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Result<Option<Vec<Bytes>>> {
		Ok(self.block(block_hash).await?.map(|block| block.block.extrinsics))
	}

	// TODO: When witnessing is catching up we query blocks in batches. It's possible that when
	// a batch is made over a runtime boundary that the metadata will need to be queried more than
	// necessary, as the order within the batch is not necessarily guaranteed. Because we limit
	// Polkadot to 32 concurrent requests and runtime upgrades are infrequent this should not be an
	// issue in reality, but probably worth solving at some point.
	async fn events(
		&self,
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
	) -> Result<Option<Events<PolkadotConfig>>> {
		// We need to get the runtime version at the previous block instead the desired block
		// because the events in the block are encoded using the previous block's runtime version,
		// not the desired block's runtime version. This is caused by the `state_getRuntimeVersion`
		// RPC returning the value of the runtime at the end of the block, not the beginning.
		let chain_runtime_version = self.runtime_version(Some(parent_hash)).await?;

		let client_runtime_version = self.online_client.runtime_version();

		// We set the metadata and runtime version we need to decode this block's events.
		// The metadata from the OnlineClient is used within the EventsClient to decode the
		// events.
		if chain_runtime_version.spec_version != client_runtime_version.spec_version ||
			chain_runtime_version.transaction_version !=
				client_runtime_version.transaction_version
		{
			tracing::debug!(
				"Setting runtime version of {chain_runtime_version:?} and metadata for block {:?}",
				block_hash
			);
			let new_metadata = self.metadata(parent_hash).await?;

			self.online_client.set_runtime_version(subxt::client::RuntimeVersion {
				spec_version: chain_runtime_version.spec_version,
				transaction_version: chain_runtime_version.transaction_version,
			});
			self.online_client.set_metadata(new_metadata);
		}

		// If we've succeeded in getting the current runtime version then we assume
		// the connection is stable (or has just been refreshed), no need to retry again.
		match EventsClient::new(self.online_client.clone()).at(block_hash).await {
			Ok(events) => Ok(Some(events)),
			Err(e) => match e {
				subxt::Error::Block(BlockError::NotFound(_)) => Ok(None),
				_ => Err(e.into()),
			},
		}
	}

	async fn runtime_version(&self, block_hash: Option<PolkadotHash>) -> Result<RuntimeVersion> {
		Ok(self.rpc_methods.state_get_runtime_version(block_hash).await.map(|v| {
			RuntimeVersion {
				spec_version: v.spec_version,
				transaction_version: v.transaction_version,
			}
		})?)
	}

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		let success = subxt::tx::SubmittableTransaction::<PolkadotConfig, _>::from_bytes(
			self.online_client.clone(),
			encoded_bytes,
		)
		.submit_and_watch()
		.await?
		.wait_for_finalized_success()
		.await?;

		Ok(success.extrinsic_hash())
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	#[ignore = "requires local node"]
	#[tokio::test]
	async fn test_http_rpc() {
		let dot_http_rpc =
			DotHttpRpcClient::new("http://localhost:9945".into(), None).await.unwrap();
		let block_hash = dot_http_rpc.block_hash(1).await.unwrap();
		println!("block_hash: {:?}", block_hash);
	}

	#[ignore = "requires local node"]
	#[tokio::test]
	async fn test_ws_rpc() {
		let dot_http_rpc = DotHttpRpcClient::new("ws://localhost:9945".into(), None).await.unwrap();
		let block_hash = dot_http_rpc.block_hash(1).await.unwrap();
		println!("block_hash: {:?}", block_hash);
	}

	#[ignore = "Uses public mainnet polkadot endpoint"]
	#[tokio::test]
	async fn test_parsing_events_in_runtime_update_block() {
		use std::str::FromStr;

		// Block hash of the block that a runtime update occurred in. Using 2 different blocks with
		// runtime updates to test.
		let block_hash_of_runtime_updates = vec![
			PolkadotHash::from_str(
				"0xa0b52be60216f8e0f2eb5bd17fa3c66908cc1652f3080a90d3ab20b2d352b610",
			)
			.unwrap(),
			PolkadotHash::from_str(
				"0xa0138c9d6686f9d80c3fa8a7e175951842ca400f43e479ba694d6d4da69969ea",
			)
			.unwrap(),
			// runtime upgrade block
			PolkadotHash::from_str(
				"0xb2c53eb7137113a73bdc02c7bd90a55a70b7b257d451453024d8b04122c30924",
			)
			.unwrap(),
			// next block was failing here
			PolkadotHash::from_str(
				"0x2c10ed1032a734cbcc93d7ba033a8ec9fa1b54e8ef1f121fe63a77bc1288e00b",
			)
			.unwrap(),
			// block with 4 dot transfer :( that was missed
			PolkadotHash::from_str(
				"0x0901b861c6db91f7f417a2fa20f3c82f005631f7d441a2a9e8fa5e2e55c6624c",
			)
			.unwrap(),
		];

		let dot_http_rpc =
			DotHttpRpcClientBuilder::new("https://polkadot-rpc-tn.dwellir.com:443".into(), None)
				.unwrap()
				.connect()
				.await;

		for block_hash in block_hash_of_runtime_updates {
			println!("TRYING BLOCK: {:?}", block_hash);

			// Block hash of the block before the runtime update occurred
			let block_hash_of_parent =
				dot_http_rpc.block(block_hash).await.unwrap().unwrap().block.header.parent_hash;

			// Get the events for the block with the runtime update in it
			let events =
				dot_http_rpc.events(block_hash, block_hash_of_parent).await.unwrap().unwrap();

			// Calling iter() will cause the events to be decoded. None of the events should fail to
			// decode if the correct metadata is used.
			assert!(!events.iter().any(|event| event.is_err()));

			// Check that mapping the events does not panic
			events.iter().filter_map(crate::witness::dot::filter_map_events).for_each(drop);
		}
	}

	#[test]
	fn test_ensure_port() {
		fn call_ensure(url: String) -> String {
			ensure_port(url.into()).unwrap().into()
		}
		let examples = vec![
			// default ports are added
			("https://www.google.com/mypath?query", "https://www.google.com:443/mypath?query"),
			("http://1.1.1.1?query", "http://1.1.1.1:80/?query"),
			("ws://[2001:db8:85a3:8d3:1319::]/path", "ws://[2001:db8:85a3:8d3:1319::]:80/path"),
			("wss://[::1]/path", "wss://[::1]:443/path"),
			// existing default ports are preserved
			("https://1.1.1.1:443/path_and?query", "https://1.1.1.1:443/path_and?query"),
			("http://1.1.1.1:80/path_and?query", "http://1.1.1.1:80/path_and?query"),
			("wss://1.1.1.1:443/path_and?query", "wss://1.1.1.1:443/path_and?query"),
			("ws://1.1.1.1:80/path_and?query", "ws://1.1.1.1:80/path_and?query"),
			// existing non-default ports are preserved
			("https://1.1.1.1:1234/path_and?query", "https://1.1.1.1:1234/path_and?query"),
			("http://1.1.1.1:443/path_and?query", "http://1.1.1.1:443/path_and?query"),
			("wss://1.1.1.1:5000/path_and?query", "wss://1.1.1.1:5000/path_and?query"),
			("ws://1.1.1.1:222/path_and?query", "ws://1.1.1.1:222/path_and?query"),
		];

		for (input, output) in examples {
			assert_eq!(call_ensure(input.to_string()), output.to_string());
		}
	}
}
