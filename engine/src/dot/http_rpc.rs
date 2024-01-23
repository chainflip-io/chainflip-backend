use cf_chains::dot::{PolkadotHash, RuntimeVersion};
use cf_primitives::PolkadotBlockNumber;
use futures_core::Future;
use jsonrpsee::{
	core::{client::ClientT, traits::ToRpcParams, Error as JsonRpseeError},
	http_client::{HttpClient, HttpClientBuilder},
};
use serde_json::value::RawValue;
use sp_core::H256;
use subxt::{
	backend::{
		legacy::{
			rpc_methods::{BlockDetails, Bytes},
			LegacyRpcMethods,
		},
		rpc::{RawRpcFuture, RawRpcSubscription, RpcClient, RpcClientT},
	},
	error::{BlockError, RpcError},
	events::{Events, EventsClient},
	OnlineClient, PolkadotConfig,
};

use anyhow::Result;
use tracing::{error, warn};
use utilities::{make_periodic_tick, redact_endpoint_secret::SecretUrl};

use crate::constants::RPC_RETRY_CONNECTION_INTERVAL;

use super::rpc::DotRpcApi;

pub struct PolkadotHttpClient(HttpClient);

impl PolkadotHttpClient {
	pub fn new(url: &SecretUrl) -> Result<Self> {
		Ok(Self(HttpClientBuilder::default().build(url)?))
	}
}

struct Params(Option<Box<RawValue>>);

impl ToRpcParams for Params {
	fn to_rpc_params(self) -> Result<Option<Box<RawValue>>, JsonRpseeError> {
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
			let res = self
				.0
				.request(method, Params(params))
				.await
				.map_err(|e| RpcError::ClientError(Box::new(e)))?;
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

#[derive(Clone)]
pub struct DotHttpRpcClient {
	online_client: OnlineClient<PolkadotConfig>,
	rpc_methods: LegacyRpcMethods<PolkadotConfig>,
}

impl DotHttpRpcClient {
	pub fn new(
		url: SecretUrl,
		expected_genesis_hash: Option<PolkadotHash>,
	) -> Result<impl Future<Output = Self>> {
		let rpc_client = RpcClient::new(PolkadotHttpClient::new(&url)?);

		Ok(async move {
			// We don't want to return an error here. Returning an error means that we'll exit the
			// CFE. So on client creation we wait until we can be successfully connected to the
			// Polkadot node. So the other chains are unaffected
			let mut poll_interval = make_periodic_tick(RPC_RETRY_CONNECTION_INTERVAL, true);
			let online_client = loop {
				poll_interval.tick().await;

				match OnlineClient::<PolkadotConfig>::from_rpc_client(rpc_client.clone()).await {
					Ok(online_client) => {
						if let Some(expected_genesis_hash) = expected_genesis_hash {
							let genesis_hash = online_client.genesis_hash();
							if genesis_hash == expected_genesis_hash {
								break online_client
							} else {
								error!(
									"Connected to Polkadot node at {url} but the genesis hash {genesis_hash} does not match the expected genesis hash {expected_genesis_hash}. Please check your CFE configuration file."
								)
							}
						} else {
							warn!("Skipping Polkadot genesis hash check");
							break online_client
						}
					},
					Err(e) => {
						error!(
							"Failed to connect to Polkadot node at {url} with error: {e}. \
						Please check your CFE configuration file. Retrying in {:?}...",
							poll_interval.period()
						);
					},
				}
			};
			Self { online_client, rpc_methods: LegacyRpcMethods::new(rpc_client) }
		})
	}

	pub async fn metadata(&self, block_hash: H256) -> Result<subxt::Metadata> {
		Ok(self.rpc_methods.state_get_metadata(Some(block_hash)).await?)
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

		let client_runtime_version = self.rpc_methods.state_get_runtime_version(None).await?;

		// We set the metadata and runtime version we need to decode this block's events.
		// The metadata from the OnlineClient is used within the EventsClient to decode the
		// events.
		if chain_runtime_version.spec_version != client_runtime_version.spec_version ||
			chain_runtime_version.transaction_version !=
				client_runtime_version.transaction_version
		{
			let new_metadata = self.metadata(parent_hash).await?;

			self.online_client.set_runtime_version(subxt::backend::RuntimeVersion {
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

	async fn runtime_version(&self, block_hash: Option<H256>) -> Result<RuntimeVersion> {
		Ok(self.rpc_methods.state_get_runtime_version(block_hash).await.map(|v| {
			RuntimeVersion {
				spec_version: v.spec_version,
				transaction_version: v.transaction_version,
			}
		})?)
	}

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		Ok(self.rpc_methods.author_submit_extrinsic(&encoded_bytes).await?)
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	#[ignore = "requires local node"]
	#[tokio::test]
	async fn test_http_rpc() {
		let dot_http_rpc =
			DotHttpRpcClient::new("http://localhost:9945".into(), None).unwrap().await;
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
			H256::from_str("0xa0b52be60216f8e0f2eb5bd17fa3c66908cc1652f3080a90d3ab20b2d352b610")
				.unwrap(),
			H256::from_str("0xa0138c9d6686f9d80c3fa8a7e175951842ca400f43e479ba694d6d4da69969ea")
				.unwrap(),
		];

		let dot_http_rpc =
			DotHttpRpcClient::new("https://polkadot-rpc-tn.dwellir.com:443".into(), None)
				.unwrap()
				.await;

		for block_hash in block_hash_of_runtime_updates {
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
}
