use sp_runtime::DispatchResult;

use cf_primitives::{
	liquidity::TradingPosition, Asset, AssetAmount, ExchangeRate, ForeignChainAddress, PoolId,
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
	) -> DispatchResult;
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

pub trait SwappingApi {
	fn swap(
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
		fee: u16,
	) -> (AssetAmount, (Asset, AssetAmount));
}

pub trait LiquidityApi {
	fn deploy(asset: Asset, position: TradingPosition<AssetAmount>);
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

	fn swap(input_amount: AssetAmount, fee: u16) -> (AssetAmount, AssetAmount);
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
	) -> DispatchResult {
		Ok(())
	}
}

impl<T: frame_system::Config> LpProvisioningApi for T {
	type AccountId = T::AccountId;

	fn provision_account(
		_who: &Self::AccountId,
		_asset: Asset,
		_amount: AssetAmount,
	) -> DispatchResult {
		// TODO
		Ok(())
	}
}

impl SwappingApi for () {
	fn swap(
		_from: Asset,
		_to: Asset,
		_input_amount: AssetAmount,
		_fee: u16,
	) -> (AssetAmount, (Asset, AssetAmount)) {
		// TODO
		(0, (Asset::Usdc, 0))
	}
}
