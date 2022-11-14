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

	/// Called when ingress is witnessed.
	fn provision_account(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult;
}

pub trait PositionManagementApi {
	type AccountId;
	fn open_position(
		who: &Self::AccountId,
		pool_id: PoolId,
		position: TradingPosition<AssetAmount>,
	) -> DispatchResult;
	fn update_position(
		who: &Self::AccountId,
		pool_id: PoolId,
		id: PositionId,
		new_position: TradingPosition<AssetAmount>,
	) -> DispatchResult;
	fn close_position(who: &Self::AccountId, id: PositionId) -> DispatchResult;
}

pub trait SwappingApi {
	fn swap(
		from: Asset,
		to: Asset,
		swap_input: AssetAmount,
		fee: u16,
	) -> (AssetAmount, (Asset, AssetAmount));
}

pub trait AmmPoolApi {
	fn asset_0(&self) -> Asset;
	fn asset_1(&self) -> Asset;
	fn liquidity_0(&self) -> AssetAmount;
	fn liquidity_1(&self) -> AssetAmount;

	fn pool_id(&self) -> PoolId {
		(self.asset_0(), self.asset_1())
	}

	fn get_exchange_rate(&self) -> ExchangeRate;

	fn get_liquidity_requirement(
		&self,
		position: &TradingPosition<AssetAmount>,
	) -> Option<(AssetAmount, AssetAmount)>;

	fn swap(swap_input: AssetAmount, fee: u16) -> (AssetAmount, AssetAmount);
}

// TODO Remove these in favour of a real mocks.
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
		unimplemented!()
	}
}

impl<T: frame_system::Config> LpProvisioningApi for T {
	type AccountId = T::AccountId;

	fn provision_account(
		_who: &Self::AccountId,
		_asset: Asset,
		_amount: AssetAmount,
	) -> DispatchResult {
		unimplemented!()
	}
}

impl SwappingApi for () {
	fn swap(
		_from: Asset,
		_to: Asset,
		_swap_input: AssetAmount,
		_fee: u16,
	) -> (AssetAmount, (Asset, AssetAmount)) {
		unimplemented!()
	}
}
