#![cfg_attr(not(feature = "std"), no_std)]
use cf_amm::{
	common::{OneToZero, Side, SideMap, SqrtPriceQ64F96, ZeroToOne},
	PoolState,
};
use cf_primitives::{chains::assets::any, Asset, AssetAmount, SwapLeg, SwapOutput, STABLE_ASSET};
use cf_traits::{Chainflip, LpBalanceApi, SwappingApi};
use frame_support::{pallet_prelude::*, transactional};
use frame_system::pallet_prelude::OriginFor;
use sp_arithmetic::traits::Zero;
use sp_runtime::{Permill, Saturating};

pub use pallet::*;

mod benchmarking;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use cf_amm::{
		common::{OneToZero, Side, SqrtPriceQ64F96, Tick, ZeroToOne},
		limit_orders,
		range_orders::{self, Liquidity},
	};
	use cf_traits::{AccountRoleRegistry, LpBalanceApi};
	use frame_system::pallet_prelude::BlockNumberFor;

	use super::*;

	#[derive(Clone, Debug, Encode, Decode, TypeInfo)]
	pub struct Pool<LiquidityProvider> {
		pub enabled: bool,
		pub pool_state: PoolState<LiquidityProvider>,
	}

	#[derive(PartialEq, Eq, Copy, Clone, Debug, Encode, Decode, TypeInfo)]
	pub enum Order {
		Buy,
		Sell,
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Pallet responsible for managing Liquidity Providers.
		type LpBalance: LpBalanceApi<AccountId = Self::AccountId>;

		#[pallet::constant]
		type NetworkFee: Get<Permill>;

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
		StorageMap<_, Twox64Concat, any::Asset, Pool<T::AccountId>, OptionQuery>;

	/// FLIP ready to be burned.
	#[pallet::storage]
	pub(super) type FlipToBurn<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	/// Interval at which we buy FLIP in order to burn it.
	#[pallet::storage]
	pub(super) type FlipBuyInterval<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// Network fees, in USDC terms, that have been collected and are ready to be converted to FLIP.
	#[pallet::storage]
	pub type CollectedNetworkFee<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub flip_buy_interval: T::BlockNumber,
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			FlipBuyInterval::<T>::set(self.flip_buy_interval);
		}
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { flip_buy_interval: T::BlockNumber::zero() }
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let mut weight_used: Weight = T::DbWeight::get().reads(1);
			let interval = FlipBuyInterval::<T>::get();
			if interval.is_zero() {
				log::debug!("Flip buy interval is zero, skipping.")
			} else {
				weight_used.saturating_accrue(T::DbWeight::get().reads(1));
				if (current_block % interval).is_zero() &&
					!CollectedNetworkFee::<T>::get().is_zero()
				{
					weight_used.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
					if let Err(e) = CollectedNetworkFee::<T>::try_mutate(|collected_fee| {
						let flip_to_burn = Self::swap_single_leg(
							SwapLeg::FromStable,
							any::Asset::Flip,
							*collected_fee,
						)?;
						FlipToBurn::<T>::mutate(|total| {
							total.saturating_accrue(flip_to_burn);
						});
						collected_fee.set_zero();
						Ok::<_, DispatchError>(())
					}) {
						log::warn!("Unable to swap Network Fee to Flip: {e:?}");
					}
				}
			}
			weight_used
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Setting the buy interval to zero is not allowed.
		ZeroBuyIntervalNotAllowed,
		/// The specified exchange pool already exists.
		PoolAlreadyExists,
		/// The specified exchange pool does not exist.
		PoolDoesNotExist,
		/// The exchange pool is currently disabled.
		PoolDisabled,
		/// the Fee BIPs must be within the allowed range.
		InvalidFeeAmount,
		/// the initial price must be within the allowed range.
		InvalidInitialPrice,
		/// The Upper or Lower tick is invalid.
		InvalidTickRange,
		/// The tick is invalid.
		InvalidTick,
		/// One of the referenced ticks reached its maximum gross liquidity
		MaximumGrossLiquidity,
		/// User's position does not have enough liquidity.
		PositionLacksLiquidity,
		/// The user's position does not exist.
		PositionDoesNotExist,
		/// It is no longer possible to mint limit orders due to reaching the maximum pool
		/// instances, other than for ticks where a fixed pool currently exists.
		MaximumPoolInstances,

		/// The pool does not have enough liquidity left to process the swap.
		InsufficientLiquidity,
		/// The swap output is past the maximum allowed amount.
		OutputOverflow,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		UpdatedBuyInterval {
			buy_interval: T::BlockNumber,
		},
		PoolStateUpdated {
			unstable_asset: any::Asset,
			enabled: bool,
		},
		NewPoolCreated {
			unstable_asset: any::Asset,
			fee_hundredth_pips: u32,
			initial_sqrt_price: SqrtPriceQ64F96,
		},
		RangeOrderMinted {
			lp: T::AccountId,
			unstable_asset: any::Asset,
			price_range_in_ticks: core::ops::Range<Tick>,
			liquidity: Liquidity,
			assets_debited: SideMap<AssetAmount>,
			collected_fees: SideMap<AssetAmount>,
		},
		RangeOrderBurned {
			lp: T::AccountId,
			unstable_asset: any::Asset,
			price_range_in_ticks: core::ops::Range<Tick>,
			liquidity: Liquidity,
			assets_credited: SideMap<AssetAmount>,
			collected_fees: SideMap<AssetAmount>,
		},
		LimitOrderMinted {
			lp: T::AccountId,
			unstable_asset: any::Asset,
			order: Order,
			price_as_tick: Tick,
			assets_debited: AssetAmount,
			collected_fees: AssetAmount,
			swapped_liquidity: AssetAmount,
		},
		LimitOrderBurned {
			lp: T::AccountId,
			unstable_asset: any::Asset,
			order: Order,
			price_as_tick: Tick,
			assets_credited: AssetAmount,
			collected_fees: AssetAmount,
			swapped_liquidity: AssetAmount,
		},
		NetworkFeeTaken {
			fee_amount: AssetAmount,
		},
		AssetSwapped {
			from: any::Asset,
			to: any::Asset,
			input_amount: AssetAmount,
			output_amount: AssetAmount,
		},
		LiquidityFeeUpdated {
			unstable_asset: any::Asset,
			fee_hundredth_pips: u32,
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
			unstable_asset: any::Asset,
			enabled: bool,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			Pools::<T>::try_mutate(unstable_asset, |maybe_pool| {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;
				pool.enabled = enabled;
				Self::deposit_event(Event::<T>::PoolStateUpdated { unstable_asset, enabled });
				Ok(())
			})
		}

		/// Create a new pool. Pools are enabled by default.
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
			unstable_asset: any::Asset,
			fee_hundredth_pips: u32,
			initial_sqrt_price: SqrtPriceQ64F96,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			Pools::<T>::try_mutate(unstable_asset, |maybe_pool| {
				ensure!(maybe_pool.is_none(), Error::<T>::PoolAlreadyExists);

				*maybe_pool = Some(Pool {
					enabled: true,
					pool_state: PoolState {
						limit_orders: limit_orders::PoolState::new(fee_hundredth_pips).map_err(
							|e| match e {
								limit_orders::NewError::InvalidFeeAmount =>
									Error::<T>::InvalidFeeAmount,
							},
						)?,
						range_orders: range_orders::PoolState::new(
							fee_hundredth_pips,
							initial_sqrt_price,
						)
						.map_err(|e| match e {
							range_orders::NewError::InvalidFeeAmount =>
								Error::<T>::InvalidFeeAmount,
							range_orders::NewError::InvalidInitialPrice =>
								Error::<T>::InvalidInitialPrice,
						})?,
					},
				});

				Ok::<_, Error<T>>(())
			})?;

			Self::deposit_event(Event::<T>::NewPoolCreated {
				unstable_asset,
				fee_hundredth_pips,
				initial_sqrt_price,
			});

			Ok(())
		}

		/// Collects and mints a range order.
		///
		/// ## Events
		///
		/// - [On success](Event::RangeOrderMinted)
		/// - [On success](Event::AccountDebited)
		/// - [On success](Event::AccountCredited)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [PoolDoesNotExist](pallet_cf_pools::Error::PoolDoesNotExist)
		/// - [PoolDisabled](pallet_cf_pools::Error::PoolDisabled)
		/// - [InvalidTickRange](pallet_cf_pools::Error::InvalidTickRange)
		/// - [PositionDoesNotExist](pallet_cf_pools::Error::PositionDoesNotExist)
		/// - [MaximumGrossLiquidity](pallet_cf_pools::Error::MaximumGrossLiquidity)
		/// - [InsufficientBalance](pallet_cf_lp::Error::InsufficientBalance)
		#[pallet::weight(T::WeightInfo::collect_and_mint_range_order())]
		pub fn collect_and_mint_range_order(
			origin: OriginFor<T>,
			unstable_asset: any::Asset,
			price_range_in_ticks: core::ops::Range<Tick>,
			liquidity: Liquidity,
		) -> DispatchResult {
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_pool_state(unstable_asset, |pool_state| {
				let (assets_debited, range_orders::CollectedFees { fees }) = pool_state
					.range_orders
					.collect_and_mint(
						&lp,
						price_range_in_ticks.start,
						price_range_in_ticks.end,
						liquidity,
						|required_amounts| {
							Self::try_debit_both_assets(&lp, unstable_asset, required_amounts)
						},
					)
					.map_err(|e| match e {
						range_orders::PositionError::InvalidTickRange =>
							Error::<T>::InvalidTickRange.into(),
						range_orders::PositionError::NonExistent =>
							Error::<T>::PositionDoesNotExist.into(),
						range_orders::PositionError::Other(
							range_orders::MintError::CallbackFailed(e),
						) => e,
						range_orders::PositionError::Other(
							range_orders::MintError::MaximumGrossLiquidity,
						) => Error::<T>::MaximumGrossLiquidity.into(),
					})?;

				let collected_fees = Self::try_credit_both_assets(&lp, unstable_asset, fees)?;

				Self::deposit_event(Event::<T>::RangeOrderMinted {
					lp,
					unstable_asset,
					price_range_in_ticks,
					liquidity,
					assets_debited,
					collected_fees,
				});

				Ok(())
			})
		}

		/// Collects and burns a range order.
		///
		/// ## Events
		///
		/// - [On success](Event::RangeOrderBurned)
		/// - [On success](Event::AccountCredited)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [PoolDoesNotExist](pallet_cf_pools::Error::PoolDoesNotExist)
		/// - [PoolDisabled](pallet_cf_pools::Error::PoolDisabled)
		/// - [InvalidTickRange](pallet_cf_pools::Error::InvalidTickRange)
		/// - [PositionDoesNotExist](pallet_cf_pools::Error::PositionDoesNotExist)
		/// - [PositionLacksLiquidity](pallet_cf_pools::Error::PositionLacksLiquidity)
		#[pallet::weight(T::WeightInfo::collect_and_burn_range_order())]
		pub fn collect_and_burn_range_order(
			origin: OriginFor<T>,
			unstable_asset: any::Asset,
			price_range_in_ticks: core::ops::Range<Tick>,
			liquidity: Liquidity,
		) -> DispatchResult {
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_pool_state(unstable_asset, |pool_state| {
				let (assets_withdrawn, range_orders::CollectedFees { fees }) = pool_state
					.range_orders
					.collect_and_burn(
						&lp,
						price_range_in_ticks.start,
						price_range_in_ticks.end,
						liquidity,
					)
					.map_err(|e| match e {
						range_orders::PositionError::InvalidTickRange =>
							Error::<T>::InvalidTickRange,
						range_orders::PositionError::NonExistent =>
							Error::<T>::PositionDoesNotExist,
						range_orders::PositionError::Other(
							range_orders::BurnError::PositionLacksLiquidity,
						) => Error::<T>::PositionLacksLiquidity,
					})?;

				let assets_credited =
					Self::try_credit_both_assets(&lp, unstable_asset, assets_withdrawn)?;
				let collected_fees = Self::try_credit_both_assets(&lp, unstable_asset, fees)?;

				Self::deposit_event(Event::<T>::RangeOrderBurned {
					lp,
					unstable_asset,
					price_range_in_ticks,
					liquidity,
					assets_credited,
					collected_fees,
				});

				Ok(())
			})
		}

		/// Collects and mints a range order.
		///
		/// ## Events
		///
		/// - [On success](Event::RangeOrderMinted)
		/// - [On success](Event::AccountDebited)
		/// - [On success](Event::AccountCredited)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [PoolDoesNotExist](pallet_cf_pools::Error::PoolDoesNotExist)
		/// - [PoolDisabled](pallet_cf_pools::Error::PoolDisabled)
		/// - [InvalidTickRange](pallet_cf_pools::Error::InvalidTickRange)
		/// - [PositionDoesNotExist](pallet_cf_pools::Error::PositionDoesNotExist)
		/// - [MaximumGrossLiquidity](pallet_cf_pools::Error::MaximumGrossLiquidity)
		#[pallet::weight(T::WeightInfo::collect_and_mint_limit_order())]
		pub fn collect_and_mint_limit_order(
			origin: OriginFor<T>,
			unstable_asset: any::Asset,
			order: Order,
			price_as_tick: Tick,
			amount: AssetAmount,
		) -> DispatchResult {
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_pool_state(unstable_asset, |pool_state| {
				let side = Self::order_to_side(order);

				Self::try_debit_single_asset(&lp, unstable_asset, side, amount)?;

				let limit_orders::CollectedAmounts { fees, swapped_liquidity } =
					(match side {
						Side::Zero =>
							cf_amm::limit_orders::PoolState::collect_and_mint::<OneToZero>,
						Side::One => cf_amm::limit_orders::PoolState::collect_and_mint::<ZeroToOne>,
					})(&mut pool_state.limit_orders, &lp, price_as_tick, amount.into())
					.map_err(|e| match e {
						limit_orders::PositionError::InvalidTick => Error::<T>::InvalidTick,
						limit_orders::PositionError::NonExistent =>
							Error::<T>::PositionDoesNotExist,
						limit_orders::PositionError::Other(
							limit_orders::MintError::MaximumLiquidity,
						) => Error::<T>::MaximumGrossLiquidity,
						limit_orders::PositionError::Other(
							limit_orders::MintError::MaximumPoolInstances,
						) => Error::<T>::MaximumPoolInstances,
					})?;

				let collected_fees =
					Self::try_credit_single_asset(&lp, unstable_asset, !side, fees)?;
				let swapped_liquidity =
					Self::try_credit_single_asset(&lp, unstable_asset, !side, swapped_liquidity)?;

				Self::deposit_event(Event::<T>::LimitOrderMinted {
					lp,
					unstable_asset,
					order,
					price_as_tick,
					assets_debited: amount,
					collected_fees,
					swapped_liquidity,
				});

				Ok(())
			})
		}

		/// Collects and burns a range order.
		///
		/// ## Events
		///
		/// - [On success](Event::RangeOrderBurned)
		/// - [On success](Event::AccountDebited)
		/// - [On success](Event::AccountCredited)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [PoolDoesNotExist](pallet_cf_pools::Error::PoolDoesNotExist)
		/// - [PoolDisabled](pallet_cf_pools::Error::PoolDisabled)
		/// - [InvalidTickRange](pallet_cf_pools::Error::InvalidTickRange)
		/// - [PositionDoesNotExist](pallet_cf_pools::Error::PositionDoesNotExist)
		/// - [PositionLacksLiquidity](pallet_cf_pools::Error::PositionLacksLiquidity)
		#[pallet::weight(T::WeightInfo::collect_and_burn_limit_order())]
		pub fn collect_and_burn_limit_order(
			origin: OriginFor<T>,
			unstable_asset: any::Asset,
			order: Order,
			price_as_tick: Tick,
			amount: AssetAmount,
		) -> DispatchResult {
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_pool_state(unstable_asset, |pool_state| {
				let side = Self::order_to_side(order);

				let (assets_credited, limit_orders::CollectedAmounts { fees, swapped_liquidity }) =
					(match side {
						Side::Zero =>
							cf_amm::limit_orders::PoolState::collect_and_burn::<OneToZero>,
						Side::One => cf_amm::limit_orders::PoolState::collect_and_burn::<ZeroToOne>,
					})(&mut pool_state.limit_orders, &lp, price_as_tick, amount.into())
					.map_err(|e| match e {
						limit_orders::PositionError::InvalidTick => Error::<T>::InvalidTick,
						limit_orders::PositionError::NonExistent =>
							Error::<T>::PositionDoesNotExist,
						limit_orders::PositionError::Other(
							limit_orders::BurnError::PositionLacksLiquidity,
						) => Error::<T>::PositionLacksLiquidity,
					})?;

				let collected_fees =
					Self::try_credit_single_asset(&lp, unstable_asset, !side, fees)?;
				let swapped_liquidity =
					Self::try_credit_single_asset(&lp, unstable_asset, !side, swapped_liquidity)?;
				let assets_credited =
					Self::try_credit_single_asset(&lp, unstable_asset, side, assets_credited)?;

				Self::deposit_event(Event::<T>::LimitOrderBurned {
					lp,
					unstable_asset,
					order,
					price_as_tick,
					assets_credited,
					collected_fees,
					swapped_liquidity,
				});

				Ok(())
			})
		}
	}
}

impl<T: Config> SwappingApi for Pallet<T> {
	fn take_network_fee(input: AssetAmount) -> AssetAmount {
		let (remaining, fee) = Self::calculate_network_fee(T::NetworkFee::get(), input);
		CollectedNetworkFee::<T>::mutate(|total| {
			total.saturating_accrue(fee);
		});
		Self::deposit_event(Event::<T>::NetworkFeeTaken { fee_amount: fee });
		remaining
	}

	#[transactional]
	fn swap_single_leg(
		leg: SwapLeg,
		unstable_asset: any::Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		Self::try_mutate_pool_state(unstable_asset, |pool_state| {
			let (from, to, output_amount) = match leg {
				SwapLeg::FromStable => (STABLE_ASSET, unstable_asset, {
					debug_assert_eq!(Self::side_to_asset(unstable_asset, Side::One), STABLE_ASSET);
					let (output_amount, remaining_amount) =
						pool_state.swap::<cf_amm::common::OneToZero>(input_amount.into(), None);
					remaining_amount
						.is_zero()
						.then_some(())
						.ok_or(Error::<T>::InsufficientLiquidity)?;
					output_amount
				}),
				SwapLeg::ToStable => (unstable_asset, STABLE_ASSET, {
					debug_assert_eq!(
						Self::side_to_asset(unstable_asset, Side::Zero),
						unstable_asset
					);
					let (output_amount, remaining_amount) =
						pool_state.swap::<cf_amm::common::ZeroToOne>(input_amount.into(), None);
					remaining_amount
						.is_zero()
						.then_some(())
						.ok_or(Error::<T>::InsufficientLiquidity)?;
					output_amount
				}),
			};
			let output_amount = output_amount.try_into().map_err(|_| Error::<T>::OutputOverflow)?;
			Self::deposit_event(Event::<T>::AssetSwapped { from, to, input_amount, output_amount });
			Ok(output_amount)
		})
	}
}

impl<T: Config> cf_traits::FlipBurnInfo for Pallet<T> {
	fn take_flip_to_burn() -> AssetAmount {
		FlipToBurn::<T>::take()
	}
}

impl<T: Config> Pallet<T> {
	#[transactional]
	pub fn swap_with_network_fee(
		from: any::Asset,
		to: any::Asset,
		input_amount: AssetAmount,
	) -> Result<SwapOutput, DispatchError> {
		Ok(match (from, to) {
			(input_asset, STABLE_ASSET) => Self::take_network_fee(Self::swap_single_leg(
				SwapLeg::ToStable,
				input_asset,
				input_amount,
			)?)
			.into(),
			(STABLE_ASSET, output_asset) => Self::swap_single_leg(
				SwapLeg::FromStable,
				output_asset,
				Self::take_network_fee(input_amount),
			)?
			.into(),
			(input_asset, output_asset) => {
				let intermediate_output =
					Self::swap_single_leg(SwapLeg::ToStable, input_asset, input_amount)?;
				SwapOutput {
					intermediary: Some(intermediate_output),
					output: Self::swap_single_leg(
						SwapLeg::FromStable,
						output_asset,
						Self::take_network_fee(intermediate_output),
					)?,
				}
			},
		})
	}

	fn try_credit_single_asset(
		lp: &T::AccountId,
		unstable_asset: Asset,
		side: Side,
		amount: cf_amm::common::Amount,
	) -> Result<AssetAmount, DispatchError> {
		let assets_credited = amount.try_into()?;
		T::LpBalance::try_credit_account(
			lp,
			Self::side_to_asset(unstable_asset, side),
			assets_credited,
		)?;
		Ok(assets_credited)
	}

	fn try_credit_both_assets(
		lp: &T::AccountId,
		unstable_asset: Asset,
		amounts: SideMap<cf_amm::common::Amount>,
	) -> Result<SideMap<AssetAmount>, DispatchError> {
		amounts
			.try_map(|side, amount| Self::try_credit_single_asset(lp, unstable_asset, side, amount))
	}

	fn try_debit_single_asset(
		lp: &T::AccountId,
		unstable_asset: Asset,
		side: Side,
		amount: AssetAmount,
	) -> DispatchResult {
		T::LpBalance::try_debit_account(lp, Self::side_to_asset(unstable_asset, side), amount)
	}

	fn try_debit_both_assets(
		lp: &T::AccountId,
		unstable_asset: Asset,
		amounts: SideMap<cf_amm::common::Amount>,
	) -> Result<SideMap<AssetAmount>, DispatchError> {
		amounts.try_map(|side, amount| {
			let assets_debited = amount.try_into()?;
			Self::try_debit_single_asset(lp, unstable_asset, side, assets_debited)?;
			Ok(assets_debited)
		})
	}

	fn side_to_asset(unstable_asset: Asset, side: Side) -> Asset {
		match side {
			Side::Zero => unstable_asset,
			Side::One => STABLE_ASSET,
		}
	}

	fn order_to_side(order: Order) -> Side {
		match order {
			Order::Buy => Side::One,
			Order::Sell => Side::Zero,
		}
	}

	fn calculate_network_fee(
		fee_percentage: Permill,
		input: AssetAmount,
	) -> (AssetAmount, AssetAmount) {
		let fee = fee_percentage * input;
		(input - fee, fee)
	}

	fn try_mutate_pool_state<
		R,
		E: From<pallet::Error<T>>,
		F: FnOnce(&mut PoolState<T::AccountId>) -> Result<R, E>,
	>(
		asset: any::Asset,
		f: F,
	) -> Result<R, E> {
		Pools::<T>::try_mutate(asset, |maybe_pool| {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;
			ensure!(pool.enabled, Error::<T>::PoolDisabled);
			f(&mut pool.pool_state)
		})
	}

	pub fn current_sqrt_price(from: Asset, to: Asset) -> Option<SqrtPriceQ64F96> {
		match (from, to) {
			(STABLE_ASSET, unstable_asset) => {
				debug_assert_eq!(Self::side_to_asset(unstable_asset, Side::One), STABLE_ASSET);
				Pools::<T>::get(unstable_asset)
					.and_then(|mut pool| pool.pool_state.current_sqrt_price::<OneToZero>())
			},
			(unstable_asset, STABLE_ASSET) => {
				debug_assert_eq!(Self::side_to_asset(unstable_asset, Side::Zero), unstable_asset);
				Pools::<T>::get(unstable_asset)
					.and_then(|mut pool| pool.pool_state.current_sqrt_price::<ZeroToOne>())
			},
			_ => None,
		}
	}
}
