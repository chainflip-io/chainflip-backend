use jsonrpc_derive::rpc;
use sc_client_api::HeaderBackend;
use state_chain_runtime::runtime_apis::CustomRuntimeApi;
use std::{marker::PhantomData, sync::Arc};

pub use self::gen_client::Client as CustomClient;

#[rpc]
/// The custom RPC endoints for the state chain node.
pub trait CustomApi {
	/// Returns true if the current phase is the auction phase.
	#[rpc(name = "is_auction_phase")]
	fn is_auction_phase(&self) -> Result<bool, jsonrpc_core::Error>;
	#[rpc(name = "environment_key_manager_address")]
	fn environment_key_manager_address(&self) -> Result<[u8; 20], jsonrpc_core::Error>;
	#[rpc(name = "environment_stake_manager_address")]
	fn environment_stake_manager_address(&self) -> Result<[u8; 20], jsonrpc_core::Error>;
	#[rpc(name = "environment_flip_token_address")]
	fn environment_flip_token_address(&self) -> Result<[u8; 20], jsonrpc_core::Error>;
	#[rpc(name = "environment_ethereum_chain_id")]
	fn environment_ethereum_chain_id(&self) -> Result<u64, jsonrpc_core::Error>;
}

/// An RPC extension for the state chain node.
pub struct CustomRpc<C, B> {
	pub client: Arc<C>,
	pub _phantom: PhantomData<B>,
}

impl<C, B> CustomApi for CustomRpc<C, B>
where
	B: sp_runtime::traits::Block,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync + 'static + HeaderBackend<B>,
	C::Api: CustomRuntimeApi<B>,
{
	fn is_auction_phase(&self) -> Result<bool, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.is_auction_phase(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn environment_flip_token_address(&self) -> Result<[u8; 20], jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.environment_flip_token_address(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn environment_stake_manager_address(&self) -> Result<[u8; 20], jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.environment_stake_manager_address(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn environment_key_manager_address(&self) -> Result<[u8; 20], jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.environment_key_manager_address(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn environment_ethereum_chain_id(&self) -> Result<u64, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.environment_ethereum_chain_id(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
}
