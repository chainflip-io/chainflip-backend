use async_trait::async_trait;

use anyhow::Context;
use jsonrpsee::{
	core::{
		client::{ClientT, SubscriptionClientT},
		RpcResult,
	},
	ws_client::WsClientBuilder,
};
use sp_core::{
	storage::{StorageData, StorageKey},
	Bytes,
};
use sp_runtime::traits::BlakeTwo256;
use sp_version::RuntimeVersion;
use state_chain_runtime::SignedBlock;

use codec::Encode;
use custom_rpc::CustomApiClient;
use sc_rpc_api::{
	author::AuthorApiClient, chain::ChainApiClient, state::StateApiClient, system::SystemApiClient,
};

#[cfg(test)]
use mockall::automock;

use crate::settings;

pub trait RawRpcApi:
	CustomApiClient
	+ SystemApiClient<state_chain_runtime::Hash, state_chain_runtime::BlockNumber>
	+ StateApiClient<state_chain_runtime::Hash>
	+ AuthorApiClient<
		state_chain_runtime::Hash,
		<state_chain_runtime::Block as sp_runtime::traits::Block>::Hash,
	> + ChainApiClient<
		state_chain_runtime::BlockNumber,
		state_chain_runtime::Hash,
		state_chain_runtime::Header,
		state_chain_runtime::SignedBlock,
	>
{
}

impl<
		T: SubscriptionClientT
			+ ClientT
			+ CustomApiClient
			+ SystemApiClient<state_chain_runtime::Hash, state_chain_runtime::BlockNumber>
			+ StateApiClient<state_chain_runtime::Hash>
			+ AuthorApiClient<
				state_chain_runtime::Hash,
				<state_chain_runtime::Block as sp_runtime::traits::Block>::Hash,
			> + ChainApiClient<
				state_chain_runtime::BlockNumber,
				state_chain_runtime::Hash,
				state_chain_runtime::Header,
				state_chain_runtime::SignedBlock,
			>,
	> RawRpcApi for T
{
}

/// Wraps the substrate client library methods. This trait allows us to mock a State Chain RPC.
/// It assumes that provided block_hash's are valid as we would have gotten them from the
/// RPC itself, and so it panics if a provided block_hash is invalid i.e. doesn't exist.
/// For calls that use block_number instead we return an Option to indicate if the associated block
/// exists or not and do not ever panic. As in some cases we make requests for block
/// numbers the RPC has not previously provided.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait BaseRpcApi {
	async fn submit_extrinsic(
		&self,
		extrinsic: state_chain_runtime::UncheckedExtrinsic,
	) -> RpcResult<sp_core::H256>;

	async fn storage(
		&self,
		block_hash: state_chain_runtime::Hash,
		storage_key: StorageKey,
	) -> RpcResult<Option<StorageData>>;

	async fn storage_pairs(
		&self,
		block_hash: state_chain_runtime::Hash,
		storage_key: StorageKey,
	) -> RpcResult<Vec<(StorageKey, StorageData)>>;

	async fn block(&self, block_hash: state_chain_runtime::Hash) -> RpcResult<Option<SignedBlock>>;

	async fn block_hash(
		&self,
		block_number: state_chain_runtime::BlockNumber,
	) -> RpcResult<Option<state_chain_runtime::Hash>>;

	async fn block_header(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<state_chain_runtime::Header>;

	async fn latest_finalized_block_hash(&self) -> RpcResult<state_chain_runtime::Hash>;

	async fn subscribe_finalized_block_headers(
		&self,
	) -> RpcResult<
		jsonrpsee::core::client::Subscription<sp_runtime::generic::Header<u32, BlakeTwo256>>,
	>;

	async fn runtime_version(&self) -> RpcResult<RuntimeVersion>;
}

pub struct BaseRpcClient<RawRpcClient> {
	pub raw_rpc_client: RawRpcClient,
}

impl BaseRpcClient<jsonrpsee::ws_client::WsClient> {
	pub async fn new(state_chain_settings: &settings::StateChain) -> Result<Self, anyhow::Error> {
		let ws_endpoint = state_chain_settings.ws_endpoint.as_str();
		Ok(Self {
			raw_rpc_client: WsClientBuilder::default().build(ws_endpoint).await.with_context(
				|| format!("Failed to establish rpc connection to substrate node '{ws_endpoint}'"),
			)?,
		})
	}
}

fn unwrap_value<T>(list_or_value: sp_rpc::list::ListOrValue<T>) -> T {
	match list_or_value {
		sp_rpc::list::ListOrValue::Value(value) => value,
		_ => panic!(
			"Expected a Value of {0} actually received a List of {0}",
			std::any::type_name::<T>()
		),
	}
}

#[async_trait]
impl<RawRpcClient: RawRpcApi + Send + Sync> BaseRpcApi for BaseRpcClient<RawRpcClient> {
	async fn submit_extrinsic(
		&self,
		extrinsic: state_chain_runtime::UncheckedExtrinsic,
	) -> RpcResult<sp_core::H256> {
		self.raw_rpc_client.submit_extrinsic(Bytes::from(extrinsic.encode())).await
	}

	async fn storage(
		&self,
		block_hash: state_chain_runtime::Hash,
		storage_key: StorageKey,
	) -> RpcResult<Option<StorageData>> {
		self.raw_rpc_client.storage(storage_key, Some(block_hash)).await
	}

	async fn storage_pairs(
		&self,
		block_hash: state_chain_runtime::Hash,
		storage_key: StorageKey,
	) -> RpcResult<Vec<(StorageKey, StorageData)>> {
		self.raw_rpc_client.storage_pairs(storage_key, Some(block_hash)).await
	}

	async fn block(&self, block_hash: state_chain_runtime::Hash) -> RpcResult<Option<SignedBlock>> {
		self.raw_rpc_client.block(Some(block_hash)).await
	}

	async fn block_hash(
		&self,
		block_number: state_chain_runtime::BlockNumber,
	) -> RpcResult<Option<state_chain_runtime::Hash>> {
		Ok(unwrap_value(
			self.raw_rpc_client
				.block_hash(Some(sp_rpc::list::ListOrValue::Value(block_number.into())))
				.await?,
		))
	}

	async fn block_header(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<state_chain_runtime::Header> {
		Ok(self.raw_rpc_client.header(Some(block_hash)).await?.unwrap())
	}

	async fn latest_finalized_block_hash(&self) -> RpcResult<state_chain_runtime::Hash> {
		self.raw_rpc_client.finalized_head().await
	}

	async fn subscribe_finalized_block_headers(
		&self,
	) -> RpcResult<
		jsonrpsee::core::client::Subscription<sp_runtime::generic::Header<u32, BlakeTwo256>>,
	> {
		self.raw_rpc_client.subscribe_finalized_heads().await
	}

	async fn runtime_version(&self) -> RpcResult<RuntimeVersion> {
		self.raw_rpc_client.runtime_version(None).await
	}
}
