use sp_runtime::DispatchResult;

use cf_primitives::{
	liquidity::TradingPosition, AccountId, Asset, ExchangeRate, ForeignChainAddress,
	ForeignChainAsset, PoolId, PositionId,
};

use crate::FlipBalance;
use cf_primitives::AssetAmount;

pub trait SwapIntentHandler {
	type AccountId;
	fn schedule_swap(
		from: Asset,
		to: ForeignChainAsset,
		amount: AssetAmount,
		egress_address: ForeignChainAddress,
		relayer_id: Self::AccountId,
		relayer_commission_bps: u16,
	);
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
	type AccountId = AccountId;
	type Amount = FlipBalance;

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

pub trait LpProvisioningApi {
	type AccountId;
	type Amount;

	/// Called from the vault when ingress is witnessed.
	fn provision_account(
		who: &Self::AccountId,
		asset: Asset,
		amount: Self::Amount,
	) -> DispatchResult;
}

impl LpProvisioningApi for () {
	type AccountId = AccountId;
	type Amount = FlipBalance;

	fn provision_account(
		_who: &Self::AccountId,
		_asset: Asset,
		_amount: Self::Amount,
	) -> DispatchResult {
		Ok(())
	}
}

pub trait LpWithdrawalApi {
	type AccountId;
	type Amount;
	type EgressAddress;

	fn withdraw_liquidity(
		who: &Self::AccountId,
		amount: Self::Amount,
		foreign_asset: &ForeignChainAsset,
		egress_address: &Self::EgressAddress,
	) -> DispatchResult;
}

impl LpWithdrawalApi for () {
	type AccountId = AccountId;
	type Amount = FlipBalance;
	type EgressAddress = ForeignChainAddress;

	fn withdraw_liquidity(
		_who: &Self::AccountId,
		_amount: Self::Amount,
		_foreign_asset: &ForeignChainAsset,
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
	type AccountId = AccountId;
	type Balance = FlipBalance;
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

	fn swap(
		ingress_asset: Asset,
		egress_asset: ForeignChainAsset,
		ingress_amount: Self::Balance,
	) -> Self::Balance;
}
