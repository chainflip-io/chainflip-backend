use core::fmt::Display;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::DispatchResult;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::FixedU128;
pub type ExchangeRate = FixedU128;

/// Primitive enum denoting different types of currencies.
#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Encode,
	Decode,
	Serialize,
	Deserialize,
	TypeInfo,
	MaxEncodedLen,
)]
#[serde(rename_all = "UPPERCASE")]
pub enum Asset {
	Eth,
	Dot,
	Usdc,
	Flip,
}

impl core::fmt::Display for Asset {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Asset::Eth => write!(f, "ETH"),
			Asset::Dot => write!(f, "DOT"),
			Asset::Usdc => write!(f, "USDC"),
			Asset::Flip => write!(f, "FLIP"),
		}
	}
}

pub type PoolId = (Asset, Asset);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub enum ForeignChain {
	Ethereum,
	Polkadot,
	Bitcoin,
}

impl Display for ForeignChain {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ForeignChain::Ethereum => write!(f, "Ethereum"),
			ForeignChain::Polkadot => write!(f, "Polkadot"),
			ForeignChain::Bitcoin => write!(f, "Bitcoin"),
		}
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub struct ForeignAsset {
	pub chain: ForeignChain,
	pub asset: Asset,
}

impl Display for ForeignAsset {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "chain: {}, Asset: {}", self.chain, self.asset)
	}
}

pub trait LpAccountHandler {
	type AccountId;
	type Amount;

	// Register a new LP account.
	fn register_lp_account(_account_id: &Self::AccountId) -> DispatchResult;

	// Try to debit given asset from the account. WIll fail if the account has insufficient balance.
	fn try_debit(who: &Self::AccountId, asset: Asset, amount: Self::Amount) -> DispatchResult;

	// Credit given asset to the account.
	fn credit(who: &Self::AccountId, asset: Asset, amount: Self::Amount) -> DispatchResult;
}

impl LpAccountHandler for () {
	type AccountId = ();
	type Amount = ();

	fn register_lp_account(_account_id: &Self::AccountId) -> DispatchResult {
		Ok(())
	}

	fn try_debit(_who: &Self::AccountId, _asset: Asset, _amount: Self::Amount) -> DispatchResult {
		Ok(())
	}

	fn credit(_who: &Self::AccountId, _asset: Asset, _amount: Self::Amount) -> DispatchResult {
        Ok(())
    }
}

/// Account Types.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub enum AccountType {
	Validator,
	Relayer,
	LiquidityProvider,
	None,
}

/// Trait that represents a registry of account types.
pub trait AccountTypeRegistry {
	type AccountId;
	fn register_account(who: &Self::AccountId, account_type: AccountType) -> DispatchResult;
	fn deregister_account(who: &Self::AccountId) -> DispatchResult;
	fn account_type(who: &Self::AccountId) -> Option<AccountType>;
}

impl AccountTypeRegistry for () {
	type AccountId = ();
	fn register_account(_who: &Self::AccountId, _account_type: AccountType) -> DispatchResult {
		Ok(())
	}

	fn deregister_account(_who: &Self::AccountId) -> DispatchResult {
		Ok(())
	}

	fn account_type(_who: &Self::AccountId) -> Option<AccountType> {
		None
	}
}
pub trait LpProvisioningApi {
	type AccountId;
	type Amount;

	/// Called from the vault when ingress is witnessed.
	fn provision_account(who: &Self::AccountId, asset: Asset, amount: Self::Amount);
}

impl LpProvisioningApi for () {
	type AccountId = ();
	type Amount = ();

	fn provision_account(_who: &Self::AccountId, _asset: Asset, _amount: Self::Amount) {}
}

pub trait LpWithdrawalApi {
	type AccountId;
	type Amount;
	type EgressAddress;

	fn withdraw_liquidity(
		who: &Self::AccountId,
		amount: Self::Amount,
		foreign_asset: &ForeignAsset,
		egress_address: &Self::EgressAddress,
	) -> DispatchResult;
}

impl LpWithdrawalApi for () {
	type AccountId = ();
	type Amount = ();
	type EgressAddress = ();

	fn withdraw_liquidity(
		_who: &Self::AccountId,
		_amount: Self::Amount,
		_foreign_asset: &ForeignAsset,
		_egress_address: &Self::EgressAddress,
	) -> DispatchResult {
		Ok(())
	}
}

/// Trait used for to manage user's LP positions.
pub trait LpPositionManagement {
	type AccountId;
	type Balance;
	fn open_position(
		who: &Self::AccountId,
		pool_id: PoolId,
		position: TradingPosition<Self::Balance>,
	) -> DispatchResult;
	fn update_position(
		who: &Self::AccountId,
		pool_id: PoolId,
		id: PositionId,
		new_position: TradingPosition<Self::Balance>,
	) -> DispatchResult;
	fn close_position(who: &Self::AccountId, id: PositionId) -> DispatchResult;
}
impl LpPositionManagement for () {
	type AccountId = ();
	type Balance = ();
	fn open_position(
		_who: &Self::AccountId,
		_pool_id: PoolId,
		_position: TradingPosition<Self::Balance>,
	) -> DispatchResult {
		Ok(())
	}
	fn update_position(
		_who: &Self::AccountId,
		_pool_id: PoolId,
		_id: PositionId,
		_new_position: TradingPosition<Self::Balance>,
	) -> DispatchResult {
		Ok(())
	}
	fn close_position(_who: &Self::AccountId, _id: PositionId) -> DispatchResult {
		Ok(())
	}
}

pub trait PalletLpApi: LpProvisioningApi + LpWithdrawalApi + LpPositionManagement {}
impl PalletLpApi for () {}

pub trait EgressHandler {
	type Amount;
	type EgressAddress;
	fn add_to_egress_batch(
		asset: &ForeignAsset,
		amount: Self::Amount,
		egress_address: &Self::EgressAddress,
	) -> DispatchResult;
}

impl EgressHandler for () {
	type Amount = ();
	type EgressAddress = ();
	fn add_to_egress_batch(
		_asset: &ForeignAsset,
		_amount: Self::Amount,
		_egress_address: &Self::EgressAddress,
	) -> DispatchResult {
		Ok(())
	}
}

/// The id type for range orders.
pub type PositionId = u64;

/// The type used for measuring price ticks. Note Uniswap uses i24 but this is not supported in
/// rust.
pub type Tick = i32;

#[derive(
	Copy,
	Clone,
	Debug,
	Default,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	TypeInfo,
)]
pub struct AmmRange {
	pub lower: Tick,
	pub upper: Tick,
}

/// Denotes the two assets contained in a pool.
#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	TypeInfo,
)]
pub enum PoolAsset {
	Asset0,
	Asset1,
}

/// Represents the types of order that an LP can submit to the AMM.
#[derive(
	Clone, Debug, TypeInfo, PartialEq, Eq, Encode, Decode, MaxEncodedLen, Serialize, Deserialize,
)]
pub enum TradingPosition<Volume> {
	/// Standard Uniswap V3 style 'sticky' range order position. When executed, the converted
	/// amount remains in the pool.
	///
	/// The volumes must be consistent with the specified range.
	ClassicV3 { range: AmmRange, volume_0: Volume, volume_1: Volume },
	/// A 'volatile' single-sided position. When executed, the converted amount is returned to the
	/// LP's free balance.
	VolatileV3 { range: AmmRange, side: PoolAsset, volume: Volume },
}

impl<Volume: Default + Copy> TradingPosition<Volume> {
	pub fn volume_0(&self) -> Volume {
		match self {
			TradingPosition::ClassicV3 { volume_0, .. } => *volume_0,
			TradingPosition::VolatileV3 { side, volume, .. } if *side == PoolAsset::Asset0 =>
				*volume,
			_ => Default::default(),
		}
	}
	pub fn volume_1(&self) -> Volume {
		match self {
			TradingPosition::ClassicV3 { volume_1, .. } => *volume_1,
			TradingPosition::VolatileV3 { side, volume, .. } if *side == PoolAsset::Asset1 =>
				*volume,
			_ => Default::default(),
		}
	}
}

/// Base Amm pool api common to both LPs and swaps.
pub trait AmmPoolApi {
	type Balance;
	fn asset_0(&self) -> Asset;
	fn asset_1(&self) -> Asset;
	fn liquidity_0(&self) -> Self::Balance;
	fn liquidity_1(&self) -> Self::Balance;

	fn pool_id(&self) -> PoolId {
		(self.asset_0(), self.asset_1())
	}

	fn get_exchange_rate(&self) -> ExchangeRate;

	fn get_liquidity_requirement(
		&self,
		position: &TradingPosition<Self::Balance>,
	) -> Option<(Self::Balance, Self::Balance)>;
}
