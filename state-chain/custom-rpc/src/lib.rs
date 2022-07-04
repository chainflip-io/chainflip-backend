use cf_chains::eth::SigData;
use jsonrpc_core::serde::{Deserialize, Serialize};
use jsonrpc_derive::rpc;
use sc_client_api::HeaderBackend;
use sp_rpc::number::NumberOrHex;
use sp_runtime::AccountId32;
use state_chain_runtime::{
	chainflip::Offence, constants::common::TX_FEE_MULTIPLIER, runtime_apis::CustomRuntimeApi,
	ChainflipAccountState,
};
use std::{marker::PhantomData, sync::Arc};

pub use self::gen_client::Client as CustomClient;

#[derive(Serialize, Deserialize)]
pub struct RpcAccountInfo {
	pub stake: NumberOrHex,
	pub bond: NumberOrHex,
	pub last_heartbeat: u32,
	pub online_credits: u32,
	pub reputation_points: i32,
	pub withdrawal_address: String,
	pub state: ChainflipAccountState,
}

#[derive(Serialize, Deserialize)]
pub struct RpcPendingClaim {
	amount: NumberOrHex,
	address: String,
	expiry: NumberOrHex,
	sig_data: SigData,
}

#[derive(Serialize, Deserialize)]
pub struct RpcPenalty {
	reputation_points: i32,
	suspension_duration_blocks: u32,
}

type RpcSuspensions = Vec<(Offence, Vec<(u32, AccountId32)>)>;

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
	/// Returns the eth vault in the form [agg_key, active_from_eth_block]
	#[rpc(name = "cf_eth_vault")]
	fn cf_eth_vault(&self) -> Result<(String, u32), jsonrpc_core::Error>;
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
	#[rpc(name = "cf_flip_supply")]
	fn cf_flip_supply(&self) -> Result<(NumberOrHex, NumberOrHex), jsonrpc_core::Error>;
	#[rpc(name = "cf_accounts")]
	fn cf_accounts(&self) -> Result<Vec<(AccountId32, String)>, jsonrpc_core::Error>;
	#[rpc(name = "cf_account_info")]
	fn cf_account_info(
		&self,
		account_id: AccountId32,
	) -> Result<RpcAccountInfo, jsonrpc_core::Error>;
	#[rpc(name = "cf_pending_claim")]
	fn cf_pending_claim(
		&self,
		account_id: AccountId32,
	) -> Result<Option<RpcPendingClaim>, jsonrpc_core::Error>;
	#[rpc(name = "cf_penalties")]
	fn cf_penalties(&self) -> Result<Vec<(Offence, RpcPenalty)>, jsonrpc_core::Error>;
	#[rpc(name = "cf_suspensions")]
	fn cf_suspensions(&self) -> Result<RpcSuspensions, jsonrpc_core::Error>;
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
	fn cf_eth_vault(&self) -> Result<(String, u32), jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		let eth_vault = self
			.client
			.runtime_api()
			.cf_eth_vault(&at)
			.expect("The runtime API should not return error.");

		Ok((hex::encode(eth_vault.0), eth_vault.1))
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
	fn cf_flip_supply(&self) -> Result<(NumberOrHex, NumberOrHex), jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		let (issuance, offchain) = self
			.client
			.runtime_api()
			.cf_flip_supply(&at)
			.expect("The runtime API should not return error.");
		Ok((issuance.into(), offchain.into()))
	}
	fn cf_accounts(&self) -> Result<Vec<(AccountId32, String)>, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		Ok(self
			.client
			.runtime_api()
			.cf_accounts(&at)
			.expect("The runtime API should not return error.")
			.into_iter()
			.map(|(account_id, vanity_name_bytes)| {
				// we can use from_utf8_lossy here because we're guaranteed utf8 when we
				// save the vanity name on the chain
				(account_id, String::from_utf8_lossy(&vanity_name_bytes).into_owned())
			})
			.collect())
	}
	fn cf_account_info(
		&self,
		account_id: AccountId32,
	) -> Result<RpcAccountInfo, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		let account_info = self
			.client
			.runtime_api()
			.cf_account_info(&at, account_id)
			.expect("The runtime API should not return error.");

		Ok(RpcAccountInfo {
			stake: account_info.stake.into(),
			bond: account_info.bond.into(),
			last_heartbeat: account_info.last_heartbeat,
			online_credits: account_info.online_credits,
			reputation_points: account_info.reputation_points,
			withdrawal_address: hex::encode(account_info.withdrawal_address),
			state: account_info.state,
		})
	}
	fn cf_pending_claim(
		&self,
		account_id: AccountId32,
	) -> Result<Option<RpcPendingClaim>, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		let pending_claim = match self
			.client
			.runtime_api()
			.cf_pending_claim(&at, account_id)
			.map_err(|_| jsonrpc_core::Error::internal_error())?
		{
			Some(pending_claim) => pending_claim,
			None => return Ok(None),
		};

		Ok(Some(RpcPendingClaim {
			amount: pending_claim.amount.into(),
			expiry: pending_claim.expiry.into(),
			address: hex::encode(pending_claim.address),
			sig_data: pending_claim.sig_data,
		}))
	}
	fn cf_penalties(&self) -> Result<Vec<(Offence, RpcPenalty)>, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		Ok(self
			.client
			.runtime_api()
			.cf_penalties(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
			.expect("The runtime API should not return error.")
			.iter()
			.map(|(offence, runtime_api_penalty)| {
				(
					*offence,
					RpcPenalty {
						reputation_points: runtime_api_penalty.reputation_points,
						suspension_duration_blocks: runtime_api_penalty.suspension_duration_blocks,
					},
				)
			})
			.collect())
	}
	fn cf_suspensions(&self) -> Result<RpcSuspensions, jsonrpc_core::Error> {
		let at = sp_api::BlockId::hash(self.client.info().best_hash);
		self.client
			.runtime_api()
			.cf_suspensions(&at)
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
}
