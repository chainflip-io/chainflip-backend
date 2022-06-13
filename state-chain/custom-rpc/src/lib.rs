use jsonrpc_derive::rpc;
use sc_client_api::HeaderBackend;
use state_chain_runtime::{constants::common::TX_FEE_MULTIPLIER, runtime_apis::CustomRuntimeApi};
use std::{marker::PhantomData, sync::Arc};

pub use self::gen_client::Client as CustomClient;

#[rpc]
/// The custom RPC endoints for the state chain node.
pub trait CustomApi {
	/// Returns true if the current phase is the auction phase.
	#[rpc(name = "cf_is_auction_phase")]
	fn cf_is_auction_phase(&self) -> Result<bool, jsonrpc_core::Error>;
	#[rpc(name = "cf_eth_key_manager_address")]
	fn cf_eth_key_manager_address(&self) -> Result<[u8; 20], jsonrpc_core::Error>;
	#[rpc(name = "cf_eth_stake_manager_address")]
	fn cf_eth_stake_manager_address(&self) -> Result<[u8; 20], jsonrpc_core::Error>;
	#[rpc(name = "cf_eth_flip_token_address")]
	fn cf_eth_flip_token_address(&self) -> Result<[u8; 20], jsonrpc_core::Error>;
	#[rpc(name = "cf_eth_chain_id")]
	fn cf_eth_chain_id(&self) -> Result<u64, jsonrpc_core::Error>;
	#[rpc(name = "cf_tx_fee_multiplier")]
	fn cf_tx_fee_multiplier(&self) -> Result<u64, jsonrpc_core::Error>;
	// Returns the Auction params in the form [min_set_size, max_set_size]
	#[rpc(name = "cf_auction_parameters")]
	fn cf_auction_parameters(&self) -> Result<(u32, u32), jsonrpc_core::Error>;
	#[rpc(name = "cf_min_stake")]
	fn cf_min_stake(&self) -> Result<u64, jsonrpc_core::Error>;
	#[rpc(name = "cf_current_epoch")]
	fn cf_current_epoch(&self) -> Result<u32, jsonrpc_core::Error>;
	#[rpc(name = "cf_current_epoch_started_at")]
	fn cf_current_epoch_started_at(&self) -> Result<u32, jsonrpc_core::Error>;
	#[rpc(name = "cf_authority_emission_per_block")]
	fn cf_authority_emission_per_block(&self) -> Result<u64, jsonrpc_core::Error>;
	#[rpc(name = "cf_backup_emission_per_block")]
	fn cf_backup_emission_per_block(&self) -> Result<u64, jsonrpc_core::Error>;
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
	fn cf_is_auction_phase(&self) -> Result<bool, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_is_auction_phase(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_eth_flip_token_address(&self) -> Result<[u8; 20], jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_eth_flip_token_address(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_eth_stake_manager_address(&self) -> Result<[u8; 20], jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_eth_stake_manager_address(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_eth_key_manager_address(&self) -> Result<[u8; 20], jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_eth_key_manager_address(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_eth_chain_id(&self) -> Result<u64, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_eth_chain_id(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_tx_fee_multiplier(&self) -> Result<u64, jsonrpc_core::Error> {
		Ok(TX_FEE_MULTIPLIER
			.try_into()
			.expect("We never set a fee multiplier greater than u64::MAX"))
	}
	fn cf_auction_parameters(&self) -> Result<(u32, u32), jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_auction_parameters(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_min_stake(&self) -> Result<u64, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_min_stake(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_current_epoch(&self) -> Result<u32, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_current_epoch(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_current_epoch_started_at(&self) -> Result<u32, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_current_epoch_started_at(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_authority_emission_per_block(&self) -> Result<u64, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_authority_emission_per_block(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_backup_emission_per_block(&self) -> Result<u64, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_backup_emission_per_block(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
}
