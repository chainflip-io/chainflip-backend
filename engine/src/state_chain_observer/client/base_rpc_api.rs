use async_trait::async_trait;

use jsonrpsee::core::{
	client::{ClientT, Subscription, SubscriptionClientT},
	RpcResult,
};
use sc_transaction_pool_api::TransactionStatus;
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
	author::AuthorApiClient,
	chain::ChainApiClient,
	state::StateApiClient,
	system::{Health, SystemApiClient},
};

use futures::future::BoxFuture;
use serde_json::value::RawValue;
use std::sync::Arc;
use subxt::backend::rpc::RawRpcSubscription;

#[cfg(test)]
use mockall::automock;

use super::SUBSTRATE_BEHAVIOUR;

pub trait RawRpcApi:
	ClientT
	+ SubscriptionClientT
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
	> + substrate_frame_rpc_system::SystemApiClient<
		state_chain_runtime::Hash,
		state_chain_runtime::AccountId,
		state_chain_runtime::Nonce,
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
			> + substrate_frame_rpc_system::SystemApiClient<
				state_chain_runtime::Block,
				state_chain_runtime::AccountId,
				state_chain_runtime::Nonce,
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
	async fn health(&self) -> RpcResult<Health>;

	async fn next_account_nonce(
		&self,
		account_id: state_chain_runtime::AccountId,
	) -> RpcResult<state_chain_runtime::Nonce>;

	async fn submit_extrinsic(
		&self,
		extrinsic: state_chain_runtime::UncheckedExtrinsic,
	) -> RpcResult<sp_core::H256>;

	async fn submit_and_watch_extrinsic(
		&self,
		extrinsic: state_chain_runtime::UncheckedExtrinsic,
	) -> RpcResult<Subscription<TransactionStatus<sp_core::H256, sp_core::H256>>>;

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

	async fn latest_unfinalized_block_hash(&self) -> RpcResult<state_chain_runtime::Hash>;

	async fn subscribe_finalized_block_headers(
		&self,
	) -> RpcResult<Subscription<sp_runtime::generic::Header<u32, BlakeTwo256>>>;

	async fn subscribe_unfinalized_block_headers(
		&self,
	) -> RpcResult<Subscription<sp_runtime::generic::Header<u32, BlakeTwo256>>>;

	async fn runtime_version(&self) -> RpcResult<RuntimeVersion>;

	async fn dry_run(
		&self,
		extrinsic: Bytes,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Bytes>;

	async fn request_raw(
		&self,
		method: &str,
		params: Option<Box<RawValue>>,
	) -> RpcResult<Box<RawValue>>;

	async fn subscribe_raw(
		&self,
		sub: &str,
		params: Option<Box<RawValue>>,
		unsub: &str,
	) -> RpcResult<Subscription<Box<RawValue>>>;
}

pub struct BaseRpcClient<RawRpcClient> {
	pub raw_rpc_client: RawRpcClient,
}
impl<RawRpcClient> BaseRpcClient<RawRpcClient> {
	pub fn new(raw_rpc_client: RawRpcClient) -> Self {
		Self { raw_rpc_client }
	}
}

#[track_caller]
fn unwrap_value<T>(list_or_value: sp_rpc::list::ListOrValue<T>) -> T {
	match list_or_value {
		sp_rpc::list::ListOrValue::Value(value) => value,
		_ => panic!("{SUBSTRATE_BEHAVIOUR}"),
	}
}

#[async_trait]
impl<RawRpcClient: RawRpcApi + Send + Sync> BaseRpcApi for BaseRpcClient<RawRpcClient> {
	async fn health(&self) -> RpcResult<Health> {
		self.raw_rpc_client.system_health().await
	}

	async fn next_account_nonce(
		&self,
		account_id: state_chain_runtime::AccountId,
	) -> RpcResult<state_chain_runtime::Nonce> {
		self.raw_rpc_client.nonce(account_id).await
	}

	async fn submit_extrinsic(
		&self,
		extrinsic: state_chain_runtime::UncheckedExtrinsic,
	) -> RpcResult<sp_core::H256> {
		self.raw_rpc_client.submit_extrinsic(Bytes::from(extrinsic.encode())).await
	}

	async fn submit_and_watch_extrinsic(
		&self,
		extrinsic: state_chain_runtime::UncheckedExtrinsic,
	) -> RpcResult<Subscription<TransactionStatus<sp_core::H256, sp_core::H256>>> {
		self.raw_rpc_client.watch_extrinsic(Bytes::from(extrinsic.encode())).await
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
		Ok(self.raw_rpc_client.header(Some(block_hash)).await?.expect(SUBSTRATE_BEHAVIOUR))
	}

	async fn latest_unfinalized_block_hash(&self) -> RpcResult<state_chain_runtime::Hash> {
		Ok(unwrap_value(self.raw_rpc_client.block_hash(None).await?).expect(SUBSTRATE_BEHAVIOUR))
	}

	async fn latest_finalized_block_hash(&self) -> RpcResult<state_chain_runtime::Hash> {
		self.raw_rpc_client.finalized_head().await
	}

	async fn subscribe_finalized_block_headers(
		&self,
	) -> RpcResult<Subscription<sp_runtime::generic::Header<u32, BlakeTwo256>>> {
		self.raw_rpc_client.subscribe_finalized_heads().await
	}

	async fn subscribe_unfinalized_block_headers(
		&self,
	) -> RpcResult<Subscription<sp_runtime::generic::Header<u32, BlakeTwo256>>> {
		self.raw_rpc_client.subscribe_new_heads().await
	}

	async fn runtime_version(&self) -> RpcResult<RuntimeVersion> {
		self.raw_rpc_client.runtime_version(None).await
	}

	async fn dry_run(
		&self,
		extrinsic: Bytes,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Bytes> {
		self.raw_rpc_client.dry_run(extrinsic, at).await
	}

	async fn request_raw(
		&self,
		method: &str,
		params: Option<Box<RawValue>>,
	) -> RpcResult<Box<RawValue>> {
		self.raw_rpc_client.request(method, Params(params)).await
	}

	async fn subscribe_raw(
		&self,
		sub: &str,
		params: Option<Box<RawValue>>,
		unsub: &str,
	) -> RpcResult<Subscription<Box<RawValue>>> {
		self.raw_rpc_client.subscribe(sub, Params(params), unsub).await
	}
}

struct Params(Option<Box<RawValue>>);

impl jsonrpsee::core::traits::ToRpcParams for Params {
	fn to_rpc_params(self) -> RpcResult<Option<Box<RawValue>>> {
		Ok(self.0)
	}
}

pub struct SubxtInterface<T>(pub T);

impl<T: BaseRpcApi + Send + Sync + 'static> subxt::backend::rpc::RpcClientT
	for SubxtInterface<Arc<T>>
{
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> BoxFuture<'a, Result<Box<RawValue>, subxt::error::RpcError>> {
		Box::pin(async move {
			self.0
				.request_raw(method, params)
				.await
				.map_err(|e| subxt::error::RpcError::ClientError(Box::new(e)))
		})
	}

	fn subscribe_raw<'a>(
		&'a self,
		sub: &'a str,
		params: Option<Box<RawValue>>,
		unsub: &'a str,
	) -> BoxFuture<'a, Result<RawRpcSubscription, subxt::error::RpcError>> {
		Box::pin(async move {
			let stream = self
				.0
				.subscribe_raw(sub, params, unsub)
				.await
				.map_err(|e| subxt::error::RpcError::ClientError(Box::new(e)))?;

			let id = match stream.kind() {
				jsonrpsee::core::client::SubscriptionKind::Subscription(
					jsonrpsee::types::SubscriptionId::Str(id),
				) => Some(id.clone().into_owned()),
				_ => None,
			};

			use futures::{StreamExt, TryStreamExt};

			let stream =
				stream.map_err(|e| subxt::error::RpcError::ClientError(Box::new(e))).boxed();
			Ok(RawRpcSubscription { stream, id })
		})
	}
}
