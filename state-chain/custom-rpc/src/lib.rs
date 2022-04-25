use jsonrpc_derive::rpc;
use sc_client_api::HeaderBackend;
use state_chain_runtime::runtime_apis::CustomRuntimeApi;
use std::{marker::PhantomData, sync::Arc};

#[rpc]
/// The custom RPC endoints for the state chain node.
pub trait CustomApi {
	/// Returns true if the current phase is the auction phase.
	#[rpc(name = "is_auction_phase")]
	fn is_auction_phase(&self) -> Result<bool, jsonrpc_core::Error>;
}

/// An RPC extension for the state chain node.
pub struct CustomRpc<C, B> {
	pub client: Arc<C>,
	pub _phantom: PhantomData<B>,
}

impl<C, B> CustomApi for CustomRpc<C, B>
where
	B: sp_runtime::traits::Block,
	C: sp_api::ProvideRuntimeApi<B>,
	C: Send + Sync + 'static,
	C: HeaderBackend<B>,
	C::Api: CustomRuntimeApi<B>,
{
	fn is_auction_phase(&self) -> Result<bool, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.is_auction_phase(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
}
