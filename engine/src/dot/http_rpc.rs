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

use std::sync::{Arc, OnceLock};

use cf_chains::dot::{PolkadotAccountId, RuntimeVersion};
use cf_primitives::{
	chains::assets::hub::Asset as HubAsset, PolkadotBlockNumber, ASSETHUB_USDC_ASSET_ID,
	ASSETHUB_USDT_ASSET_ID,
};
use http::uri::Uri;
use jsonrpsee::{
	core::{client::ClientT, traits::ToRpcParams},
	http_client::{HttpClient, HttpClientBuilder},
};
use subxt::{
	backend::LegacyBackend,
	config::RpcConfigFor,
	error::OnlineClientAtBlockError,
	rpcs::{
		self as subxt_rpcs,
		client::{RawRpcFuture, RawRpcSubscription, RawValue, RpcClient, RpcClientT},
		methods::legacy::{BlockDetails, Bytes, LegacyRpcMethods},
	},
	OnlineClient, PolkadotConfig,
};
use url::Url;

use anyhow::{anyhow, Result};
use cf_utilities::{make_periodic_tick, redact_endpoint_secret::SecretUrl};
use tracing::{error, warn};

use crate::{
	constants::RPC_RETRY_CONNECTION_INTERVAL,
	dot::{PolkadotEvents, PolkadotHeader},
	witness::hub::assethub::{self, runtime_types::pallet_assets::types::AccountStatus},
};

use super::rpc::DotRpcApi;

use crate::dot::PolkadotHash;

// Substrate transaction pool rejection code (see substrate's `sc-rpc-api` author errors) meaning
// the pool already holds this *exact* transaction, so resubmitting it is a harmless no-op.
const POOL_ALREADY_IMPORTED: i32 = 1013;

#[derive(Clone)]
pub struct PolkadotHttpClient(HttpClient);

impl PolkadotHttpClient {
	pub fn new(url: &SecretUrl) -> Result<Self> {
		Ok(Self(HttpClientBuilder::default().build(url)?))
	}
}

struct Params(Option<Box<RawValue>>);

impl ToRpcParams for Params {
	fn to_rpc_params(self) -> Result<Option<Box<RawValue>>, serde_json::Error> {
		Ok(self.0)
	}
}

impl RpcClientT for PolkadotHttpClient {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		Box::pin(async move {
			let res =
				self.0.request(method, Params(params)).await.map_err(subxt_rpcs::Error::from)?;
			Ok(res)
		})
	}

	fn subscribe_raw<'a>(
		&'a self,
		_sub: &'a str,
		_params: Option<Box<RawValue>>,
		_unsub: &'a str,
	) -> RawRpcFuture<'a, RawRpcSubscription> {
		unimplemented!("HTTP Client does not support subscription");
	}
}

/// Adds a default port to the URL based on its scheme,
/// if none exists. Otherwise preservers existing port.
///
/// This function assumes that `url` is already validated, i.e.:
///  - It's accepted by `Url::parse()`
///  - It includes a host part
///  - It does not have a fragment part
fn ensure_port(url: &SecretUrl) -> Result<SecretUrl> {
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
pub struct DotRpcClientBuilder {
	http_url: SecretUrl,
	http_rpc_client: Arc<OnceLock<DotRpcClient>>,
	expected_genesis_hash: Option<PolkadotHash>,
}

#[derive(Clone)]
pub struct DotRpcClient {
	online_client: OnlineClient<PolkadotConfig>,
	rpc_methods: LegacyRpcMethods<RpcConfigFor<PolkadotConfig>>,
}

impl DotRpcClientBuilder {
	pub fn new(http_url: SecretUrl, expected_genesis_hash: Option<PolkadotHash>) -> Result<Self> {
		// Currently, the jsonrpsee library used by the PolkadotHttpClient expects
		// a port number to be always present in the url. Here we ensure this,
		// adding the default port if none is present.
		let http_url = ensure_port(&http_url)?;

		Ok(Self { http_url, expected_genesis_hash, http_rpc_client: Arc::new(OnceLock::new()) })
	}

	/// Creates or returns a cached http client.
	pub async fn http_client(&self) -> DotRpcClient {
		if let Some(client) = self.http_rpc_client.get() {
			client.clone()
		} else {
			let new_client = self.connect().await;
			let _ = self.http_rpc_client.set(new_client.clone());
			new_client
		}
	}

	async fn connect(&self) -> DotRpcClient {
		// We don't want to return an error here. Returning an error means that we'll exit the
		// CFE. So on client creation we wait until we can be successfully connected to the
		// Polkadot node. So the other chains are unaffected
		let url = self.http_url.clone();
		let mut poll_interval = make_periodic_tick(RPC_RETRY_CONNECTION_INTERVAL, true);
		let (rpc_client, online_client) = loop {
			poll_interval.tick().await;

			let rpc_client = PolkadotHttpClient::new(&url).map(RpcClient::new);

			let maybe_online_client: anyhow::Result<_> = match rpc_client {
				Ok(rpc_client) => OnlineClient::<PolkadotConfig>::from_backend_with_config(
					// Historic types are only needed for blocks with pre-V14 metadata.
					PolkadotConfig::builder().use_historic_types(false).build(),
					// The HTTP client doesn't support the subscriptions other backends need.
					Arc::new(LegacyBackend::builder().build(rpc_client.clone())),
				)
				.await
				.map(move |a| (rpc_client, a))
				.map_err(|e| e.into()),
				Err(e) => Err(e),
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
								url
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
						url,
						poll_interval.period()
					);
				},
			}
		};

		DotRpcClient { online_client, rpc_methods: LegacyRpcMethods::new(rpc_client) }
	}
}

#[async_trait::async_trait]
impl DotRpcApi for DotRpcClient {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>> {
		Ok(self.rpc_methods.chain_get_block_hash(Some(block_number.into())).await?)
	}

	async fn finalized_head(&self) -> Result<PolkadotHash> {
		Ok(self.rpc_methods.chain_get_finalized_head().await?)
	}

	async fn header(&self, block_hash: PolkadotHash) -> Result<Option<PolkadotHeader>> {
		Ok(self.rpc_methods.chain_get_header(Some(block_hash)).await?)
	}

	async fn block(
		&self,
		block_hash: PolkadotHash,
	) -> Result<Option<BlockDetails<RpcConfigFor<PolkadotConfig>>>> {
		Ok(self.rpc_methods.chain_get_block(Some(block_hash)).await?)
	}

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Result<Option<Vec<Bytes>>> {
		Ok(self.block(block_hash).await?.map(|block| block.block.extrinsics))
	}

	async fn events(
		&self,
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
	) -> Result<Option<PolkadotEvents>> {
		// Events are encoded using the runtime that executed the block, which is the one in the
		// parent block's state: in a runtime upgrade block the new runtime is only in the state at
		// the end of the block.
		let at_parent = match self.online_client.at_block(parent_hash).await {
			Ok(at_parent) => at_parent,
			Err(OnlineClientAtBlockError::BlockHeaderNotFound { .. }) => return Ok(None),
			Err(e) => return Err(e.into()),
		};

		let event_bytes = self
			.rpc_methods
			.state_get_storage(
				&frame_support::storage::storage_prefix(b"System", b"Events"),
				Some(block_hash),
			)
			.await?
			.unwrap_or_default();

		Ok(Some(Arc::new(at_parent.events().from_bytes(event_bytes))))
	}

	async fn runtime_version(&self, block_hash: Option<PolkadotHash>) -> Result<RuntimeVersion> {
		Ok(self.rpc_methods.state_get_runtime_version(block_hash).await.map(|v| {
			RuntimeVersion {
				spec_version: v.spec_version,
				transaction_version: v.transaction_version,
			}
		})?)
	}

	/// Submits a raw encoded extrinsic, returning its hash once it is accepted into the
	/// transaction pool.
	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		// We use `author_submit_extrinsic` because subxt's submit() uses a subscription to wait
		// for the first transaction status, which the HTTP client doesn't support.
		match self.rpc_methods.author_submit_extrinsic(&encoded_bytes).await {
			Err(subxt_rpcs::Error::User(user_error))
				if user_error.code == POOL_ALREADY_IMPORTED =>
				Ok(sp_crypto_hashing::blake2_256(&encoded_bytes).into()),
			result => Ok(result?),
		}
	}

	async fn liquid_account_balance(
		&self,
		account_id: PolkadotAccountId,
		asset: HubAsset,
		block_hash: PolkadotHash,
	) -> Result<u128> {
		let account_id = subxt::utils::AccountId32(account_id.0);
		Ok(match asset {
			HubAsset::HubDot => {
				self.online_client
					.at_block(block_hash)
					.await?
					.storage()
					.try_fetch(assethub::storage().system().account(), (account_id,))
					.await?
					.map(|value| value.decode())
					.transpose()?
					.map(|account_info| {
						account_info.data.free.saturating_sub(account_info.data.frozen)
					})
					// No account => no balance
					.unwrap_or_default()
			},
			other => {
				let id = match other {
					HubAsset::HubUsdc => ASSETHUB_USDC_ASSET_ID,
					HubAsset::HubUsdt => ASSETHUB_USDT_ASSET_ID,
					HubAsset::HubDot => unreachable!(),
				};
				self.online_client
					.at_block(block_hash)
					.await?
					.storage()
					.try_fetch(assethub::storage().assets().account(), (id, account_id))
					.await?
					.map(|value| value.decode())
					.transpose()?
					.map(|asset_account| {
						// `AccountStatus` is a subxt-generated enum that does not derive
						// `PartialEq`, so use `matches!` rather than `==`.
						if matches!(asset_account.status, AccountStatus::Liquid) {
							asset_account.balance
						} else {
							0
						}
					})
					// No account => no balance
					.unwrap_or_default()
			},
		})
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	#[ignore = "requires local node"]
	#[tokio::test]
	async fn test_http_rpc() {
		let dot_http_rpc = DotRpcClientBuilder::new("http://localhost:9945".into(), None)
			.unwrap()
			.http_client()
			.await;
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
			DotRpcClientBuilder::new("https://polkadot-rpc-tn.dwellir.com:443".into(), None)
				.unwrap()
				.http_client()
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
			ensure_port(&url.into()).unwrap().into()
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
