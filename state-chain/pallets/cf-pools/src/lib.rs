#![cfg_attr(not(feature = "std"), no_std)]
use cf_primitives::{
	chains::assets::any, AccountId, AmmRange, AmountU256, Liquidity, PoolAsset, PoolAssetMap,
	SqrtPriceQ64F96,
};
use cf_traits::{Chainflip, LiquidityPoolApi};
use chainflip_amm::{MintError, PoolState, MAX_FEE_100TH_BIPS};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::OriginFor;
use sp_core::U256;

pub use pallet::*;

// #[cfg(test)]
// mod mock;

// #[cfg(test)]
// mod tests;

#[frame_support::pallet]
pub mod pallet {
	use cf_primitives::AssetAmount;

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::Origin>;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Pools are indexed by single asset since USDC is implicit.
	/// any::Asset::Usdc is always PoolAsset::Asset1
	#[pallet::storage]
	pub(super) type Pools<T: Config> =
		StorageMap<_, Twox64Concat, any::Asset, PoolState, OptionQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// The specified exchange pool does not exist.
		PoolDoesNotExist,
		/// The specified exchange pool already exists.
		PoolAlreadyExists,
		/// the Fee BIPs must be within the allowed range.
		InvalidFeeAmount,
		/// The exchange pool is currently disabled.
		PoolDisabled,
		/// The Upper or Lower tick is invalid.
		InvalidTickRange,
		/// One of the start/end ticks of the range reached its maximum gross liquidity
		MaximumGrossLiquidity,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		PoolStateUpdated {
			asset: any::Asset,
			enabled: bool,
		},
		NewPoolCreated {
			asset: any::Asset,
			fee_100th_bips: u32,
			initial_sqrt_price: SqrtPriceQ64F96,
		},
		LiquidityMinted {
			asset: any::Asset,
			provider: AccountId,
			range: AmmRange,
			asset_amount: AssetAmount,
			stable_amount: AssetAmount,
			liquidity_amount: Liquidity,
		},
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Enable or disable an exchange pool.
		/// Requires Governance.
		///
		/// ## Events
		///
		/// - [On update](Event::PoolStateUpdated)
		#[pallet::weight(0)]
		pub fn update_pool_state(
			origin: OriginFor<T>,
			asset: any::Asset,
			enabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			Pools::<T>::mutate(asset, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					pool.update_pool_state(enabled);
					Ok(())
				} else {
					Err(Error::<T>::PoolDoesNotExist)
				}
			})?;

			Self::deposit_event(Event::<T>::PoolStateUpdated { asset, enabled });

			Ok(())
		}

		/// Create a new pool with some initial liquidity. Pools are enabled by default.
		/// Requires Governance.
		///
		/// ## Events
		///
		/// - [On update](Event::PoolStateUpdated)
		#[pallet::weight(0)]
		pub fn new_pool(
			origin: OriginFor<T>,
			asset: any::Asset,
			fee_100th_bips: u32,
			initial_sqrt_price: SqrtPriceQ64F96,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			// Fee amount must be <= 50%
			ensure!(fee_100th_bips <= MAX_FEE_100TH_BIPS, Error::<T>::InvalidFeeAmount);
			Pools::<T>::mutate(asset, |maybe_pool| {
				if maybe_pool.is_some() {
					Err(Error::<T>::PoolAlreadyExists)
				} else {
					let pool = PoolState::new(fee_100th_bips, initial_sqrt_price);
					*maybe_pool = Some(pool);
					Ok(())
				}
			})?;

			Self::deposit_event(Event::<T>::NewPoolCreated {
				asset,
				fee_100th_bips,
				initial_sqrt_price,
			});

			Ok(())
		}
	}
}

impl<T: Config> cf_traits::SwappingApi<U256> for Pallet<T> {
	fn swap(
		from: any::Asset,
		to: any::Asset,
		input_amount: AmountU256,
	) -> Result<(AmountU256, AmountU256, AmountU256), DispatchError> {
		match (from, to) {
			(input_asset, any::Asset::Usdc) => Pools::<T>::mutate(input_asset, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					ensure!(pool.pool_state(), Error::<T>::PoolDisabled);
					let (output_amount, asset_0_fee) = pool.swap_from_base_to_pair(input_amount);
					Ok((output_amount, asset_0_fee, U256::zero()))
				} else {
					Err(Error::<T>::PoolDoesNotExist.into())
				}
			}),
			(any::Asset::Usdc, output_asset) => Pools::<T>::mutate(output_asset, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					ensure!(pool.pool_state(), Error::<T>::PoolDisabled);
					let (output_amount, asset_1_fee) = pool.swap_from_pair_to_base(input_amount);
					Ok((output_amount, Default::default(), asset_1_fee))
				} else {
					Err(Error::<T>::PoolDoesNotExist.into())
				}
			}),
			(input_asset, output_asset) => {
				let (intermediate_amount, asset_0_fee) =
					Pools::<T>::mutate(input_asset, |maybe_pool| {
						if let Some(pool) = maybe_pool.as_mut() {
							ensure!(pool.pool_state(), Error::<T>::PoolDisabled);
							Ok(pool.swap_from_base_to_pair(input_amount))
						} else {
							Err(Error::<T>::PoolDoesNotExist)
						}
					})?;
				let (output_amount, stable_asset_fee) =
					Pools::<T>::mutate(output_asset, |maybe_pool| {
						if let Some(pool) = maybe_pool.as_mut() {
							ensure!(pool.pool_state(), Error::<T>::PoolDisabled);
							Ok(pool.swap_from_pair_to_base(intermediate_amount))
						} else {
							Err(Error::<T>::PoolDoesNotExist)
						}
					})?;
				Ok((output_amount, asset_0_fee, stable_asset_fee))
			},
		}
	}
}

/// Implementation for Liquidity Pool API for chainflip-amm.
/// `Amount` and `AccountId` are hard-coded type locked in by the amm code.
impl<T: Config> LiquidityPoolApi<AmountU256, AccountId> for Pallet<T> {
	const STABLE_ASSET: any::Asset = any::Asset::Usdc;

	/// Deposit up to some amount of assets into an exchange pool. Minting some "Liquidity".
	fn mint(
		lp: AccountId,
		asset: any::Asset,
		range: AmmRange,
		max_asset_amount: AmountU256,
		max_stable_amount: AmountU256,
		check_callback: impl FnOnce(PoolAssetMap<AmountU256>) -> bool,
	) -> Result<(PoolAssetMap<AmountU256>, Liquidity), DispatchError> {
		Pools::<T>::mutate(&asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				ensure!(pool.pool_state(), Error::<T>::PoolDisabled);

				// TODO: Calculate maximum liquidity from the given asset.
				let liquidity_amount = 0;

				// Mint the Liquidity from the pool.
				let asset_spent: PoolAssetMap<AmountU256> = pool
					.mint(lp.clone(), range.lower, range.upper, liquidity_amount, check_callback)
					.map_err(|e| match e {
						MintError::InvalidTickRange => Error::<T>::InvalidTickRange,
						MintError::MaximumGrossLiquidity => Error::<T>::MaximumGrossLiquidity,
					})?;

				Self::deposit_event(Event::<T>::LiquidityMinted {
					asset,
					provider: lp,
					range,
					asset_amount: asset_spent[PoolAsset::Asset0].as_u128(),
					stable_amount: asset_spent[PoolAsset::Asset1].as_u128(),
					liquidity_amount,
				});

				Ok((asset_spent, liquidity_amount))
			} else {
				Err(Error::<T>::PoolDoesNotExist.into())
			}
		})
	}

	/// Burn some liquidity from an exchange pool to withdraw assets.
	fn burn(
		lp: AccountId,
		asset: any::Asset,
		range: AmmRange,
		burnt_liquidity: Liquidity,
	) -> DispatchResult {
		Ok(())
	}

	/// Collects fees yeilded by user's position into user's free balance.
	fn collect(lp: AccountId, asset: any::Asset, range: AmmRange) -> DispatchResult {
		Ok(())
	}

	/// Returns the user's Minted liquidity for a specific pool.
	fn minted_liqudity(lp: &AccountId, asset: &any::Asset) -> Vec<(AmmRange, Liquidity)> {
		vec![]
	}

	/// Gets the current price of the pool in SqrtPrice
	fn current_sqrt_price(asset: &any::Asset) -> Option<SqrtPriceQ64F96> {
		None
	}
}
