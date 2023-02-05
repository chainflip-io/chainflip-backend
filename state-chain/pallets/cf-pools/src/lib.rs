#![cfg_attr(not(feature = "std"), no_std)]
use cf_amm::{
	CreatePoolError, MintError, PoolState, PositionError, MAX_FEE_100TH_BIPS, MAX_TICK, MIN_TICK,
};
use cf_primitives::{
	chains::assets::any, AccountId, AmmRange, AmountU256, AssetAmount, BurnResult, Liquidity,
	PoolAssetMap, Tick,
};
use cf_traits::{Chainflip, LiquidityPoolApi, SwappingApi};
use frame_support::{pallet_prelude::*, transactional};
use frame_system::pallet_prelude::OriginFor;
use sp_arithmetic::traits::Zero;
use sp_runtime::{Permill, Saturating};
use sp_std::vec;

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use frame_system::pallet_prelude::BlockNumberFor;

	use super::*;

	pub const STABLE_ASSET: any::Asset = any::Asset::Usdc;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		#[pallet::constant]
		type NetworkFee: Get<Permill>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Pools are indexed by single asset since USDC is implicit.
	/// The STABLE_ASSET is always PoolSide::Asset1
	#[pallet::storage]
	pub(super) type Pools<T: Config> =
		StorageMap<_, Twox64Concat, any::Asset, PoolState, OptionQuery>;

	/// FLIP ready to be burned.
	#[pallet::storage]
	pub(super) type FlipToBurn<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	/// Interval at which we buy FLIP in order to burn it.
	#[pallet::storage]
	pub(super) type FlipBuyInterval<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// Network fee
	#[pallet::storage]
	pub type CollectedNetworkFee<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub flip_buy_interval: T::BlockNumber,
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			FlipBuyInterval::<T>::set(T::BlockNumber::from(1_u32));
		}
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { flip_buy_interval: T::BlockNumber::from(1_u32) }
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			// Note: FlipBuyInterval is never zero!
			if current_block % FlipBuyInterval::<T>::get() == Zero::zero() &&
				CollectedNetworkFee::<T>::get() != 0
			{
				CollectedNetworkFee::<T>::mutate(|collected_fee| {
					if let Ok(flip_to_burn) =
						Pallet::<T>::swap(STABLE_ASSET, any::Asset::Flip, *collected_fee)
					{
						FlipToBurn::<T>::mutate(|total| {
							total.saturating_accrue(flip_to_burn);
						});
						*collected_fee = Default::default();
					}
				});
			}
			Weight::from_ref_time(0)
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Setting the buy interval to zero is not allowed.
		ZeroBuyIntervalNotAllowed,
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
		/// The tick is invalid.
		InvalidTick,
		/// One of the start/end ticks of the range reached its maximum gross liquidity
		MaximumGrossLiquidity,
		/// User's position does not have enough liquidity.
		PositionLacksLiquidity,
		/// The user's position does not exist.
		PositionDoesNotExist,
		/// The pool does not have enough liquidity left to process the swap.
		InsufficientLiquidity,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		UpdatedBuyInterval {
			buy_interval: T::BlockNumber,
		},
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
			assets_debited: PoolAssetMap<AssetAmount>,
			fees_harvested: PoolAssetMap<AssetAmount>,
		},
		LiquidityBurned {
			lp: AccountId,
			asset: any::Asset,
			range: AmmRange,
			burnt_liquidity: Liquidity,
			assets_returned: PoolAssetMap<AssetAmount>,
			fees_harvested: PoolAssetMap<AssetAmount>,
		},
		NetworkFeeTaken {
			fee_amount: AssetAmount,
		},
		AssetsSwapped {
			from: any::Asset,
			to: any::Asset,
			input: AssetAmount,
			output: AssetAmount,
			liquidity_fee: AssetAmount,
		},
		LiquidityFeeUpdated {
			asset: any::Asset,
			fee_100th_bips: u32,
		},
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Updates the buy interval.
		///
		/// ## Events
		///
		/// - [UpdatedBuyInterval](Event::UpdatedBuyInterval)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [ZeroBuyIntervalNotAllowed](pallet_cf_pools::Error::ZeroBuyIntervalNotAllowed)
		#[pallet::weight(T::WeightInfo::update_buy_interval())]
		pub fn update_buy_interval(
			origin: OriginFor<T>,
			new_buy_interval: T::BlockNumber,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(new_buy_interval != Zero::zero(), Error::<T>::ZeroBuyIntervalNotAllowed);
			FlipBuyInterval::<T>::set(new_buy_interval);
			Self::deposit_event(Event::<T>::UpdatedBuyInterval { buy_interval: new_buy_interval });
			Ok(())
		}

		/// Enable or disable an exchange pool.
		/// Requires Governance.
		///
		/// ## Events
		///
		/// - [On update](Event::PoolStateUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [PoolDoesNotExist](pallet_cf_pools::Error::PoolDoesNotExist)
		#[pallet::weight(T::WeightInfo::update_pool_enabled())]
		pub fn update_pool_enabled(
			origin: OriginFor<T>,
			asset: any::Asset,
			enabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			Pools::<T>::try_mutate(asset, |maybe_pool| {
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
		/// - [On success](Event::NewPoolCreated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [InvalidFeeAmount](pallet_cf_pools::Error::InvalidFeeAmount)
		/// - [InvalidTick](pallet_cf_pools::Error::InvalidTick)
		/// - [InvalidInitialPrice](pallet_cf_pools::Error::InvalidInitialPrice)
		/// - [PoolAlreadyExists](pallet_cf_pools::Error::PoolAlreadyExists)
		#[pallet::weight(T::WeightInfo::new_pool())]
		pub fn new_pool(
			origin: OriginFor<T>,
			asset: any::Asset,
			fee_100th_bips: u32,
			initial_tick_price: Tick,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			// Fee amount must be <= 50%
			ensure!(fee_100th_bips <= MAX_FEE_100TH_BIPS, Error::<T>::InvalidFeeAmount);
			ensure!((MIN_TICK..=MAX_TICK).contains(&initial_tick_price), Error::<T>::InvalidTick);
			Pools::<T>::try_mutate(asset, |maybe_pool| {
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

		/// Sets the liquidity fee for an exchange pool.
		///
		/// Requires governance origin.
		///
		/// ## Events
		///
		/// - [On success](Event::LiquidityFeeUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [InvalidFeeAmount](pallet_cf_pools::Error::InvalidFeeAmount)
		/// - [PoolDoesNotExist](pallet_cf_pools::Error::PoolDoesNotExist)
		#[pallet::weight(T::WeightInfo::set_liquidity_fee())]
		pub fn set_liquidity_fee(
			origin: OriginFor<T>,
			asset: any::Asset,
			fee_100th_bips: u32,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			Pools::<T>::try_mutate(asset, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					pool.set_liquidity_fees(fee_100th_bips).map_err(|_| Error::InvalidFeeAmount)
				} else {
					Err(Error::<T>::PoolDoesNotExist)
				}
			})?;
			Self::deposit_event(Event::<T>::LiquidityFeeUpdated { asset, fee_100th_bips });

			Ok(())
		}
	}
}

impl<T: Config> SwappingApi for Pallet<T> {
	#[transactional]
	fn swap(
		from: any::Asset,
		to: any::Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		Ok(match (from, to) {
			(input_asset, STABLE_ASSET) => {
				let gross_output =
					Self::process_swap_leg(SwapLeg::ToStable, input_asset, input_amount)?;
				Self::take_network_fee(gross_output)
			},
			(STABLE_ASSET, output_asset) => {
				let net_input = Self::take_network_fee(input_amount);
				Self::process_swap_leg(SwapLeg::FromStable, output_asset, net_input)?
			},
			(input_asset, output_asset) => {
				let intermediate_output =
					Self::process_swap_leg(SwapLeg::ToStable, input_asset, input_amount)?;
				let intermediate_input = Self::take_network_fee(intermediate_output);
				Self::process_swap_leg(SwapLeg::FromStable, output_asset, intermediate_input)?
			},
		})
	}
}

/// Implementation of Liquidity Pool API for cf-amm.
impl<T: Config> LiquidityPoolApi<AccountId> for Pallet<T> {
	const STABLE_ASSET: any::Asset = STABLE_ASSET;

	fn mint(
		lp: AccountId,
		asset: any::Asset,
		range: AmmRange,
		liquidity_amount: Liquidity,
		try_debit: impl FnOnce(PoolAssetMap<AssetAmount>) -> Result<(), DispatchError>,
	) -> Result<PoolAssetMap<AssetAmount>, DispatchError> {
		Pools::<T>::try_mutate(asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);

				let try_debit_u256 = |amount: PoolAssetMap<AmountU256>| {
					try_debit(
						amount
							.try_into()
							.expect("Mint required asset amounts must be less than u128::MAX"),
					)
				};

				let (assets_debited, fees_harvested) = pool
					.mint(lp.clone(), range.lower, range.upper, liquidity_amount, try_debit_u256)
					.map_err(|e| match e {
						MintError::InvalidTickRange => Error::<T>::InvalidTickRange.into(),
						MintError::MaximumGrossLiquidity =>
							Error::<T>::MaximumGrossLiquidity.into(),
						MintError::CallbackError(e) => e,
					})?;

				let assets_debited = assets_debited
					.try_into()
					.expect("Mint required asset amounts must be less than u128::MAX");
				Self::deposit_event(Event::<T>::LiquidityMinted {
					lp,
					asset,
					range,
					minted_liquidity: liquidity_amount,
					assets_debited,
					fees_harvested,
				});

				Ok(fees_harvested)
			} else {
				Err(Error::<T>::PoolDoesNotExist.into())
			}
		})
	}

	fn burn(
		lp: AccountId,
		asset: any::Asset,
		range: AmmRange,
		burnt_liquidity: Liquidity,
	) -> Result<BurnResult, DispatchError> {
		Pools::<T>::try_mutate(asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);

				let (assets_returned_u256, fees_harvested): (
					PoolAssetMap<AmountU256>,
					PoolAssetMap<u128>,
				) =
					pool.burn(lp.clone(), range.lower, range.upper, burnt_liquidity).map_err(
						|e| match e {
							PositionError::NonExistent => Error::<T>::PositionDoesNotExist,
							PositionError::PositionLacksLiquidity =>
								Error::<T>::PositionLacksLiquidity,
						},
					)?;

				let assets_returned = assets_returned_u256
					.try_into()
					.expect("Asset amount returned from Burn must be less than u128::MAX");
				Self::deposit_event(Event::<T>::LiquidityBurned {
					lp,
					asset,
					range,
					burnt_liquidity,
					assets_returned,
					fees_harvested,
				});

				Ok(BurnResult::new(assets_returned, fees_harvested))
			} else {
				Err(Error::<T>::PoolDoesNotExist.into())
			}
		})
	}

	fn minted_liquidity(lp: &AccountId, asset: &any::Asset, range: AmmRange) -> Liquidity {
		if let Some(pool) = Pools::<T>::get(asset) {
			pool.minted_liquidity(lp.clone(), range)
		} else {
			Default::default()
		}
	}

	fn current_tick(asset: &any::Asset) -> Option<Tick> {
		Pools::<T>::get(asset).map(|pool| pool.current_tick())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn new_pool(asset: any::Asset) {
		Pools::<T>::insert(asset, PoolState::new(0u32, PoolState::sqrt_price_at_tick(0)).unwrap());
	}
}

impl<T: Config> cf_traits::FlipBurnInfo for Pallet<T> {
	fn take_flip_to_burn() -> AssetAmount {
		FlipToBurn::<T>::take()
	}
}

enum SwapLeg {
	FromStable,
	ToStable,
}

impl<T: Config> Pallet<T> {
	fn calc_fee(fee: Permill, input: AssetAmount) -> AssetAmount {
		fee * input
	}

	pub fn take_network_fee(input: AssetAmount) -> AssetAmount {
		let fee = Self::calc_fee(T::NetworkFee::get(), input);
		CollectedNetworkFee::<T>::mutate(|total| {
			*total = total.saturating_add(fee);
		});
		Self::deposit_event(Event::<T>::NetworkFeeTaken { fee_amount: fee });
		input.saturating_sub(fee)
	}

	fn process_swap_leg(
		direction: SwapLeg,
		asset: any::Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, Error<T>> {
		Pools::<T>::try_mutate(asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				ensure!(pool.pool_enabled(), Error::<T>::PoolDisabled);
				let (from, to, (output_amount, fee)) = match direction {
					SwapLeg::FromStable => (
						STABLE_ASSET,
						asset,
						pool.swap_from_asset_1_to_asset_0(input_amount.into())
							.map_err(|_| Error::<T>::InsufficientLiquidity)?,
					),
					SwapLeg::ToStable => (
						asset,
						STABLE_ASSET,
						pool.swap_from_asset_0_to_asset_1(input_amount.into())
							.map_err(|_| Error::<T>::InsufficientLiquidity)?,
					),
				};
				Self::deposit_event(Event::<T>::AssetsSwapped {
					from,
					to,
					input: input_amount,
					output: output_amount
						.try_into()
						.expect("Swap output must be less than u128::MAX"),
					liquidity_fee: fee.try_into().expect("Swap fees must be less than u128::MAX"),
				});
				Ok(output_amount.try_into().expect("Swap output must be less than u128::MAX"))
			} else {
				Err(Error::<T>::PoolDoesNotExist)
			}
		})
	}
}
