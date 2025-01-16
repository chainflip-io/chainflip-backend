use crate::{
	signed_client::{SignedPoolClient, WaitFor},
	RpcResult,
};
use jsonrpsee::{core::async_trait, proc_macros::rpc};
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, HeaderBackend, StorageProvider,
};
use sp_api::{CallApiAt, Core};
use sp_runtime::traits::Block as BlockT;
use state_chain_runtime::{runtime_apis::CustomRuntimeApi, AccountId, Nonce, RuntimeCall};
use std::sync::Arc;

#[rpc(server, client, namespace = "lp")]
pub trait LpSignedApi {
	#[method(name = "register_account")]
	async fn register_account(&self) -> RpcResult<state_chain_runtime::Hash>;

	// async fn request_liquidity_deposit_address(
	//     &self,
	//     asset: Asset,
	//     wait_for: Option<WaitFor>,
	//     boost_fee: Option<BasisPoints>,
	// ) -> RpcResult<ApiWaitForResult<String>>;
}

/// An LP signed RPC extension for the state chain node.
pub struct LpSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>
		+ sp_api::ProvideRuntimeApi<B>
		+ sp_runtime::traits::BlockIdTo<B>,
	C::Api: CustomRuntimeApi<B>
		+ Core<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	pub client: Arc<C>,
	pub signed_pool_client: SignedPoolClient<C, B, BE>,
}

#[async_trait]
impl<C, B, BE> LpSignedApiServer for LpSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>
		+ sp_api::ProvideRuntimeApi<B>
		+ sp_runtime::traits::BlockIdTo<B>,
	C::Api: CustomRuntimeApi<B>
		+ Core<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	async fn register_account(&self) -> RpcResult<state_chain_runtime::Hash> {
		let details = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_lp::Call::register_lp_account {}),
				WaitFor::InBlock,
				true,
				None,
			)
			.await?;

		//Ok(format!("{:#x}", details.tx_hash))
		Ok(details.tx_hash)
	}
}
