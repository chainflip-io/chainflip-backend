use cf_chains::eth::SigData;
use cf_utilities::JsonResultExt;
use jsonrpc_core::serde::{Deserialize, Serialize};
use jsonrpc_derive::rpc;
use sc_client_api::HeaderBackend;
use sp_api::BlockT;
use sp_rpc::number::NumberOrHex;
use sp_runtime::AccountId32;
use state_chain_runtime::{
	chainflip::Offence,
	constants::common::TX_FEE_MULTIPLIER,
	runtime_apis::{ChainflipAccountStateWithPassive, CustomRuntimeApi},
};
use std::{marker::PhantomData, sync::Arc};

pub use self::gen_client::Client as CustomClient;

#[derive(Serialize, Deserialize)]
pub struct RpcAccountInfo {
	pub stake: NumberOrHex,
	pub bond: NumberOrHex,
	pub last_heartbeat: u32,
	pub is_live: bool,
	pub is_activated: bool,
	pub online_credits: u32,
	pub reputation_points: i32,
	pub withdrawal_address: String,
	pub state: ChainflipAccountStateWithPassive,
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
	fn cf_is_auction_phase(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<bool, jsonrpc_core::Error>;
	#[rpc(name = "cf_eth_key_manager_address")]
	fn cf_eth_key_manager_address(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<String, jsonrpc_core::Error>;
	#[rpc(name = "cf_eth_stake_manager_address")]
	fn cf_eth_stake_manager_address(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<String, jsonrpc_core::Error>;
	#[rpc(name = "cf_eth_flip_token_address")]
	fn cf_eth_flip_token_address(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<String, jsonrpc_core::Error>;
	#[rpc(name = "cf_eth_chain_id")]
	fn cf_eth_chain_id(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<u64, jsonrpc_core::Error>;
	/// Returns the eth vault in the form [agg_key, active_from_eth_block]
	#[rpc(name = "cf_eth_vault")]
	fn cf_eth_vault(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<(String, u32), jsonrpc_core::Error>;
	#[rpc(name = "cf_tx_fee_multiplier")]
	fn cf_tx_fee_multiplier(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<u64, jsonrpc_core::Error>;
	// Returns the Auction params in the form [min_set_size, max_set_size]
	#[rpc(name = "cf_auction_parameters")]
	fn cf_auction_parameters(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<(u32, u32), jsonrpc_core::Error>;
	#[rpc(name = "cf_min_stake")]
	fn cf_min_stake(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<NumberOrHex, jsonrpc_core::Error>;
	#[rpc(name = "cf_current_epoch")]
	fn cf_current_epoch(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<u32, jsonrpc_core::Error>;
	#[rpc(name = "cf_epoch_duration")]
	fn cf_epoch_duration(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<u32, jsonrpc_core::Error>;
	#[rpc(name = "cf_current_epoch_started_at")]
	fn cf_current_epoch_started_at(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<u32, jsonrpc_core::Error>;
	#[rpc(name = "cf_authority_emission_per_block")]
	fn cf_authority_emission_per_block(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<NumberOrHex, jsonrpc_core::Error>;
	#[rpc(name = "cf_backup_emission_per_block")]
	fn cf_backup_emission_per_block(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<NumberOrHex, jsonrpc_core::Error>;
	#[rpc(name = "cf_flip_supply")]
	fn cf_flip_supply(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<(NumberOrHex, NumberOrHex), jsonrpc_core::Error>;
	#[rpc(name = "cf_accounts")]
	fn cf_accounts(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<Vec<(AccountId32, String)>, jsonrpc_core::Error>;
	#[rpc(name = "cf_account_info")]
	fn cf_account_info(
		&self,
		account_id: AccountId32,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<RpcAccountInfo, jsonrpc_core::Error>;
	#[rpc(name = "cf_pending_claim")]
	fn cf_pending_claim(
		&self,
		account_id: AccountId32,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<Option<RpcPendingClaim>, jsonrpc_core::Error>;
	#[rpc(name = "cf_penalties")]
	fn cf_penalties(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<Vec<(Offence, RpcPenalty)>, jsonrpc_core::Error>;
	#[rpc(name = "cf_suspensions")]
	fn cf_suspensions(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> Result<RpcSuspensions, jsonrpc_core::Error>;
}

/// An RPC extension for the state chain node.
pub struct CustomRpc<C, B> {
	pub client: Arc<C>,
	pub _phantom: PhantomData<B>,
}

impl<C, B> CustomRpc<C, B>
where
	B: sp_runtime::traits::Block<Hash = state_chain_runtime::Hash>,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync + 'static + HeaderBackend<B>,
	C::Api: CustomRuntimeApi<B>,
{
	fn query_block_id(&self, from_rpc: Option<<B as BlockT>::Hash>) -> sp_api::BlockId<B> {
		sp_api::BlockId::hash(from_rpc.unwrap_or_else(|| self.client.info().best_hash))
	}
}

impl<C, B> CustomApi for CustomRpc<C, B>
where
	B: sp_runtime::traits::Block<Hash = state_chain_runtime::Hash>,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync + 'static + HeaderBackend<B>,
	C::Api: CustomRuntimeApi<B>,
{
	fn cf_is_auction_phase(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<bool, jsonrpc_core::Error> {
		self.client
			.runtime_api()
			.cf_is_auction_phase(&self.query_block_id(at))
			.map_to_json_error()
	}
	fn cf_eth_flip_token_address(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<String, jsonrpc_core::Error> {
		let eth_flip_token_address = self
			.client
			.runtime_api()
			.cf_eth_flip_token_address(&self.query_block_id(at))
			.map_to_json_error()?;
		Ok(hex::encode(eth_flip_token_address))
	}
	fn cf_eth_stake_manager_address(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<String, jsonrpc_core::Error> {
		let eth_stake_manager_address = self
			.client
			.runtime_api()
			.cf_eth_stake_manager_address(&self.query_block_id(at))
			.map_to_json_error()?;
		Ok(hex::encode(eth_stake_manager_address))
	}
	fn cf_eth_key_manager_address(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<String, jsonrpc_core::Error> {
		let eth_key_manager_address = self
			.client
			.runtime_api()
			.cf_eth_key_manager_address(&self.query_block_id(at))
			.map_to_json_error()?;
		Ok(hex::encode(eth_key_manager_address))
	}
	fn cf_eth_chain_id(&self, at: Option<<B as BlockT>::Hash>) -> Result<u64, jsonrpc_core::Error> {
		self.client
			.runtime_api()
			.cf_eth_chain_id(&self.query_block_id(at))
			.map_to_json_error()
	}
	fn cf_eth_vault(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<(String, u32), jsonrpc_core::Error> {
		let eth_vault = self
			.client
			.runtime_api()
			.cf_eth_vault(&self.query_block_id(at))
			.expect("The runtime API should not return error.");

		Ok((hex::encode(eth_vault.0), eth_vault.1))
	}
	// FIXME: Respect the block hash argument here
	fn cf_tx_fee_multiplier(
		&self,
		_at: Option<<B as BlockT>::Hash>,
	) -> Result<u64, jsonrpc_core::Error> {
		Ok(TX_FEE_MULTIPLIER
			.try_into()
			.expect("We never set a fee multiplier greater than u64::MAX"))
	}
	fn cf_auction_parameters(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<(u32, u32), jsonrpc_core::Error> {
		self.client
			.runtime_api()
			.cf_auction_parameters(&self.query_block_id(at))
			.map_to_json_error()
	}
	fn cf_min_stake(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<NumberOrHex, jsonrpc_core::Error> {
		let min_stake = self
			.client
			.runtime_api()
			.cf_min_stake(&self.query_block_id(at))
			.map_to_json_error()?;
		Ok(min_stake.into())
	}
	fn cf_current_epoch(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<u32, jsonrpc_core::Error> {
		self.client
			.runtime_api()
			.cf_current_epoch(&self.query_block_id(at))
			.map_to_json_error()
	}
	fn cf_epoch_duration(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<u32, jsonrpc_core::Error> {
		self.client
			.runtime_api()
			.cf_epoch_duration(&self.query_block_id(at))
			.map_err(|_| jsonrpc_core::Error::new(jsonrpc_core::ErrorCode::ServerError(0)))
	}
	fn cf_current_epoch_started_at(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<u32, jsonrpc_core::Error> {
		self.client
			.runtime_api()
			.cf_current_epoch_started_at(&self.query_block_id(at))
			.map_to_json_error()
	}
	fn cf_authority_emission_per_block(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<NumberOrHex, jsonrpc_core::Error> {
		let authority_emission_per_block = self
			.client
			.runtime_api()
			.cf_authority_emission_per_block(&self.query_block_id(at))
			.map_to_json_error()?;
		Ok(authority_emission_per_block.into())
	}
	fn cf_backup_emission_per_block(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<NumberOrHex, jsonrpc_core::Error> {
		let backup_emission_per_block = self
			.client
			.runtime_api()
			.cf_backup_emission_per_block(&self.query_block_id(at))
			.map_to_json_error()?;
		Ok(backup_emission_per_block.into())
	}
	fn cf_flip_supply(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<(NumberOrHex, NumberOrHex), jsonrpc_core::Error> {
		let (issuance, offchain) = self
			.client
			.runtime_api()
			.cf_flip_supply(&self.query_block_id(at))
			.map_to_json_error()?;
		Ok((issuance.into(), offchain.into()))
	}
	fn cf_accounts(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<Vec<(AccountId32, String)>, jsonrpc_core::Error> {
		Ok(self
			.client
			.runtime_api()
			.cf_accounts(&self.query_block_id(at))
			.map_to_json_error()?
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
		at: Option<<B as BlockT>::Hash>,
	) -> Result<RpcAccountInfo, jsonrpc_core::Error> {
		let account_info = self
			.client
			.runtime_api()
			.cf_account_info(&self.query_block_id(at), account_id)
			.map_to_json_error()?;

		Ok(RpcAccountInfo {
			stake: account_info.stake.into(),
			bond: account_info.bond.into(),
			last_heartbeat: account_info.last_heartbeat,
			is_live: account_info.is_live,
			is_activated: account_info.is_activated,
			online_credits: account_info.online_credits,
			reputation_points: account_info.reputation_points,
			withdrawal_address: hex::encode(account_info.withdrawal_address),
			state: account_info.state,
		})
	}
	fn cf_pending_claim(
		&self,
		account_id: AccountId32,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<Option<RpcPendingClaim>, jsonrpc_core::Error> {
		let pending_claim = match self
			.client
			.runtime_api()
			.cf_pending_claim(&self.query_block_id(at), account_id)
			.map_to_json_error()?
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
	fn cf_penalties(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<Vec<(Offence, RpcPenalty)>, jsonrpc_core::Error> {
		Ok(self
			.client
			.runtime_api()
			.cf_penalties(&self.query_block_id(at))
			.map_to_json_error()?
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
	fn cf_suspensions(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> Result<RpcSuspensions, jsonrpc_core::Error> {
		self.client
			.runtime_api()
			.cf_suspensions(&self.query_block_id(at))
			.map_to_json_error()
	}
}
