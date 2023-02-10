use cf_primitives::{
	AmmRange, Asset, AssetAmount, BurnResult, ForeignChainAddress, Liquidity, PoolAssetMap, Tick,
};
use frame_support::dispatch::DispatchError;
use sp_runtime::DispatchResult;

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
	// If OK, return (output_amount, input_asset_fee, stable_asset_fee)
	fn swap(
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError>;
}

impl SwappingApi for () {
	fn swap(
		_from: Asset,
		_to: Asset,
		_input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		Ok(Default::default())
	}
}

/// API to interface with the Pools pallet to manage Uniswap v3 style Exchange Pools.
/// All pools are Asset <-> USDC
pub trait LiquidityPoolApi<AccountId> {
	const STABLE_ASSET: Asset;

	/// Deposit up to some amount of assets into an exchange pool.
	///
	/// The passed `try_debit` closure should debit the account balance and fail if this
	/// is not possible.
	///
	/// Returns the harvested fees, if any.
	fn mint(
		lp: AccountId,
		asset: Asset,
		range: AmmRange,
		liquidity_amount: Liquidity,
		try_debit: impl FnOnce(PoolAssetMap<AssetAmount>) -> Result<(), DispatchError>,
	) -> Result<PoolAssetMap<AssetAmount>, DispatchError>;

	/// Burn some liquidity from an exchange pool to withdraw assets and harvest fees.
	fn burn(
		lp: AccountId,
		asset: Asset,
		range: AmmRange,
		burnt_liquidity: Liquidity,
	) -> Result<BurnResult, DispatchError>;

	/// Returns the LP's minted liquidity for a specific position in a pool.
	fn minted_liquidity(lp: &AccountId, asset: &Asset, range: AmmRange) -> Liquidity;

	/// Gets the current price of the pool in Tick.
	fn current_tick(asset: &Asset) -> Option<Tick>;

	#[cfg(feature = "runtime-benchmarks")]
	fn new_pool(_asset: Asset) {}
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
