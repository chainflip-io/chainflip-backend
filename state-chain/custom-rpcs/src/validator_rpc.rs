use jsonrpc_derive::rpc;
use sc_client_api::HeaderBackend;
use state_chain_runtime::runtime_apis::ValidatorRuntimeApi;
use std::{marker::PhantomData, sync::Arc};

#[rpc]
pub trait ValidatorApi {
	/// Returns the anwser to the meaning of live
	///
	/// Usage:
	///
	/// curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d '{
	///     "jsonrpc":"2.0",
	///     "id":1,
	///     "method":"ask",
	///     "params": []
	/// }'
	#[rpc(name = "is_auction_phase")]
	fn is_auction_phase(&self) -> Result<bool, jsonrpc_core::Error>;
}

pub struct ValidatorRpc<C, B> {
	pub client: Arc<C>,
	pub _phantom: PhantomData<B>,
}

impl<C, B> ValidatorApi for ValidatorRpc<C, B>
where
	B: sp_runtime::traits::Block,
	C: sp_api::ProvideRuntimeApi<B>,
	C: Send + Sync + 'static,
	C: HeaderBackend<B>,
	C::Api: ValidatorRuntimeApi<B>,
{
	fn is_auction_phase(&self) -> Result<bool, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.is_auction_phase(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
}
