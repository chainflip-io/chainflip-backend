#![cfg_attr(not(feature = "std"), no_std)]
use cf_amm::{CreatePoolError, MintError, PoolState, PositionError, MAX_FEE_100TH_BIPS};
use cf_primitives::{
	chains::assets::any, AccountId, AmmRange, AmountU256, AssetAmount, BurnResult, Liquidity,
	MintedLiquidity, PoolAssetMap, SwapResult, Tick,
};
use cf_traits::{Chainflip, LiquidityPoolApi};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::OriginFor;
use sp_std::{vec, vec::Vec};

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use cf_primitives::AssetAmount;

	use super::*;

	pub const STABLE_ASSET: any::Asset = any::Asset::Usdc;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Pools are indexed by single asset since USDC is implicit.
	/// any::Asset::Usdc is always PoolSide::Asset1
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
		/// the initial price must be within the allowed range.
		InvalidInitialPrice,
		/// The exchange pool is currently disabled.
		PoolDisabled,
		/// The Upper or Lower tick is invalid.
		InvalidTickRange,
		/// One of the start/end ticks of the range reached its maximum gross liquidity
		MaximumGrossLiquidity,
		/// User's position does not have enough liquidity.
		PositionLacksLiquidity,
		/// The user's position does not exist.
		PositionDoesNotExist,
		/// The user does not have enough balance to mint liquidity.
		InsufficientBalance,
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
			initial_tick_price: Tick,
		},
		LiquidityMinted {
			lp: AccountId,
			asset: any::Asset,
			range: AmmRange,
			minted_liquidity: Liquidity,
			asset_debited: PoolAssetMap<AssetAmount>,
		},
		LiquidityBurned {
			lp: AccountId,
			asset: any::Asset,
			range: AmmRange,
			burnt_liquidity: Liquidity,
			asset_credited: PoolAssetMap<AssetAmount>,
			fee_yielded: PoolAssetMap<AssetAmount>,
		},
		FeeCollected {
			lp: AccountId,
			asset: any::Asset,
			range: AmmRange,
			fee_yielded: PoolAssetMap<AssetAmount>,
		},
		AssetsSwapped {
			from: any::Asset,
			to: any::Asset,
			input: AssetAmount,
			output: AssetAmount,
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
		pub fn update_pool_enabled(
			origin: OriginFor<T>,
			asset: any::Asset,
			enabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			Pools::<T>::mutate(asset, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					pool.update_pool_enabled(enabled);
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
			initial_tick_price: Tick,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			// Fee amount must be <= 50%
			ensure!(fee_100th_bips <= MAX_FEE_100TH_BIPS, Error::<T>::InvalidFeeAmount);
			Pools::<T>::mutate(asset, |maybe_pool| {
				if maybe_pool.is_some() {
					Err(Error::<T>::PoolAlreadyExists)
				} else {
					let pool = PoolState::new(
						fee_100th_bips,
						PoolState::sqrt_price_at_tick(initial_tick_price),
					)
					.map_err(|e| match e {
						CreatePoolError::InvalidFeeAmount => Error::<T>::InvalidFeeAmount,
						CreatePoolError::InvalidInitialPrice => Error::<T>::InvalidInitialPrice,
					})?;
					*maybe_pool = Some(pool);
					Ok(())
				}
			})?;

			Self::deposit_event(Event::<T>::NewPoolCreated {
				asset,
				fee_100th_bips,
				initial_tick_price,
			});

			Ok(())
		}
	}
}

impl<T: Config> cf_traits::SwappingApi for Pallet<T> {
	fn swap(
		from: any::Asset,
		to: any::Asset,
		input_amount: AssetAmount,
	) -> Result<SwapResult, DispatchError> {
		match (from, to) {
			(input_asset, any::Asset::Usdc) => Pools::<T>::mutate(input_asset, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);
					let (output_amount, asset_0_fee) =
						pool.swap_from_base_to_pair(input_amount.into());
					Self::deposit_event(Event::<T>::AssetsSwapped {
						from,
						to,
						input: input_amount,
						output: output_amount
							.try_into()
							.expect("Swap output must be less than u128::MAX"),
					});
					Ok(SwapResult::new(
						output_amount.try_into().expect("Swap output must be less than u128::MAX"),
						asset_0_fee.try_into().expect("Swap fees must be less than u128::MAX"),
						Default::default(),
					))
				} else {
					Err(Error::<T>::PoolDoesNotExist.into())
				}
			}),
			(any::Asset::Usdc, output_asset) => Pools::<T>::mutate(output_asset, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);
					let (output_amount, asset_1_fee) =
						pool.swap_from_pair_to_base(input_amount.into());
					Self::deposit_event(Event::<T>::AssetsSwapped {
						from,
						to,
						input: input_amount,
						output: output_amount
							.try_into()
							.expect("Swap output must be less than u128::MAX"),
					});
					Ok(SwapResult::new(
						output_amount.try_into().expect("Swap output must be less than u128::MAX"),
						Default::default(),
						asset_1_fee.try_into().expect("Swap fees must be less than u128::MAX"),
					))
				} else {
					Err(Error::<T>::PoolDoesNotExist.into())
				}
			}),
			(input_asset, output_asset) => {
				let (intermediate_amount, asset_0_fee) =
					Pools::<T>::mutate(input_asset, |maybe_pool| {
						if let Some(pool) = maybe_pool.as_mut() {
							ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);
							Ok(pool.swap_from_base_to_pair(input_amount.into()))
						} else {
							Err(Error::<T>::PoolDoesNotExist)
						}
					})?;
				Self::deposit_event(Event::<T>::AssetsSwapped {
					from,
					to: STABLE_ASSET,
					input: input_amount,
					output: intermediate_amount
						.try_into()
						.expect("Swap output must be less than u128::MAX"),
				});

				let (output_amount, stable_asset_fee) =
					Pools::<T>::mutate(output_asset, |maybe_pool| {
						if let Some(pool) = maybe_pool.as_mut() {
							ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);
							Ok(pool.swap_from_pair_to_base(intermediate_amount))
						} else {
							Err(Error::<T>::PoolDoesNotExist)
						}
					})?;

				Self::deposit_event(Event::<T>::AssetsSwapped {
					from: STABLE_ASSET,
					to,
					input: intermediate_amount
						.try_into()
						.expect("Swap output must be less than u128::MAX"),
					output: output_amount
						.try_into()
						.expect("Swap output must be less than u128::MAX"),
				});
				Ok(SwapResult::new(
					output_amount.try_into().expect("Swap output must be less than u128::MAX"),
					asset_0_fee.try_into().expect("Swap fees must be less than u128::MAX"),
					stable_asset_fee.try_into().expect("Swap fees must be less than u128::MAX"),
				))
			},
		}
	}
}

/// Implementation of Liquidity Pool API for cf-amm.
/// `Amount` and `AccountId` are hard-coded type locked in by the amm code.
impl<T: Config> LiquidityPoolApi<AssetAmount, AccountId> for Pallet<T> {
	const STABLE_ASSET: any::Asset = STABLE_ASSET;

	/// Deposit up to some amount of assets into an exchange pool. Minting some "Liquidity".
	/// Returns Ok((asset_vested, liquidity_minted))
	fn mint(
		lp: AccountId,
		asset: any::Asset,
		range: AmmRange,
		liquidity_amount: Liquidity,
		should_mint: impl FnOnce(PoolAssetMap<AssetAmount>) -> bool,
	) -> Result<PoolAssetMap<AssetAmount>, DispatchError> {
		Pools::<T>::mutate(asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);

				let should_mint_u256 = |amount: PoolAssetMap<AmountU256>| -> bool {
					should_mint(
						amount
							.try_into()
							.expect("Mint required asset amounts must be less than u128::MAX"),
					)
				};
				// Mint the Liquidity from the pool.
				let asset_spent_u256: PoolAssetMap<AmountU256> = pool
					.mint(lp.clone(), range.lower, range.upper, liquidity_amount, should_mint_u256)
					.map_err(|e| match e {
						MintError::InvalidTickRange => Error::<T>::InvalidTickRange,
						MintError::MaximumGrossLiquidity => Error::<T>::MaximumGrossLiquidity,
						MintError::ShouldMintFunctionFailed => Error::<T>::InsufficientBalance,
					})?;

				let asset_debited = asset_spent_u256
					.try_into()
					.expect("Mint required asset amounts must be less than u128::MAX");
				Self::deposit_event(Event::<T>::LiquidityMinted {
					lp,
					asset,
					range,
					minted_liquidity: liquidity_amount,
					asset_debited,
				});

				Ok(asset_debited)
			} else {
				Err(Error::<T>::PoolDoesNotExist.into())
			}
		})
	}

	/// Burn some liquidity from an exchange pool to withdraw assets.
	/// Returns Ok((assets_retrieved, fee_accrued))
	fn burn(
		lp: AccountId,
		asset: any::Asset,
		range: AmmRange,
		burnt_liquidity: Liquidity,
	) -> Result<BurnResult, DispatchError> {
		Pools::<T>::mutate(asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);

				// Burn liquidity from the user's position.
				let (asset_credited_u256, fees): (PoolAssetMap<AmountU256>, PoolAssetMap<u128>) =
					pool.burn(lp.clone(), range.lower, range.upper, burnt_liquidity).map_err(
						|e| match e {
							PositionError::NonExistent => Error::<T>::PositionDoesNotExist,
							PositionError::PositionLacksLiquidity =>
								Error::<T>::PositionLacksLiquidity,
						},
					)?;

				let asset_credited = asset_credited_u256
					.try_into()
					.expect("Asset amount returned from Burn must be less than u128::MAX");
				Self::deposit_event(Event::<T>::LiquidityBurned {
					lp,
					asset,
					range,
					burnt_liquidity,
					asset_credited,
					fee_yielded: fees,
				});

				Ok(BurnResult::new(asset_credited, fees))
			} else {
				Err(Error::<T>::PoolDoesNotExist.into())
			}
		})
	}

	/// Collects fees yielded by user's position into user's free balance.
	fn collect(
		lp: AccountId,
		asset: any::Asset,
		range: AmmRange,
	) -> Result<PoolAssetMap<AssetAmount>, DispatchError> {
		Pools::<T>::mutate(asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);

				// Collect fees acrued by user's position.
				let fees: PoolAssetMap<AssetAmount> =
					pool.collect(lp.clone(), range.lower, range.upper).map_err(|e| match e {
						PositionError::NonExistent => Error::<T>::PositionDoesNotExist,
						PositionError::PositionLacksLiquidity => Error::<T>::PositionLacksLiquidity,
					})?;

				Self::deposit_event(Event::<T>::FeeCollected {
					lp,
					asset,
					range,
					fee_yielded: fees,
				});

				Ok(fees)
			} else {
				Err(Error::<T>::PoolDoesNotExist.into())
			}
		})
	}

	/// Returns the user's Minted liquidities and fees accrued for a specific pool.
	fn minted_liquidity(lp: &AccountId, asset: &any::Asset) -> Vec<MintedLiquidity> {
		if let Some(pool) = Pools::<T>::get(asset) {
			pool.minted_liquidity(lp.clone())
		} else {
			vec![]
		}
	}

	/// Gets the current price of the pool in Tick
	fn current_tick(asset: &any::Asset) -> Option<Tick> {
		Pools::<T>::get(asset).map(|pool| pool.current_tick())
	}
}
