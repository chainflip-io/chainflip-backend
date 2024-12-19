use crate::{
	crypto::SubxtSignerInterface, internal_error, subxt_state_chain_config::StateChainConfig,
	RpcResult,
};
use codec::Decode;
use jsonrpsee::{core::async_trait, proc_macros::rpc};
use sc_client_api::{Backend, BlockBackend, HeaderBackend};
use sp_api::Core;
use sp_runtime::traits::Block as BlockT;
use state_chain_runtime::{runtime_apis::CustomRuntimeApi, Hash};
use std::{marker::PhantomData, sync::Arc};
use subxt::{ext::frame_metadata, OfflineClient, OnlineClient};

#[rpc(server, client, namespace = "broker")]
pub trait BrokerSignedApi {
	#[method(name = "send_remark")]
	async fn cf_send_remark(&self) -> RpcResult<()>;
}

/// An Broker signed RPC extension for the state chain node.
pub struct BrokerSignedRpc<C, B, BE> {
	pub client: Arc<C>,
	pub backend: Arc<BE>,
	pub executor: Arc<dyn sp_core::traits::SpawnNamed>,
	pub _phantom: PhantomData<B>,
	pub signer: SubxtSignerInterface<sp_core::sr25519::Pair>,
}

impl<C, B, BE> BrokerSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send + Sync + 'static + BlockBackend<B> + HeaderBackend<B> + sp_api::ProvideRuntimeApi<B>,
	C::Api: CustomRuntimeApi<B> + Core<B>,
{
	pub fn with_offline_subxt(
		&self,
		at: Option<Hash>,
	) -> RpcResult<OfflineClient<StateChainConfig>> {
		let genesis_hash =
			self.client.block_hash(0).ok().flatten().expect("Genesis block exists; qed");
		let hash = at.unwrap_or_else(|| self.client.info().best_hash);
		let version = self.client.runtime_api().version(hash)?;

		let metadata = frame_metadata::RuntimeMetadataPrefixed::decode(
			&mut state_chain_runtime::Runtime::metadata_at_version(15)
				.expect("Version 15 should be supported by the runtime.")
				.as_slice(),
		)
		.expect("Runtime metadata should be valid.");

		Ok(OfflineClient::<StateChainConfig>::new(
			genesis_hash,
			subxt::client::RuntimeVersion {
				spec_version: version.spec_version,
				transaction_version: version.transaction_version,
			},
			subxt::Metadata::try_from(metadata).map_err(internal_error)?,
		))
	}
}

impl<C, B, BE> BrokerSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send + Sync + 'static + BlockBackend<B> + HeaderBackend<B> + sp_api::ProvideRuntimeApi<B>,
	C::Api: CustomRuntimeApi<B> + Core<B>,
{
	pub async fn with_online_subxt(&self) -> RpcResult<OnlineClient<StateChainConfig>> {
		Ok(OnlineClient::<StateChainConfig>::new().await.map_err(internal_error)?)
	}
}

#[async_trait]
impl<C, B, BE> BrokerSignedApiServer for BrokerSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send + Sync + 'static + BlockBackend<B> + HeaderBackend<B> + sp_api::ProvideRuntimeApi<B>,
	C::Api: CustomRuntimeApi<B> + Core<B>,
{
	async fn cf_send_remark(&self) -> RpcResult<()> {
		let subxt = self.with_online_subxt().await?;

		let tx_payload = subxt::dynamic::tx(
			"System",
			"remark",
			vec![subxt::dynamic::Value::from_bytes("Hello from Chainflip RPC 2.0")],
		);

		let _events = subxt
			.tx()
			.sign_and_submit_then_watch_default(&tx_payload, &self.signer)
			.await
			.map_err(internal_error)?;

		Ok(())
	}
}
