use jsonrpc_derive::rpc;
use sc_client_api::HeaderBackend;
use state_chain_runtime::runtime_apis::MeaningOfLiveRuntimeApi;
use std::{marker::PhantomData, sync::Arc};

#[rpc]
pub trait MeaningOfLiveApi {
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
	#[rpc(name = "ask")]
	fn ask(&self) -> Result<u32, jsonrpc_core::Error>;
}

pub struct MeaningOfLiveRpc<C, B> {
	pub client: Arc<C>,
	pub _phantom: PhantomData<B>,
}

impl<C, B> MeaningOfLiveApi for MeaningOfLiveRpc<C, B>
where
	B: sp_runtime::traits::Block,
	C: sp_api::ProvideRuntimeApi<B>,
	C: Send + Sync + 'static,
	C: HeaderBackend<B>,
	C::Api: MeaningOfLiveRuntimeApi<B>,
{
	fn ask(&self) -> Result<u32, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.ask(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
}
