use sp_runtime::DispatchResult;

use cf_primitives::{
	liquidity::TradingPosition, Asset, AssetAmount, ExchangeRate, ForeignChainAddress, AmountU256
};
use frame_support::dispatch::DispatchError;

pub trait SwapIntentHandler {
	type AccountId;
	fn on_swap_ingress(
		ingress_address: ForeignChainAddress,
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

pub trait SwappingApi {
	// Attempt to swap `from` asset to `to` asset.
	// If OK, return (output_amount, input_asset_fee, output_asset_fee)
	fn swap(
		from: Asset,
		to: Asset,
		input_amount: AmountU256,
	) -> Result<(AmountU256, AmountU256, AmountU256), DispatchError>;
}

impl SwappingApi for () {
	fn swap(
		_from: Asset,
		_to: Asset,
		_input_amount: AmountU256,
	) -> Result<(AmountU256, AmountU256, AmountU256), DispatchError> {
		Ok((Default::default(), Default::default(), Default::default()))
	}
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
	fn swap_rate(
		input_asset: Asset,
		output_asset: Asset,
		input_amount: AssetAmount,
	) -> ExchangeRate;

	/// Calculates the liquidity that corresponds to a given trading position.
	fn get_liquidity_amount_by_position(
		asset: &Asset,
		position: &TradingPosition<AssetAmount>,
	) -> Option<(AssetAmount, AssetAmount)>;
}

// TODO Remove these in favour of a real mocks.
impl<T: frame_system::Config> SwapIntentHandler for T {
	type AccountId = T::AccountId;

	fn on_swap_ingress(
		_ingress_address: ForeignChainAddress,
		_from: Asset,
		_to: Asset,
		_amount: AssetAmount,
		_egress_address: ForeignChainAddress,
		_relayer_id: Self::AccountId,
		_relayer_commission_bps: u16,
	) {
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