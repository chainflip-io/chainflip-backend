use sp_runtime::DispatchResult;

use cf_primitives::{
	liquidity::TradingPosition, Asset, AssetAmount, ExchangeRate, ForeignChainAddress, PoolId,
	PositionId,
};

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

pub trait PositionManagementApi {
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

pub trait SwappingApi {
	type Balance;

	fn swap(
		from: Asset,
		to: Asset,
		swap_input: Self::Balance,
		fee: u16,
	) -> (Self::Balance, (Asset, Self::Balance));
}

// TODO: remove this.
impl SwappingApi for () {
	type Balance = u128;

	fn swap(
		_from: Asset,
		_to: Asset,
		_swap_input: Self::Balance,
		_fee: u16,
	) -> (Self::Balance, (Asset, Self::Balance)) {
		unimplemented!()
	}
}

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

	fn swap(swap_input: Self::Balance, fee: u16) -> (Self::Balance, Self::Balance);
}
