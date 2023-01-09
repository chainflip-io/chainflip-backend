use cf_primitives::{
	AmmRange, AmountU256, Asset, AssetAmount, ForeignChainAddress, Liquidity, PoolAssetMap,
	SqrtPriceQ64F96, Tick,
};
use frame_support::dispatch::DispatchError;
use sp_core::U256;
use sp_runtime::DispatchResult;
use sp_std::vec::Vec;

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

pub trait SwappingApi<Amount> {
	// Attempt to swap `from` asset to `to` asset.
	// If OK, return (output_amount, input_asset_fee, stable_asset_fee)
	fn swap(
		from: Asset,
		to: Asset,
		input_amount: Amount,
	) -> Result<(Amount, Amount, Amount), DispatchError>;
}

impl SwappingApi<AmountU256> for () {
	fn swap(
		_from: Asset,
		_to: Asset,
		_input_amount: AmountU256,
	) -> Result<(AmountU256, AmountU256, AmountU256), DispatchError> {
		Ok((U256::zero(), U256::zero(), U256::zero()))
	}
}

/// API to interface with the Pools pallet to manage Uniswap v3 style Exchange Pools.
/// All pools are Asset <-> USDC
pub trait LiquidityPoolApi<Amount, AccountId> {
	const STABLE_ASSET: Asset;

	/// Deposit up to some amount of assets into an exchange pool. Minting some "Liquidity".
	fn mint(
		lp: AccountId,
		asset: Asset,
		range: AmmRange,
		liquidity_amount: Liquidity,
		check_callback: impl FnOnce(PoolAssetMap<Amount>) -> bool,
	) -> Result<(PoolAssetMap<Amount>, Liquidity), DispatchError>;

	/// Burn some liquidity from an exchange pool to withdraw assets.
	fn burn(
		lp: AccountId,
		asset: Asset,
		range: AmmRange,
		burnt_liquidity: Liquidity,
	) -> Result<(PoolAssetMap<AmountU256>, PoolAssetMap<u128>), DispatchError>;

	/// Collects fees yeilded by user's position into user's free balance.
	fn collect(
		lp: AccountId,
		asset: Asset,
		range: AmmRange,
	) -> Result<PoolAssetMap<u128>, DispatchError>;

	/// Returns the user's Minted liquidities and fees acrued for a specific pool.
	fn minted_liqudity(
		lp: &AccountId,
		asset: &Asset,
	) -> Vec<(Tick, Tick, Liquidity, PoolAssetMap<u128>)>;

	/// Gets the current price of the pool in SqrtPrice
	fn current_sqrt_price(asset: &Asset) -> Option<SqrtPriceQ64F96>;

	/// Gets the current price of the pool in Tick
	fn current_tick(asset: &Asset) -> Option<Tick>;
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
