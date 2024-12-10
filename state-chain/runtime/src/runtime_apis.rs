use crate::{chainflip::Offence, Runtime, RuntimeSafeMode};
use cf_amm::{
	common::{PoolPairsMap, Side},
	math::{Amount, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	self, address::EncodedAddress, assets::any::AssetMap, eth::Address as EthereumAddress, Chain,
	ChainCrypto, ForeignChainAddress,
};
use cf_primitives::{
	AccountRole, AffiliateShortId, Affiliates, Asset, AssetAmount, BasisPoints, BlockNumber,
	BroadcastId, DcaParameters, EpochIndex, FlipBalance, ForeignChain, NetworkEnvironment,
	PrewitnessedDepositId, SemVer,
};
use cf_traits::SwapLimits;
use codec::{Decode, Encode};
use core::{ops::Range, str};
use frame_support::sp_runtime::AccountId32;
use pallet_cf_governance::GovCallHash;
pub use pallet_cf_ingress_egress::OwedAmount;
use pallet_cf_pools::{
	AskBidMap, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders, PoolPriceV1, PoolPriceV2,
	UnidirectionalPoolDepth,
};
use pallet_cf_swapping::SwapLegInfo;
use pallet_cf_witnesser::CallHash;
use scale_info::{prelude::string::String, TypeInfo};
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;
use sp_runtime::DispatchError;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec::Vec,
};

type VanityName = Vec<u8>;

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize)]
#[serde(tag = "chain")]
pub enum VaultSwapDetails<BtcAddress> {
	Bitcoin {
		#[serde(with = "sp_core::bytes")]
		nulldata_payload: Vec<u8>,
		deposit_address: BtcAddress,
	},
}

impl<BtcAddress> VaultSwapDetails<BtcAddress> {
	pub fn map_btc_address<F, T>(self, f: F) -> VaultSwapDetails<T>
	where
		F: FnOnce(BtcAddress) -> T,
	{
		match self {
			VaultSwapDetails::Bitcoin { nulldata_payload, deposit_address } =>
				VaultSwapDetails::Bitcoin { nulldata_payload, deposit_address: f(deposit_address) },
		}
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, Copy, TypeInfo, Serialize, Deserialize)]
pub enum BackupOrPassive {
	Backup,
	Passive,
}

// TEMP: so frontend doesn't break after removal of passive from backend
#[derive(PartialEq, Eq, Clone, Encode, Decode, Copy, TypeInfo, Serialize, Deserialize)]
pub enum ChainflipAccountStateWithPassive {
	CurrentAuthority,
	BackupOrPassive(BackupOrPassive),
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Serialize, Deserialize)]
pub struct ValidatorInfo {
	pub balance: u128,
	pub bond: u128,
	pub last_heartbeat: u32, // can *maybe* remove this - check with Andrew
	pub reputation_points: i32,
	pub keyholder_epochs: Vec<EpochIndex>,
	pub is_current_authority: bool,
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
	pub bound_redeem_address: Option<EthereumAddress>,
	pub apy_bp: Option<u32>, // APY for validator/back only. In Basis points.
	pub restricted_balances: BTreeMap<EthereumAddress, u128>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Clone)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BoostPoolDepth {
	#[cfg_attr(feature = "std", serde(flatten))]
	pub asset: Asset,
	pub tier: u16,
	#[cfg_attr(feature = "std", serde(serialize_with = "serialize_as_hex"))]
	pub available_amount: AssetAmount,
}

#[derive(Encode, Decode, TypeInfo)]
pub enum SimulateSwapAdditionalOrder {
	LimitOrder {
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		tick: Tick,
		sell_amount: AssetAmount,
	},
}

#[cfg(feature = "std")]
fn serialize_as_hex<S>(amount: &AssetAmount, s: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	sp_core::U256::from(*amount).serialize(s)
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct BoostPoolDetails {
	pub available_amounts: BTreeMap<AccountId32, AssetAmount>,
	pub pending_boosts:
		BTreeMap<PrewitnessedDepositId, BTreeMap<AccountId32, OwedAmount<AssetAmount>>>,
	pub pending_withdrawals: BTreeMap<AccountId32, BTreeSet<PrewitnessedDepositId>>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct RuntimeApiPenalty {
	pub reputation_points: i32,
	pub suspension_duration_blocks: u32,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct AuctionState {
	pub blocks_per_epoch: u32,
	pub current_epoch_started_at: u32,
	pub redemption_period_as_percentage: u8,
	pub min_funding: u128,
	pub auction_size_range: (u32, u32),
	pub min_active_bid: Option<u128>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct LiquidityProviderBoostPoolInfo {
	pub fee_tier: u16,
	pub total_balance: AssetAmount,
	pub available_balance: AssetAmount,
	pub in_use_balance: AssetAmount,
	pub is_withdrawing: bool,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct LiquidityProviderInfo {
	pub refund_addresses: Vec<(ForeignChain, Option<ForeignChainAddress>)>,
	pub balances: Vec<(Asset, AssetAmount)>,
	pub earned_fees: AssetMap<AssetAmount>,
	pub boost_balances: AssetMap<Vec<LiquidityProviderBoostPoolInfo>>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct BrokerInfo {
	pub earned_fees: Vec<(Asset, AssetAmount)>,
	pub channel_address: Option<String>,
}

/// Struct that represents the estimated output of a Swap.
#[obake::versioned]
#[obake(version("1.0.0"))]
#[obake(version("2.0.0"))]
#[derive(Encode, Decode, TypeInfo)]
pub struct SimulatedSwapInformation {
	pub intermediary: Option<AssetAmount>,
	pub output: AssetAmount,
	pub network_fee: AssetAmount,
	pub ingress_fee: AssetAmount,
	pub egress_fee: AssetAmount,
	#[obake(cfg(">=2.0"))]
	pub broker_fee: AssetAmount,
}

impl From<SimulatedSwapInformation!["1.0.0"]> for SimulatedSwapInformation {
	fn from(value: SimulatedSwapInformation!["1.0.0"]) -> Self {
		Self {
			intermediary: value.intermediary,
			output: value.output,
			network_fee: value.network_fee,
			ingress_fee: value.ingress_fee,
			egress_fee: value.egress_fee,
			broker_fee: Default::default(),
		}
	}
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum DispatchErrorWithMessage {
	Module(Vec<u8>),
	Other(DispatchError),
}
impl<E: Into<DispatchError>> From<E> for DispatchErrorWithMessage {
	fn from(error: E) -> Self {
		match error.into() {
			DispatchError::Module(sp_runtime::ModuleError { message: Some(message), .. }) =>
				DispatchErrorWithMessage::Module(message.as_bytes().to_vec()),
			error => DispatchErrorWithMessage::Other(error),
		}
	}
}

#[cfg(feature = "std")]
impl core::fmt::Display for DispatchErrorWithMessage {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		match self {
			DispatchErrorWithMessage::Module(message) => write!(
				f,
				"{}",
				str::from_utf8(message).unwrap_or("<Error message is not valid UTF-8>")
			),
			DispatchErrorWithMessage::Other(error) => write!(f, "{:?}", error),
		}
	}
}
#[cfg(feature = "std")]
impl std::error::Error for DispatchErrorWithMessage {}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct FailingWitnessValidators {
	pub failing_count: u32,
	pub validators: Vec<(cf_primitives::AccountId, String, bool)>,
}

type ChainAccountFor<C> = <C as Chain>::ChainAccount;

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct ChainAccounts {
	pub btc_chain_accounts: Vec<ChainAccountFor<cf_chains::Bitcoin>>,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub enum TransactionScreeningEvent<TxId> {
	TransactionRejectionRequestReceived {
		account_id: <Runtime as frame_system::Config>::AccountId,
		tx_id: TxId,
	},

	TransactionRejectionRequestExpired {
		account_id: <Runtime as frame_system::Config>::AccountId,
		tx_id: TxId,
	},

	TransactionRejectedByBroker {
		refund_broadcast_id: BroadcastId,
		tx_id: TxId,
	},
}

type BrokerRejectionEventFor<C> =
	TransactionScreeningEvent<<<C as Chain>::ChainCrypto as ChainCrypto>::TransactionInId>;

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct TransactionScreeningEvents {
	pub btc_events: Vec<BrokerRejectionEventFor<cf_chains::Bitcoin>>,
}

// READ THIS BEFORE UPDATING THIS TRAIT:
//
// ## When changing an existing method:
//  - Bump the api_version of the trait, for example from #[api_version(2)] to #[api_version(3)].
//  - Annotate the old method with #[changed_in($VERSION)] where $VERSION is the *new* api_version,
//    for example #[changed_in(3)].
//  - Handle the old method in the custom rpc implementation using runtime_api().api_version().
//
// ## When adding a new method:
//  - Bump the api_version of the trait, for example from #[api_version(2)] to #[api_version(3)].
//  - Create a dummy method with the same name, but no args and no return value.
//  - Annotate the dummy method with #[changed_in($VERSION)] where $VERSION is the *new*
//    api_version.
//  - Handle the dummy method gracefully in the custom rpc implementation using
//    runtime_api().api_version().
decl_runtime_apis!(
	#[api_version(2)]
	pub trait CustomRuntimeApi {
		/// Returns true if the current phase is the auction phase.
		fn cf_is_auction_phase() -> bool;
		fn cf_eth_flip_token_address() -> EthereumAddress;
		fn cf_eth_state_chain_gateway_address() -> EthereumAddress;
		fn cf_eth_key_manager_address() -> EthereumAddress;
		fn cf_eth_chain_id() -> u64;
		/// Returns the eth vault in the form [agg_key, active_from_eth_block]
		fn cf_eth_vault() -> ([u8; 33], u32);
		/// Returns the Auction params in the form [min_set_size, max_set_size]
		fn cf_auction_parameters() -> (u32, u32);
		fn cf_min_funding() -> u128;
		fn cf_current_epoch() -> u32;
		#[deprecated(note = "Use direct storage access of `CurrentReleaseVersion` instead.")]
		fn cf_current_compatibility_version() -> SemVer;
		fn cf_epoch_duration() -> u32;
		fn cf_current_epoch_started_at() -> u32;
		fn cf_authority_emission_per_block() -> u128;
		fn cf_backup_emission_per_block() -> u128;
		/// Returns the flip supply in the form [total_issuance, offchain_funds]
		fn cf_flip_supply() -> (u128, u128);
		fn cf_accounts() -> Vec<(AccountId32, VanityName)>;
		fn cf_account_flip_balance(account_id: &AccountId32) -> u128;
		fn cf_validator_info(account_id: &AccountId32) -> ValidatorInfo;
		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)>;
		fn cf_suspensions() -> Vec<(Offence, Vec<(u32, AccountId32)>)>;
		fn cf_generate_gov_key_call_hash(call: Vec<u8>) -> GovCallHash;
		fn cf_auction_state() -> AuctionState;
		fn cf_pool_price(from: Asset, to: Asset) -> Option<PoolPriceV1>;
		fn cf_pool_price_v2(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolPriceV2, DispatchErrorWithMessage>;
		#[changed_in(2)]
		fn cf_pool_simulate_swap(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			additional_limit_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
		) -> Result<SimulatedSwapInformation!["1.0.0"], DispatchErrorWithMessage>;
		fn cf_pool_simulate_swap(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			broker_commission: BasisPoints,
			dca_parameters: Option<DcaParameters>,
			additional_limit_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
		) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage>;
		fn cf_pool_info(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolInfo, DispatchErrorWithMessage>;
		fn cf_pool_depth(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::math::Tick>,
		) -> Result<AskBidMap<UnidirectionalPoolDepth>, DispatchErrorWithMessage>;
		fn cf_pool_liquidity(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolLiquidity, DispatchErrorWithMessage>;
		fn cf_required_asset_ratio_for_range_order(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::math::Tick>,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage>;
		fn cf_pool_orderbook(
			base_asset: Asset,
			quote_asset: Asset,
			orders: u32,
		) -> Result<PoolOrderbook, DispatchErrorWithMessage>;
		fn cf_pool_orders(
			base_asset: Asset,
			quote_asset: Asset,
			lp: Option<AccountId32>,
			filled_orders: bool,
		) -> Result<PoolOrders<Runtime>, DispatchErrorWithMessage>;
		fn cf_pool_range_order_liquidity_value(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<Tick>,
			liquidity: Liquidity,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage>;

		fn cf_max_swap_amount(asset: Asset) -> Option<AssetAmount>;
		fn cf_min_deposit_amount(asset: Asset) -> AssetAmount;
		fn cf_egress_dust_limit(asset: Asset) -> AssetAmount;
		fn cf_prewitness_swaps(
			base_asset: Asset,
			quote_asset: Asset,
			side: Side,
		) -> Vec<AssetAmount>;
		fn cf_scheduled_swaps(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Vec<(SwapLegInfo, BlockNumber)>;
		fn cf_liquidity_provider_info(account_id: AccountId32) -> LiquidityProviderInfo;
		fn cf_broker_info(account_id: AccountId32) -> BrokerInfo;
		fn cf_account_role(account_id: AccountId32) -> Option<AccountRole>;
		fn cf_free_balances(account_id: AccountId32) -> AssetMap<AssetAmount>;
		fn cf_lp_total_balances(account_id: AccountId32) -> AssetMap<AssetAmount>;
		fn cf_redemption_tax() -> AssetAmount;
		fn cf_network_environment() -> NetworkEnvironment;
		fn cf_failed_call_ethereum(
			broadcast_id: BroadcastId,
		) -> Option<<cf_chains::Ethereum as Chain>::Transaction>;
		fn cf_failed_call_arbitrum(
			broadcast_id: BroadcastId,
		) -> Option<<cf_chains::Arbitrum as Chain>::Transaction>;
		fn cf_ingress_fee(asset: Asset) -> Option<AssetAmount>;
		fn cf_egress_fee(asset: Asset) -> Option<AssetAmount>;
		fn cf_witness_count(
			hash: CallHash,
			epoch_index: Option<EpochIndex>,
		) -> Option<FailingWitnessValidators>;
		fn cf_witness_safety_margin(chain: ForeignChain) -> Option<u64>;
		fn cf_channel_opening_fee(chain: ForeignChain) -> FlipBalance;
		fn cf_boost_pools_depth() -> Vec<BoostPoolDepth>;
		fn cf_boost_pool_details(asset: Asset) -> BTreeMap<u16, BoostPoolDetails>;
		fn cf_safe_mode_statuses() -> RuntimeSafeMode;
		fn cf_pools() -> Vec<PoolPairsMap<Asset>>;
		fn cf_swap_retry_delay_blocks() -> u32;
		fn cf_swap_limits() -> SwapLimits;
		fn cf_lp_events() -> Vec<pallet_cf_pools::Event<Runtime>>;
		fn cf_minimum_chunk_size(asset: Asset) -> AssetAmount;
		fn cf_validate_dca_params(
			number_of_chunks: u32,
			chunk_interval: u32,
		) -> Result<(), DispatchErrorWithMessage>;
		fn cf_validate_refund_params(
			retry_duration: BlockNumber,
		) -> Result<(), DispatchErrorWithMessage>;
		fn cf_get_vault_swap_details(
			broker: AccountId32,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			min_output_amount: AssetAmount,
			retry_duration: BlockNumber,
			boost_fee: BasisPoints,
			affiliate_fees: Affiliates<AccountId32>,
			dca_parameters: Option<DcaParameters>,
		) -> Result<VaultSwapDetails<String>, DispatchErrorWithMessage>;
		fn cf_get_open_deposit_channels(account_id: Option<AccountId32>) -> ChainAccounts;
		fn cf_transaction_screening_events() -> TransactionScreeningEvents;
		fn cf_get_affiliates(broker: AccountId32) -> Vec<(AffiliateShortId, AccountId32)>;
	}
);

decl_runtime_apis!(
	pub trait ElectoralRuntimeApi<Instance: 'static> {
		/// Returns SCALE encoded `Option<ElectoralDataFor<state_chain_runtime::Runtime,
		/// Instance>>`
		fn cf_electoral_data(account_id: AccountId32) -> Vec<u8>;

		/// Returns SCALE encoded `BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as
		/// pallet_cf_elections::Config<Instance>>::ElectoralSystem>>`
		fn cf_filter_votes(account_id: AccountId32, proposed_votes: Vec<u8>) -> Vec<u8>;
	}
);
