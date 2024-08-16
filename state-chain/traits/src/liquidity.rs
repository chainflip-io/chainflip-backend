use cf_amm::common::PoolPairsMap;
use cf_chains::assets::any::AssetMap;
use cf_primitives::{Asset, AssetAmount};
use frame_support::pallet_prelude::{DispatchError, DispatchResult};
use sp_std::{vec, vec::Vec};

pub trait LpDepositHandler {
	type AccountId;

	/// Attempt to credit the account with the given asset and amount
	/// as a result of a liquidity deposit.
	fn add_deposit(who: &Self::AccountId, asset: Asset, amount: AssetAmount) -> DispatchResult;
}

pub trait LpBalanceApi {
	type AccountId;

	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(
		who: &Self::AccountId,
		address: cf_chains::ForeignChainAddress,
	);

	fn ensure_has_refund_address_for_pair(
		who: &Self::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
	) -> DispatchResult;

	/// Attempt to credit the account with the given asset and amount.
	fn try_credit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult;

	/// Attempt to debit the account with the given asset and amount.
	fn try_debit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult;

	/// Record the fees collected by the account.
	fn record_fees(who: &Self::AccountId, amount: AssetAmount, asset: Asset);

	/// Returns the asset free balances of the given account.
	fn free_balances(who: &Self::AccountId) -> Result<AssetMap<AssetAmount>, DispatchError>;
}

pub trait PoolApi {
	type AccountId;

	/// Sweep all earnings of an LP into their free balance (Should be called before any assets are
	/// debited from their free balance)
	fn sweep(who: &Self::AccountId) -> Result<(), DispatchError>;

	/// Returns the number of open orders for the given account and pair.
	fn open_order_count(
		who: &Self::AccountId,
		asset_pair: &PoolPairsMap<Asset>,
	) -> Result<u32, DispatchError>;

	fn open_order_balances(who: &Self::AccountId) -> AssetMap<AssetAmount>;

	fn pools() -> Vec<PoolPairsMap<Asset>>;
}

impl<T: frame_system::Config> PoolApi for T {
	type AccountId = T::AccountId;

	fn sweep(_who: &Self::AccountId) -> Result<(), DispatchError> {
		Ok(())
	}

	fn open_order_count(
		_who: &Self::AccountId,
		_asset_pair: &PoolPairsMap<Asset>,
	) -> Result<u32, DispatchError> {
		Ok(0)
	}
	fn open_order_balances(_who: &Self::AccountId) -> AssetMap<AssetAmount> {
		AssetMap::from_fn(|_| 0)
	}
	fn pools() -> Vec<PoolPairsMap<Asset>> {
		vec![]
	}
}

pub trait SwappingApi {
	/// Process a single leg of a swap, into or from Stable asset. No network fee is taken.
	fn swap_single_leg(
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError>;
}

pub trait BoostApi {
	type AccountId;
	type AssetMap;

	fn boost_pool_account_balances(who: &Self::AccountId) -> Self::AssetMap;
}
