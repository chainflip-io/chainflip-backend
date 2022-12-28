#![cfg_attr(not(feature = "std"), no_std)]
use cf_primitives::{chains::assets::any, AmountU256, SqrtPriceQ64F96};
use cf_traits::Chainflip;
use chainflip_amm::{PoolState, MAX_FEE_100TH_BIPS};
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

impl<T: Config> cf_traits::SwappingApi for Pallet<T> {
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
				let (output_amount, asset_1_fee) =
					Pools::<T>::mutate(output_asset, |maybe_pool| {
						if let Some(pool) = maybe_pool.as_mut() {
							ensure!(pool.pool_state(), Error::<T>::PoolDisabled);
							Ok(pool.swap_from_pair_to_base(intermediate_amount))
						} else {
							Err(Error::<T>::PoolDoesNotExist)
						}
					})?;
				Ok((output_amount, asset_0_fee, asset_1_fee))
			},
		}
	}
}

// impl<T: Config> cf_traits::LiquidityPoolApi for Pallet<T> {
// 	const STABLE_ASSET: any::Asset = any::Asset::Usdc;

// 	fn deploy(asset: &any::Asset, position: cf_primitives::TradingPosition<AssetAmount>) {
// 		match position {
// 			TradingPosition::ClassicV3 { volume_0, volume_1, .. } => {
// 				Pools::<T>::mutate(asset, |pool| pool.add_liquidity(volume_0, volume_1));
// 			},
// 			TradingPosition::VolatileV3 { side, volume, .. } => {
// 				Pools::<T>::mutate(asset, |pool| match side {
// 					PoolAsset::Asset0 => pool.add_liquidity(volume, 0),
// 					PoolAsset::Asset1 => pool.add_liquidity(0, volume),
// 				});
// 			},
// 		}
// 	}

// 	fn retract(
// 		asset: &any::Asset,
// 		position: cf_primitives::TradingPosition<AssetAmount>,
// 	) -> (AssetAmount, AssetAmount) {
// 		match position {
// 			TradingPosition::ClassicV3 { volume_0, volume_1, .. } =>
// 				Pools::<T>::mutate(asset, |pool| pool.remove_liquidity(volume_0, volume_1)),
// 			TradingPosition::VolatileV3 { side, volume, .. } =>
// 				Pools::<T>::mutate(asset, |pool| match side {
// 					PoolAsset::Asset0 => pool.remove_liquidity(volume, 0),
// 					PoolAsset::Asset1 => pool.remove_liquidity(0, volume),
// 				}),
// 		}
// 	}

// 	fn get_liquidity(asset: &any::Asset) -> (AssetAmount, AssetAmount) {
// 		Pools::<T>::get(asset).get_liquidity()
// 	}

// 	fn swap_rate(
// 		input_asset: any::Asset,
// 		output_asset: any::Asset,
// 		input_amount: AssetAmount,
// 	) -> ExchangeRate {
// 		if input_amount == 0 {
// 			match (input_asset, output_asset) {
// 				(input_asset, any::Asset::Usdc) => Pools::<T>::get(input_asset).swap_rate(0),
// 				(any::Asset::Usdc, output_asset) =>
// 					Pools::<T>::get(output_asset).reversed().swap_rate(0),
// 				(input_asset, output_asset) => {
// 					let rate_1 = Pools::<T>::get(input_asset).swap_rate(0);
// 					let rate_2 = Pools::<T>::get(output_asset).reversed().swap_rate(0);
// 					rate_1 * rate_2
// 				},
// 			}
// 		} else {
// 			let output_amount = match (input_asset, output_asset) {
// 				(input_asset, any::Asset::Usdc) => Pools::<T>::get(input_asset).swap(input_amount),
// 				(any::Asset::Usdc, output_asset) =>
// 					Pools::<T>::get(output_asset).reverse_swap(input_amount),
// 				(input_asset, output_asset) => Pools::<T>::get(output_asset)
// 					.reverse_swap(Pools::<T>::get(input_asset).swap(input_amount)),
// 			};
// 			ExchangeRate::from_rational(output_amount, input_amount)
// 		}
// 	}

// 	fn get_liquidity_amount_by_position(
// 		_asset: &any::Asset,
// 		position: &TradingPosition<AssetAmount>,
// 	) -> Option<(AssetAmount, AssetAmount)> {
// 		// Naive placeholder implementation. Does not take account into existing liquidity in the
// 		// pool.
// 		Some(match position {
// 			TradingPosition::ClassicV3 { volume_0, volume_1, .. } => (*volume_0, *volume_1),
// 			TradingPosition::VolatileV3 { side, volume, .. } => match side {
// 				PoolAsset::Asset0 => (*volume, 0u128),
// 				PoolAsset::Asset1 => (0u128, *volume),
// 			},
// 		})
// 	}
// }
