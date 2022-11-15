use sp_runtime::DispatchResult;

use cf_primitives::{
	liquidity::TradingPosition, AccountId, Asset, ExchangeRate, ForeignChainAddress, PoolId,
	PositionId,
};

use crate::FlipBalance;
use cf_primitives::AssetAmount;

pub trait SwapIntentHandler {
	type AccountId;
	fn schedule_swap(
		from: Asset,
		to: Asset,
		amount: AssetAmount,
		egress_address: ForeignChainAddress,
		relayer_id: Self::AccountId,
		relayer_commission_bps: u16,
	);
}

impl<T: frame_system::Config> SwapIntentHandler for T {
	type AccountId = T::AccountId;

	fn schedule_swap(
		_from: Asset,
		_to: Asset,
		_amount: AssetAmount,
		_egress_address: ForeignChainAddress,
		_relayer_id: Self::AccountId,
		_relayer_commission_bps: u16,
	) {
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
	type AccountId = u64;
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
		foreign_asset: &Asset,
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
		_foreign_asset: &Asset,
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
		from: Asset,
		to: Asset,
		swap_input: Self::Balance,
		fee: u16,
	) -> (Self::Balance, (Asset, Self::Balance));
}
