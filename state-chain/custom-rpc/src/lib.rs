use cf_chains::eth::SigData;
use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::error::CallError};
use pallet_cf_governance::GovCallHash;
use sc_client_api::HeaderBackend;
use serde::{Deserialize, Serialize};
use sp_api::BlockT;
use sp_rpc::number::NumberOrHex;
use sp_runtime::AccountId32;
use state_chain_runtime::{
	chainflip::Offence,
	constants::common::TX_FEE_MULTIPLIER,
	runtime_apis::{ChainflipAccountStateWithPassive, CustomRuntimeApi},
};
use std::{marker::PhantomData, sync::Arc};

#[allow(unused)]
use state_chain_runtime::{Asset, AssetAmount, ExchangeRate};

use cf_primitives::Tick;

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
pub struct RpcAccountInfoV2 {
	pub stake: NumberOrHex,
	pub bond: NumberOrHex,
	pub last_heartbeat: u32,
	pub online_credits: u32,
	pub reputation_points: i32,
	pub withdrawal_address: String,
	pub keyholder_epochs: Vec<u32>,
	pub is_current_authority: bool,
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
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

#[derive(Serialize, Deserialize)]
pub struct RpcAuctionState {
	blocks_per_epoch: u32,
	current_epoch_started_at: u32,
	claim_period_as_percentage: u8,
	min_stake: NumberOrHex,
	auction_size_range: (u32, u32),
}

#[rpc(server, client, namespace = "cf")]
/// The custom RPC endpoints for the state chain node.
pub trait CustomApi {
	/// Returns true if the current phase is the auction phase.
	#[method(name = "is_auction_phase")]
	fn cf_is_auction_phase(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<bool>;
	#[method(name = "eth_key_manager_address")]
	fn cf_eth_key_manager_address(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<String>;
	#[method(name = "eth_stake_manager_address")]
	fn cf_eth_stake_manager_address(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<String>;
	#[method(name = "eth_flip_token_address")]
	fn cf_eth_flip_token_address(&self, at: Option<state_chain_runtime::Hash>)
		-> RpcResult<String>;
	#[method(name = "eth_chain_id")]
	fn cf_eth_chain_id(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u64>;
	/// Returns the eth vault in the form [agg_key, active_from_eth_block]
	#[method(name = "eth_vault")]
	fn cf_eth_vault(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<(String, u32)>;
	#[method(name = "tx_fee_multiplier")]
	fn cf_tx_fee_multiplier(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u64>;
	// Returns the Auction params in the form [min_set_size, max_set_size]
	#[method(name = "auction_parameters")]
	fn cf_auction_parameters(&self, at: Option<state_chain_runtime::Hash>)
		-> RpcResult<(u32, u32)>;
	#[method(name = "min_stake")]
	fn cf_min_stake(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<NumberOrHex>;
	#[method(name = "current_epoch")]
	fn cf_current_epoch(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u32>;
	#[method(name = "epoch_duration")]
	fn cf_epoch_duration(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u32>;
	#[method(name = "current_epoch_started_at")]
	fn cf_current_epoch_started_at(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u32>;
	#[method(name = "authority_emission_per_block")]
	fn cf_authority_emission_per_block(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<NumberOrHex>;
	#[method(name = "backup_emission_per_block")]
	fn cf_backup_emission_per_block(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<NumberOrHex>;
	#[method(name = "flip_supply")]
	fn cf_flip_supply(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<(NumberOrHex, NumberOrHex)>;
	#[method(name = "accounts")]
	fn cf_accounts(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<(AccountId32, String)>>;
	#[method(name = "account_info")]
	fn cf_account_info(
		&self,
		account_id: AccountId32,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcAccountInfo>;
	#[method(name = "account_info_v2")]
	fn cf_account_info_v2(
		&self,
		account_id: AccountId32,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcAccountInfoV2>;
	#[method(name = "penalties")]
	fn cf_penalties(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<(Offence, RpcPenalty)>>;
	#[method(name = "suspensions")]
	fn cf_suspensions(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcSuspensions>;
	#[method(name = "generate_gov_key_call_hash")]
	fn cf_generate_gov_key_call_hash(
		&self,
		call: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<GovCallHash>;
	#[method(name = "auction_state")]
	fn cf_auction_state(&self, at: Option<state_chain_runtime::Hash>)
		-> RpcResult<RpcAuctionState>;
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

fn to_rpc_error<E: std::error::Error + Send + Sync + 'static>(e: E) -> jsonrpsee::core::Error {
	CallError::from_std_error(e).into()
}

impl<C, B> CustomApiServer for CustomRpc<C, B>
where
	B: sp_runtime::traits::Block<Hash = state_chain_runtime::Hash>,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync + 'static + HeaderBackend<B>,
	C::Api: CustomRuntimeApi<B>,
{
	fn cf_is_auction_phase(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<bool> {
		self.client
			.runtime_api()
			.cf_is_auction_phase(&self.query_block_id(at))
			.map_err(to_rpc_error)
	}
	fn cf_eth_flip_token_address(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<String> {
		let eth_flip_token_address = self
			.client
			.runtime_api()
			.cf_eth_flip_token_address(&self.query_block_id(at))
			.map_err(to_rpc_error)?;
		Ok(hex::encode(eth_flip_token_address))
	}
	fn cf_eth_stake_manager_address(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<String> {
		let eth_stake_manager_address = self
			.client
			.runtime_api()
			.cf_eth_stake_manager_address(&self.query_block_id(at))
			.map_err(to_rpc_error)?;
		Ok(hex::encode(eth_stake_manager_address))
	}
	fn cf_eth_key_manager_address(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<String> {
		let eth_key_manager_address = self
			.client
			.runtime_api()
			.cf_eth_key_manager_address(&self.query_block_id(at))
			.map_err(to_rpc_error)?;
		Ok(hex::encode(eth_key_manager_address))
	}
	fn cf_eth_chain_id(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<u64> {
		self.client
			.runtime_api()
			.cf_eth_chain_id(&self.query_block_id(at))
			.map_err(to_rpc_error)
	}
	fn cf_eth_vault(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<(String, u32)> {
		self.client
			.runtime_api()
			.cf_eth_vault(&self.query_block_id(at))
			.map(|(public_key, active_from_block)| (hex::encode(public_key), active_from_block))
			.map_err(to_rpc_error)
	}
	// FIXME: Respect the block hash argument here
	fn cf_tx_fee_multiplier(&self, _at: Option<<B as BlockT>::Hash>) -> RpcResult<u64> {
		Ok(TX_FEE_MULTIPLIER as u64)
	}
	fn cf_auction_parameters(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<(u32, u32)> {
		self.client
			.runtime_api()
			.cf_auction_parameters(&self.query_block_id(at))
			.map_err(to_rpc_error)
	}
	fn cf_min_stake(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<NumberOrHex> {
		let min_stake = self
			.client
			.runtime_api()
			.cf_min_stake(&self.query_block_id(at))
			.map_err(to_rpc_error)?;
		Ok(min_stake.into())
	}
	fn cf_current_epoch(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<u32> {
		self.client
			.runtime_api()
			.cf_current_epoch(&self.query_block_id(at))
			.map_err(to_rpc_error)
	}
	fn cf_epoch_duration(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<u32> {
		self.client
			.runtime_api()
			.cf_epoch_duration(&self.query_block_id(at))
			.map_err(to_rpc_error)
	}
	fn cf_current_epoch_started_at(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<u32> {
		self.client
			.runtime_api()
			.cf_current_epoch_started_at(&self.query_block_id(at))
			.map_err(to_rpc_error)
	}
	fn cf_authority_emission_per_block(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<NumberOrHex> {
		let authority_emission_per_block = self
			.client
			.runtime_api()
			.cf_authority_emission_per_block(&self.query_block_id(at))
			.map_err(to_rpc_error)?;
		Ok(authority_emission_per_block.into())
	}
	fn cf_backup_emission_per_block(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<NumberOrHex> {
		let backup_emission_per_block = self
			.client
			.runtime_api()
			.cf_backup_emission_per_block(&self.query_block_id(at))
			.map_err(to_rpc_error)?;
		Ok(backup_emission_per_block.into())
	}
	fn cf_flip_supply(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<(NumberOrHex, NumberOrHex)> {
		let (issuance, offchain) = self
			.client
			.runtime_api()
			.cf_flip_supply(&self.query_block_id(at))
			.map_err(to_rpc_error)?;
		Ok((issuance.into(), offchain.into()))
	}
	fn cf_accounts(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<Vec<(AccountId32, String)>> {
		Ok(self
			.client
			.runtime_api()
			.cf_accounts(&self.query_block_id(at))
			.map_err(to_rpc_error)?
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
	) -> RpcResult<RpcAccountInfo> {
		let account_info = self
			.client
			.runtime_api()
			.cf_account_info(&self.query_block_id(at), account_id)
			.map_err(to_rpc_error)?;

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
	fn cf_account_info_v2(
		&self,
		account_id: AccountId32,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<RpcAccountInfoV2> {
		let account_info = self
			.client
			.runtime_api()
			.cf_account_info_v2(&self.query_block_id(at), account_id)
			.map_err(to_rpc_error)?;

		Ok(RpcAccountInfoV2 {
			stake: account_info.stake.into(),
			bond: account_info.bond.into(),
			last_heartbeat: account_info.last_heartbeat,
			online_credits: account_info.online_credits,
			reputation_points: account_info.reputation_points,
			withdrawal_address: hex::encode(account_info.withdrawal_address),
			keyholder_epochs: account_info.keyholder_epochs,
			is_current_authority: account_info.is_current_authority,
			is_current_backup: account_info.is_current_backup,
			is_qualified: account_info.is_qualified,
			is_online: account_info.is_online,
			is_bidding: account_info.is_bidding,
		})
	}
	fn cf_penalties(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<Vec<(Offence, RpcPenalty)>> {
		Ok(self
			.client
			.runtime_api()
			.cf_penalties(&self.query_block_id(at))
			.map_err(to_rpc_error)?
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
	fn cf_suspensions(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<RpcSuspensions> {
		self.client
			.runtime_api()
			.cf_suspensions(&self.query_block_id(at))
			.map_err(to_rpc_error)
	}

	fn cf_generate_gov_key_call_hash(
		&self,
		call: Vec<u8>,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<GovCallHash> {
		self.client
			.runtime_api()
			.cf_generate_gov_key_call_hash(&self.query_block_id(at), call)
			.map_err(to_rpc_error)
	}

	fn cf_auction_state(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<RpcAuctionState> {
		let auction_state = self
			.client
			.runtime_api()
			.cf_auction_state(&self.query_block_id(at))
			.map_err(to_rpc_error)?;

		Ok(RpcAuctionState {
			blocks_per_epoch: auction_state.blocks_per_epoch,
			current_epoch_started_at: auction_state.current_epoch_started_at,
			claim_period_as_percentage: auction_state.claim_period_as_percentage,
			min_stake: auction_state.min_stake.into(),
			auction_size_range: auction_state.auction_size_range,
		})
	}
}

use pallet_cf_pools_runtime_api::PoolsApi;

pub struct PoolsRpc<C, B> {
	pub client: Arc<C>,
	pub _phantom: PhantomData<B>,
}

impl<C, B> PoolsRpc<C, B>
where
	B: sp_runtime::traits::Block<Hash = state_chain_runtime::Hash>,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync + 'static + HeaderBackend<B>,
	C::Api: pallet_cf_pools_runtime_api::PoolsApi<B>,
{
	fn query_block_id(&self, from_rpc: Option<<B as BlockT>::Hash>) -> sp_api::BlockId<B> {
		sp_api::BlockId::hash(from_rpc.unwrap_or_else(|| self.client.info().best_hash))
	}
}

#[rpc(server, client, namespace = "cf")]
pub trait PoolsApi {
	#[method(name = "pool_tick_price")]
	fn cf_pool_tick_price(
		&self,
		asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<Tick>>;
}

impl<C, B> PoolsApiServer for PoolsRpc<C, B>
where
	B: sp_runtime::traits::Block<Hash = state_chain_runtime::Hash>,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync + 'static + HeaderBackend<B>,
	C::Api: pallet_cf_pools_runtime_api::PoolsApi<B>,
{
	fn cf_pool_tick_price(
		&self,
		asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<Tick>> {
		self.client
			.runtime_api()
			.cf_pool_tick_price(&self.query_block_id(at), asset)
			.map_err(to_rpc_error)
	}
}
