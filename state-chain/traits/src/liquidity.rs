use sp_runtime::DispatchResult;

use cf_primitives::{
	liquidity::TradingPosition, Asset, AssetAmount, ExchangeRate, ForeignChainAddress,
};

pub trait SwapIntentHandler {
	type AccountId;
	fn schedule_swap(
		ingress_address: ForeignChainAddress,
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

/// API to interface with Exchange Pools.
/// All pools are Asset <-> USDC
pub trait LiquidityPoolApi {
	const STABLE_ASSET: Asset;

	/// Deploy a liquidity position into a pool.
	fn deploy(asset: &Asset, position: TradingPosition<AssetAmount>);

	/// Retract a liquidity position from a pool.
	fn retract(asset: &Asset, position: TradingPosition<AssetAmount>)
		-> (AssetAmount, AssetAmount);

	/// Gets the current liquidity amount from a pool
	fn get_liquidity(asset: &Asset) -> (AssetAmount, AssetAmount);

	/// Gets the current swap rate for an pool
	fn swap_rate(asset: &Asset, input_amount: AssetAmount) -> ExchangeRate;

	/// Calculates the liquidity that corresponds to a given trading position.
	fn get_liquidity_amount_by_position(
		asset: &Asset,
		position: &TradingPosition<AssetAmount>,
	) -> Option<(AssetAmount, AssetAmount)>;
}

// TODO Remove these in favour of a real mocks.
impl<T: frame_system::Config> SwapIntentHandler for T {
	type AccountId = T::AccountId;

	fn schedule_swap(
		_ingress_address: ForeignChainAddress,
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
