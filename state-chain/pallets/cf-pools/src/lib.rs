#![cfg_attr(not(feature = "std"), no_std)]
use core::ops::Range;

use cf_amm::{
	common::{Amount, Order, Price, Side, SideMap, Tick},
	limit_orders,
	limit_orders::{Collected, PositionInfo},
	range_orders,
	range_orders::Liquidity,
	PoolState,
};
use cf_primitives::{chains::assets::any, Asset, AssetAmount, SwapOutput, STABLE_ASSET};
use cf_traits::{impl_pallet_safe_mode, Chainflip, LpBalanceApi, PoolApi, SwappingApi};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{Permill, Saturating},
	traits::StorageVersion,
	transactional,
};
use frame_system::pallet_prelude::OriginFor;
use serde::{Deserialize, Serialize};
use sp_arithmetic::traits::Zero;
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

pub use pallet::*;

mod benchmarking;
pub mod migrations;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

impl_pallet_safe_mode!(PalletSafeMode; range_order_update_enabled, limit_order_update_enabled);

/// For referring to either the stable or unstable asset of a particular pool
enum Stability {
	Stable,
	Unstable,
}

// TODO Add custom serialize/deserialize and encode/decode implementations that preserve canonical
// nature.
/// Represents a pair of assets in a canonical ordering, so given two different assets they are
/// always the same way around. In this case the unstable asset is `zero` and the stable is `one`.
#[derive(
	Clone, DebugNoBound, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEqNoBound, EqNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct CanonicalAssetPair<T: Config> {
	assets: cf_amm::common::SideMap<Asset>,
	#[doc(hidden)]
	#[codec(skip)]
	_phantom: core::marker::PhantomData<T>,
}
impl<T: Config> Copy for CanonicalAssetPair<T> {}
impl<T: Config> CanonicalAssetPair<T> {
	pub fn new(base_asset: Asset, pair_asset: Asset) -> Result<Self, Error<T>> {
		match (base_asset, pair_asset) {
			(STABLE_ASSET, STABLE_ASSET) => Err(Error::<T>::PoolDoesNotExist),
			(STABLE_ASSET, unstable_asset) | (unstable_asset, STABLE_ASSET) => Ok(Self {
				assets: cf_amm::common::SideMap::<()>::default().map(|side, _| {
					match Self::side_to_stability(side) {
						Stability::Stable => STABLE_ASSET,
						Stability::Unstable => unstable_asset,
					}
				}),
				_phantom: Default::default(),
			}),
			_ => Err(Error::<T>::PoolDoesNotExist),
		}
	}

	fn side_to_asset(&self, side: Side) -> Asset {
		self.assets[side]
	}

	/// !!! Must match side_to_stability !!!
	fn stability_to_side(stability: Stability) -> Side {
		match stability {
			Stability::Stable => Side::One,
			Stability::Unstable => Side::Zero,
		}
	}

	/// !!! Must match stability_to_side !!!
	fn side_to_stability(side: Side) -> Stability {
		match side {
			Side::Zero => Stability::Unstable,
			Side::One => Stability::Stable,
		}
	}
}

pub struct AssetPair<T: Config> {
	canonical_asset_pair: CanonicalAssetPair<T>,
	base_side: Side,
}
impl<T: Config> AssetPair<T> {
	pub fn new(base_asset: Asset, pair_asset: Asset) -> Result<Self, Error<T>> {
		Ok(Self {
			canonical_asset_pair: CanonicalAssetPair::new(base_asset, pair_asset)?,
			base_side: CanonicalAssetPair::<T>::stability_to_side(match (base_asset, pair_asset) {
				(STABLE_ASSET, STABLE_ASSET) => Err(Error::<T>::PoolDoesNotExist),
				(STABLE_ASSET, _unstable_asset) => Ok(Stability::Stable),
				(_unstable_asset, STABLE_ASSET) => Ok(Stability::Unstable),
				_ => Err(Error::<T>::PoolDoesNotExist),
			}?),
		})
	}

	/// Remaps the amounts into a SideMap, assuming the base and pair are the same way around as the
	/// assets when this AssetPair was created.
	pub fn asset_amounts_to_side_map(
		&self,
		asset_amounts: AssetAmounts,
	) -> cf_amm::common::SideMap<cf_amm::common::Amount> {
		cf_amm::common::SideMap::from_array(match self.base_side {
			Side::Zero => [asset_amounts.base.into(), asset_amounts.pair.into()],
			Side::One => [asset_amounts.pair.into(), asset_amounts.base.into()],
		})
	}

	/// Remaps the amounts into an AssetsMap, assuming the base and pair should be the same way
	/// around as the assets when this AssetPair was created.
	pub fn side_map_to_asset_amounts(
		&self,
		side_map: cf_amm::common::SideMap<cf_amm::common::Amount>,
	) -> Result<AssetAmounts, <cf_amm::common::Amount as TryInto<AssetAmount>>::Error> {
		Ok(self.side_map_to_assets_map(side_map.try_map(|_, amount| amount.try_into())?))
	}

	/// Remaps a SideMap into an AssetsMap, assuming the base and pair should be the same way around
	/// as the assets when this AssetPair was created.
	pub fn side_map_to_assets_map<R>(&self, side_map: cf_amm::common::SideMap<R>) -> AssetsMap<R> {
		match self.base_side {
			Side::Zero => AssetsMap { base: side_map.zero, pair: side_map.one },
			Side::One => AssetsMap { base: side_map.one, pair: side_map.zero },
		}
	}

	fn try_xxx_assets<F: Fn(&T::AccountId, Asset, AssetAmount) -> DispatchResult>(
		&self,
		lp: &T::AccountId,
		side_map: cf_amm::common::SideMap<cf_amm::common::Amount>,
		f: F,
	) -> Result<AssetAmounts, DispatchError> {
		self.side_map_to_asset_amounts(side_map)?
			.try_map_with_asset(self, |asset, asset_amount| {
				f(lp, asset, asset_amount).map(|_| asset_amount)
			})
	}

	/// Debits the specified amounts, and returns the amounts debited. If the requested amounts
	/// couldn't be debited this is a noop and returns an error.
	fn try_debit_assets(
		&self,
		lp: &T::AccountId,
		side_map: cf_amm::common::SideMap<cf_amm::common::Amount>,
	) -> Result<AssetAmounts, DispatchError> {
		self.try_xxx_assets(lp, side_map, T::LpBalance::try_debit_account)
	}

	/// Credits the specified amounts, and returns the amounts credited. If the requested amounts
	/// couldn't be credited this is a noop and returns an error.
	fn try_credit_assets(
		&self,
		lp: &T::AccountId,
		side_map: cf_amm::common::SideMap<cf_amm::common::Amount>,
	) -> Result<AssetAmounts, DispatchError> {
		self.try_xxx_assets(lp, side_map, T::LpBalance::try_credit_account)
	}

	fn try_xxx_asset<F: FnOnce(&T::AccountId, Asset, AssetAmount) -> DispatchResult>(
		&self,
		lp: &T::AccountId,
		side: Side,
		amount: cf_amm::common::Amount,
		f: F,
	) -> Result<AssetAmount, DispatchError> {
		let asset_amount: AssetAmount = amount.try_into()?;
		f(lp, self.canonical_asset_pair.side_to_asset(side), asset_amount)?;
		Ok(asset_amount)
	}

	/// Debits the specified amount, and returns the amount debited. If the requested amount
	/// couldn't be debited this is a noop and returns an error.
	fn try_debit_asset(
		&self,
		lp: &T::AccountId,
		side: Side,
		amount: cf_amm::common::Amount,
	) -> Result<AssetAmount, DispatchError> {
		self.try_xxx_asset(lp, side, amount, T::LpBalance::try_debit_account)
	}

	/// Credits the specified amount, and returns the amount credited. If the requested amount
	/// couldn't be credited this is a noop and returns an error.
	fn try_credit_asset(
		&self,
		lp: &T::AccountId,
		side: Side,
		amount: cf_amm::common::Amount,
	) -> Result<AssetAmount, DispatchError> {
		self.try_xxx_asset(lp, side, amount, T::LpBalance::try_credit_account)
	}
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use cf_amm::{
		common::Tick,
		limit_orders,
		range_orders::{self, Liquidity},
		NewError,
	};
	use cf_traits::{AccountRoleRegistry, LpBalanceApi};
	use frame_system::pallet_prelude::BlockNumberFor;
	use sp_std::collections::btree_map::BTreeMap;

	use super::*;

	#[derive(Clone, Debug, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct Pool<T: Config> {
		/// A cache of all the range orders that exist in the pool. This must be kept up to date
		/// with the underlying pool.
		pub range_orders_cache: BTreeMap<T::AccountId, BTreeMap<OrderId, Range<Tick>>>,
		/// A cache of all the limit orders that exist in the pool. This must be kept up to date
		/// with the underlying pool. These are grouped by the asset the limit order is selling
		pub limit_orders_cache: SideMap<BTreeMap<T::AccountId, BTreeMap<OrderId, Tick>>>,
		pub pool_state: PoolState<(T::AccountId, OrderId)>,
	}

	pub type OrderId = u64;

	#[derive(
		Copy,
		Clone,
		Debug,
		Default,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		PartialEq,
		Eq,
		Deserialize,
		Serialize,
	)]
	pub struct AssetsMap<S> {
		pub base: S,
		pub pair: S,
	}
	impl<S> AssetsMap<S> {
		pub fn try_map<R, E, F: FnMut(S) -> Result<R, E>>(
			self,
			mut f: F,
		) -> Result<AssetsMap<R>, E> {
			Ok(AssetsMap { base: f(self.base)?, pair: f(self.pair)? })
		}

		pub fn map<R, F: FnMut(S) -> R>(self, mut f: F) -> AssetsMap<R> {
			AssetsMap { base: f(self.base), pair: f(self.pair) }
		}

		pub fn map_with_side<T: Config, R, F: FnMut(Side, S) -> R>(
			self,
			asset_pair: &AssetPair<T>,
			mut f: F,
		) -> AssetsMap<R> {
			AssetsMap {
				base: f(asset_pair.base_side, self.base),
				pair: f(!asset_pair.base_side, self.pair),
			}
		}

		pub fn try_map_with_asset<T: Config, R, E, F: FnMut(Asset, S) -> Result<R, E>>(
			self,
			asset_pair: &AssetPair<T>,
			mut f: F,
		) -> Result<AssetsMap<R>, E> {
			Ok(AssetsMap {
				base: f(
					asset_pair.canonical_asset_pair.side_to_asset(asset_pair.base_side),
					self.base,
				)?,
				pair: f(
					asset_pair.canonical_asset_pair.side_to_asset(!asset_pair.base_side),
					self.pair,
				)?,
			})
		}
	}

	pub type AssetAmounts = AssetsMap<AssetAmount>;

	/// Represents an amount of liquidity, either as an exact amount, or through maximum and minimum
	/// amounts of both assets. Internally those max/min are converted into exact liquidity amounts,
	/// that is if the appropriate asset ratio can be achieved while maintaining the max/min bounds.
	#[derive(
		Copy,
		Clone,
		Debug,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		PartialEq,
		Eq,
		Deserialize,
		Serialize,
	)]
	pub enum RangeOrderSize {
		AssetAmounts { maximum: AssetAmounts, minimum: AssetAmounts },
		Liquidity { liquidity: Liquidity },
	}

	/// Indicates the change caused by an operation in the positions size, both in terms of
	/// liquidity and equivalently in asset amounts
	#[derive(
		Copy,
		Clone,
		Debug,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		PartialEq,
		Eq,
		Deserialize,
		Serialize,
	)]
	pub struct RangeOrderChange {
		pub liquidity: Liquidity,
		pub amounts: AssetsMap<AssetAmount>,
	}

	/// Indicates if an LP wishes to increase or decrease the size of an order.
	#[derive(
		Copy,
		Clone,
		Debug,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		PartialEq,
		Eq,
		Deserialize,
		Serialize,
	)]
	#[serde(rename_all = "snake_case")]
	pub enum IncreaseOrDecrease<T> {
		Increase(T),
		Decrease(T),
	}

	impl<T> IncreaseOrDecrease<T> {
		pub fn abs(&self) -> &T {
			match self {
				IncreaseOrDecrease::Increase(t) => t,
				IncreaseOrDecrease::Decrease(t) => t,
			}
		}

		pub fn map<R, F: FnOnce(T) -> R>(self, f: F) -> IncreaseOrDecrease<R> {
			match self {
				IncreaseOrDecrease::Increase(t) => IncreaseOrDecrease::Increase(f(t)),
				IncreaseOrDecrease::Decrease(t) => IncreaseOrDecrease::Decrease(f(t)),
			}
		}

		pub fn try_map<R, E, F: FnOnce(T) -> Result<R, E>>(
			self,
			f: F,
		) -> Result<IncreaseOrDecrease<R>, E> {
			Ok(match self {
				IncreaseOrDecrease::Increase(t) => IncreaseOrDecrease::Increase(f(t)?),
				IncreaseOrDecrease::Decrease(t) => IncreaseOrDecrease::Decrease(f(t)?),
			})
		}
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

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(PALLET_VERSION)]
	pub struct Pallet<T>(PhantomData<T>);

	/// All the available pools.
	#[pallet::storage]
	pub type Pools<T: Config> =
		StorageMap<_, Twox64Concat, CanonicalAssetPair<T>, Pool<T>, OptionQuery>;

	/// FLIP ready to be burned.
	#[pallet::storage]
	pub(super) type FlipToBurn<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	/// Interval at which we buy FLIP in order to burn it.
	#[pallet::storage]
	pub(super) type FlipBuyInterval<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Network fees, in USDC terms, that have been collected and are ready to be converted to FLIP.
	#[pallet::storage]
	pub type CollectedNetworkFee<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub flip_buy_interval: BlockNumberFor<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			FlipBuyInterval::<T>::set(self.flip_buy_interval);
		}
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { flip_buy_interval: BlockNumberFor::<T>::zero() }
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
							any::Asset::Usdc,
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
		/// For previously unused order ids, you must specific a tick/tick range for the order,
		/// thereby specifying the order price associated with that order id
		UnspecifiedOrderPrice,
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
		/// The user's order does not exist.
		OrderDoesNotExist,
		/// It is no longer possible to mint limit orders due to reaching the maximum pool
		/// instances, other than for ticks where a fixed pool currently exists.
		MaximumPoolInstances,
		/// The pool does not have enough liquidity left to process the swap.
		InsufficientLiquidity,
		/// The swap output is past the maximum allowed amount.
		OutputOverflow,
		/// There are no amounts between the specified maximum and minimum that match the required
		/// ratio of assets
		AssetRatioUnachieveable,
		/// Updating Limit Orders is disabled
		UpdatingLimitOrdersDisabled,
		/// Updating Range Orders is disabled
		UpdatingRangeOrdersDisabled,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		UpdatedBuyInterval {
			buy_interval: BlockNumberFor<T>,
		},
		NewPoolCreated {
			base_asset: Asset,
			pair_asset: Asset,
			fee_hundredth_pips: u32,
			initial_price: Price,
		},
		/// Indicates the details of a change made to a range order. A single update extrinsic may
		/// produce multiple of these events, particularly for example if the update changes the
		/// price/range of the order.
		RangeOrderUpdated {
			lp: T::AccountId,
			base_asset: Asset,
			pair_asset: Asset,
			id: OrderId,
			tick_range: core::ops::Range<Tick>,
			size_change: Option<IncreaseOrDecrease<RangeOrderChange>>,
			liquidity_total: Liquidity,
			collected_fees: AssetAmounts,
		},
		/// Indicates the details of a change made to a limit order. A single update extrinsic may
		/// produce multiple of these events, particularly for example if the update changes the
		/// price of the order.
		LimitOrderUpdated {
			lp: T::AccountId,
			sell_asset: Asset,
			buy_asset: Asset,
			id: OrderId,
			tick: Tick,
			amount_change: Option<IncreaseOrDecrease<AssetAmount>>,
			amount_total: AssetAmount,
			collected_fees: AssetAmount,
			bought_amount: AssetAmount,
		},
		NetworkFeeTaken {
			fee_amount: AssetAmount,
		},
		AssetSwapped {
			from: Asset,
			to: Asset,
			input_amount: AssetAmount,
			output_amount: AssetAmount,
		},
		PoolFeeSet {
			base_asset: Asset,
			pair_asset: Asset,
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
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::update_buy_interval())]
		pub fn update_buy_interval(
			origin: OriginFor<T>,
			new_buy_interval: BlockNumberFor<T>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(new_buy_interval != Zero::zero(), Error::<T>::ZeroBuyIntervalNotAllowed);
			FlipBuyInterval::<T>::set(new_buy_interval);
			Self::deposit_event(Event::<T>::UpdatedBuyInterval { buy_interval: new_buy_interval });
			Ok(())
		}

		/// Create a new pool.
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
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::new_pool())]
		pub fn new_pool(
			origin: OriginFor<T>,
			base_asset: any::Asset,
			pair_asset: any::Asset,
			fee_hundredth_pips: u32,
			initial_price: Price,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			let canonical_asset_pair = CanonicalAssetPair::<T>::new(base_asset, pair_asset)?;
			Pools::<T>::try_mutate(canonical_asset_pair, |maybe_pool| {
				ensure!(maybe_pool.is_none(), Error::<T>::PoolAlreadyExists);

				*maybe_pool = Some(Pool {
					range_orders_cache: Default::default(),
					limit_orders_cache: Default::default(),
					pool_state: PoolState::new(fee_hundredth_pips, initial_price).map_err(|e| {
						match e {
							NewError::LimitOrders(limit_orders::NewError::InvalidFeeAmount) =>
								Error::<T>::InvalidFeeAmount,
							NewError::RangeOrders(range_orders::NewError::InvalidFeeAmount) =>
								Error::<T>::InvalidFeeAmount,
							NewError::RangeOrders(range_orders::NewError::InvalidInitialPrice) =>
								Error::<T>::InvalidInitialPrice,
						}
					})?,
				});

				Ok::<_, Error<T>>(())
			})?;

			Self::deposit_event(Event::<T>::NewPoolCreated {
				base_asset,
				pair_asset,
				fee_hundredth_pips,
				initial_price,
			});

			Ok(())
		}

		/// Optionally move the order to a different range and then increase or decrease its amount
		/// of liquidity. As different ranges may require different ratios of assets, when
		/// optionally moving the order it may not be possible to allocate all the assets previously
		/// associated with the order to the new range; If so the unused assets will be returned to
		/// your balance. The appropriate assets will be debited or credited from your balance as
		/// needed. If the order_id isn't being used at the moment you must specify a tick_range,
		/// otherwise it will not know what range you want the order to be over.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::update_range_order())]
		pub fn update_range_order(
			origin: OriginFor<T>,
			base_asset: Asset,
			pair_asset: Asset,
			id: OrderId,
			option_tick_range: Option<core::ops::Range<Tick>>,
			size_change: IncreaseOrDecrease<RangeOrderSize>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().range_order_update_enabled,
				Error::<T>::UpdatingRangeOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_order(&lp, base_asset, pair_asset, |asset_pair, pool| {
				let tick_range = match (
					pool.range_orders_cache
						.get(&lp)
						.and_then(|range_orders| range_orders.get(&id))
						.cloned(),
					option_tick_range,
				) {
					(None, None) => Err(Error::<T>::UnspecifiedOrderPrice),
					(None, Some(tick_range)) | (Some(tick_range), None) => Ok(tick_range),
					(Some(previous_tick_range), Some(new_tick_range)) => {
						if previous_tick_range != new_tick_range {
							let withdrawn_asset_amounts = Self::inner_update_range_order(
								pool,
								&lp,
								asset_pair,
								id,
								previous_tick_range,
								IncreaseOrDecrease::Decrease(range_orders::Size::Liquidity {
									liquidity: Liquidity::MAX,
								}),
								/* allow_noop */ false,
							)?;
							Self::inner_update_range_order(
								pool,
								&lp,
								asset_pair,
								id,
								new_tick_range.clone(),
								IncreaseOrDecrease::Increase(range_orders::Size::Amount {
									minimum: Default::default(),
									maximum: asset_pair
										.asset_amounts_to_side_map(withdrawn_asset_amounts),
								}),
								/* allow_noop */ true,
							)?;
						}

						Ok(new_tick_range)
					},
				}?;
				Self::inner_update_range_order(
					pool,
					&lp,
					asset_pair,
					id,
					tick_range,
					size_change.map(|size| match size {
						RangeOrderSize::Liquidity { liquidity } =>
							range_orders::Size::Liquidity { liquidity },
						RangeOrderSize::AssetAmounts { maximum, minimum } =>
							range_orders::Size::Amount {
								maximum: asset_pair.asset_amounts_to_side_map(maximum),
								minimum: asset_pair.asset_amounts_to_side_map(minimum),
							},
					}),
					/* allow_noop */ false,
				)?;

				Ok(())
			})
		}

		/// Optionally move the order to a different range and then set its amount of liquidity. The
		/// appropriate assets will be debited or credited from your balance as needed. If the
		/// order_id isn't being used at the moment you must specify a tick_range, otherwise it will
		/// not know what range you want the order to be over.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::set_range_order())]
		pub fn set_range_order(
			origin: OriginFor<T>,
			base_asset: Asset,
			pair_asset: Asset,
			id: OrderId,
			option_tick_range: Option<core::ops::Range<Tick>>,
			size: RangeOrderSize,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().range_order_update_enabled,
				Error::<T>::UpdatingRangeOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_order(&lp, base_asset, pair_asset, |asset_pair, pool| {
				let tick_range = match (
					pool.range_orders_cache
						.get(&lp)
						.and_then(|range_orders| range_orders.get(&id))
						.cloned(),
					option_tick_range,
				) {
					(None, None) => Err(Error::<T>::UnspecifiedOrderPrice),
					(None, Some(tick_range)) => Ok(tick_range),
					(Some(previous_tick_range), option_new_tick_range) => {
						Self::inner_update_range_order(
							pool,
							&lp,
							asset_pair,
							id,
							previous_tick_range.clone(),
							IncreaseOrDecrease::Decrease(range_orders::Size::Liquidity {
								liquidity: Liquidity::MAX,
							}),
							/* allow noop */ false,
						)?;

						Ok(option_new_tick_range.unwrap_or(previous_tick_range))
					},
				}?;
				Self::inner_update_range_order(
					pool,
					&lp,
					asset_pair,
					id,
					tick_range,
					IncreaseOrDecrease::Increase(match size {
						RangeOrderSize::Liquidity { liquidity } =>
							range_orders::Size::Liquidity { liquidity },
						RangeOrderSize::AssetAmounts { maximum, minimum } =>
							range_orders::Size::Amount {
								maximum: asset_pair.asset_amounts_to_side_map(maximum),
								minimum: asset_pair.asset_amounts_to_side_map(minimum),
							},
					}),
					/* allow noop */ true,
				)?;

				Ok(())
			})
		}

		/// Optionally move the order to a different tick and then increase or decrease its amount
		/// of liquidity. The appropriate assets will be debited or credited from your balance as
		/// needed. If the order_id isn't being used at the moment you must specify a tick,
		/// otherwise it will not know what tick you want the order to be over. Note limit order
		/// order_id's are independent of range order order_id's. In addition to that, order_id's
		/// for buy and sell limit orders i.e. those in different directions are independent.
		/// Therefore you may have two limit orders with the same order_id in the same pool, one to
		/// buy Eth and one to sell Eth for example.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::update_limit_order())]
		pub fn update_limit_order(
			origin: OriginFor<T>,
			sell_asset: any::Asset,
			buy_asset: any::Asset,
			id: OrderId,
			option_tick: Option<Tick>,
			amount_change: IncreaseOrDecrease<AssetAmount>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().limit_order_update_enabled,
				Error::<T>::UpdatingLimitOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_order(&lp, sell_asset, buy_asset, |asset_pair, pool| {
				let tick = match (
					pool.limit_orders_cache[asset_pair.base_side]
						.get(&lp)
						.and_then(|limit_orders| limit_orders.get(&id))
						.cloned(),
					option_tick,
				) {
					(None, None) => Err(Error::<T>::UnspecifiedOrderPrice),
					(None, Some(tick)) | (Some(tick), None) => Ok(tick),
					(Some(previous_tick), Some(new_tick)) => {
						if previous_tick != new_tick {
							let withdrawn_asset_amount = Self::inner_update_limit_order(
								pool,
								&lp,
								asset_pair,
								id,
								previous_tick,
								IncreaseOrDecrease::Decrease(cf_amm::common::Amount::MAX),
								/* allow_noop */ false,
							)?;
							Self::inner_update_limit_order(
								pool,
								&lp,
								asset_pair,
								id,
								new_tick,
								IncreaseOrDecrease::Increase(withdrawn_asset_amount.into()),
								/* allow_noop */ true,
							)?;
						}

						Ok(new_tick)
					},
				}?;
				Self::inner_update_limit_order(
					pool,
					&lp,
					asset_pair,
					id,
					tick,
					amount_change.map(|amount| amount.into()),
					/* allow_noop */ false,
				)?;

				Ok(())
			})
		}

		/// Optionally move the order to a different tick and then set its amount of liquidity. The
		/// appropriate assets will be debited or credited from your balance as needed. If the
		/// order_id isn't being used at the moment you must specify a tick, otherwise it will not
		/// know what tick you want the order to be over. Note limit order order_id's are
		/// independent of range order order_id's. In addition to that, order_id's for buy and sell
		/// limit orders i.e. those in different directions are independent. Therefore you may have
		/// two limit orders with the same order_id in the same pool, one to buy Eth and one to sell
		/// Eth for example.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_limit_order())]
		pub fn set_limit_order(
			origin: OriginFor<T>,
			sell_asset: any::Asset,
			buy_asset: any::Asset,
			id: OrderId,
			option_tick: Option<Tick>,
			sell_amount: AssetAmount,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().limit_order_update_enabled,
				Error::<T>::UpdatingLimitOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_order(&lp, sell_asset, buy_asset, |asset_pair, pool| {
				let tick = match (
					pool.limit_orders_cache[asset_pair.base_side]
						.get(&lp)
						.and_then(|limit_orders| limit_orders.get(&id))
						.cloned(),
					option_tick,
				) {
					(None, None) => Err(Error::<T>::UnspecifiedOrderPrice),
					(None, Some(tick)) => Ok(tick),
					(Some(previous_tick), option_new_tick) => {
						Self::inner_update_limit_order(
							pool,
							&lp,
							asset_pair,
							id,
							previous_tick,
							IncreaseOrDecrease::Decrease(cf_amm::common::Amount::MAX),
							/* allow noop */ false,
						)?;

						Ok(option_new_tick.unwrap_or(previous_tick))
					},
				}?;
				Self::inner_update_limit_order(
					pool,
					&lp,
					asset_pair,
					id,
					tick,
					IncreaseOrDecrease::Increase(sell_amount.into()),
					/* allow noop */ true,
				)?;

				Ok(())
			})
		}

		/// Sets the Liquidity Pool fees. Also collect earned fees and bought amount for
		/// all positions within the fee and accredit them to the liquidity provider.
		/// Requires governance origin.
		///
		/// ## Events
		///
		/// - [On success](Event::PoolFeeSet)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [InvalidFeeAmount](pallet_cf_pools::Error::InvalidFeeAmount)
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::set_pool_fees())]
		pub fn set_pool_fees(
			origin: OriginFor<T>,
			base_asset: Asset,
			pair_asset: Asset,
			fee_hundredth_pips: u32,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				PoolState::<(T::AccountId, OrderId)>::validate_fees(fee_hundredth_pips),
				Error::<T>::InvalidFeeAmount
			);
			Self::try_mutate_pool(
				base_asset,
				pair_asset,
				|_| Ok(()),
				|asset_pair: &AssetPair<T>, pool| {
					pool.pool_state
						.set_fees(fee_hundredth_pips)
						.map_err(|_| Error::<T>::InvalidFeeAmount)?
						.try_map(|side, collected_fees| {
							for ((tick, (lp, order)), (collected, position_info)) in
								collected_fees.into_iter()
							{
								Self::process_limit_order_update(
									pool,
									asset_pair,
									&lp,
									side,
									order,
									tick,
									collected,
									position_info,
									IncreaseOrDecrease::Increase(0),
								)?;
							}
							Result::<(), DispatchError>::Ok(())
						})
				},
			)?;

			Self::deposit_event(Event::<T>::PoolFeeSet {
				base_asset,
				pair_asset,
				fee_hundredth_pips,
			});

			Ok(())
		}
	}
}

impl<T: Config> SwappingApi for Pallet<T> {
	fn take_network_fee(input: AssetAmount) -> AssetAmount {
		if input.is_zero() {
			return input
		}
		let (remaining, fee) = utilities::calculate_network_fee(T::NetworkFee::get(), input);
		CollectedNetworkFee::<T>::mutate(|total| {
			total.saturating_accrue(fee);
		});
		Self::deposit_event(Event::<T>::NetworkFeeTaken { fee_amount: fee });
		remaining
	}

	#[transactional]
	fn swap_single_leg(
		from: any::Asset,
		to: any::Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		Self::try_mutate_pool(
			from,
			to,
			|_| Ok(()),
			|asset_pair, pool| {
				let (output_amount, remaining_amount) =
					pool.pool_state.swap(asset_pair.base_side, Order::Sell, input_amount.into());
				remaining_amount
					.is_zero()
					.then_some(())
					.ok_or(Error::<T>::InsufficientLiquidity)?;
				let output_amount =
					output_amount.try_into().map_err(|_| Error::<T>::OutputOverflow)?;
				Self::deposit_event(Event::<T>::AssetSwapped {
					from,
					to,
					input_amount,
					output_amount,
				});
				Ok(output_amount)
			},
		)
	}
}

impl<T: Config> PoolApi for Pallet<T> {
	type AccountId = T::AccountId;

	fn sweep(who: &T::AccountId) -> DispatchResult {
		Self::inner_sweep(who, Side::Zero)
	}
}

impl<T: Config> cf_traits::FlipBurnInfo for Pallet<T> {
	fn take_flip_to_burn() -> AssetAmount {
		FlipToBurn::<T>::take()
	}
}

#[derive(
	Copy,
	Clone,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Deserialize,
	Serialize,
)]
pub struct PoolInfo {
	/// The fee taken, when limit orders are used, from swap inputs that contributes to liquidity
	/// provider earnings
	pub limit_order_fee_hundredth_pips: u32,
	/// The fee taken, when range orders are used, from swap inputs that contributes to liquidity
	/// provider earnings
	pub range_order_fee_hundredth_pips: u32,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolOrders {
	/// Limit orders are groups by which asset they are selling.
	pub limit_orders: AssetsMap<Vec<(OrderId, Tick, Amount)>>,
	/// Range orders can be both buy and/or sell therefore they not split. The current range order
	/// price determines if they are buy and/or sell.
	pub range_orders: Vec<(OrderId, Range<Tick>, Liquidity)>,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolLiquidity {
	/// An ordered lists of the amount of assets available at each tick, if a tick contains zero
	/// liquidity it will not be included in the list. Note limit order liquidity is split by which
	/// asset the liquidity is "selling".
	pub limit_orders: AssetsMap<Vec<(Tick, Amount)>>,
	/// An ordered list of the amount of range order liquidity available from a tick until the next
	/// tick in the list. Note range orders can be both buy and/or sell therefore they not split by
	/// sold asset. The current range order price determines if the liquidity can be used for
	/// buying and/or selling,
	pub range_orders: Vec<(Tick, Liquidity)>,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct UnidirectionalSubPoolDepth {
	/// The current price in this sub pool, in the given direction of swaps.
	pub price: Option<Price>,
	/// The approximate amount of assets available to be sold in the specified price range.
	pub depth: Amount,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct UnidirectionalPoolDepth {
	/// The depth of the limit order pool.
	pub limit_orders: UnidirectionalSubPoolDepth,
	/// The depth of the range order pool.
	pub range_orders: UnidirectionalSubPoolDepth,
}

impl<T: Config> Pallet<T> {
	fn inner_sweep(lp: &T::AccountId, range_order_base_side: Side) -> DispatchResult {
		// Collect to avoid undefined behaviour (See StorsgeMap::iter_keys documentation)
		for canonical_asset_pair in Pools::<T>::iter_keys().collect::<Vec<_>>() {
			let mut pool = Pools::<T>::get(canonical_asset_pair).unwrap();

			if let Some(range_orders_cache) = pool.range_orders_cache.get(lp).cloned() {
				let asset_pair =
					AssetPair { canonical_asset_pair, base_side: range_order_base_side };

				for (id, range) in range_orders_cache.iter() {
					Self::inner_update_range_order(
						&mut pool,
						lp,
						&asset_pair,
						*id,
						range.clone(),
						IncreaseOrDecrease::Decrease(range_orders::Size::Liquidity {
							liquidity: 0,
						}),
						false,
					)?;
				}
			}

			for (side, limit_orders_cache) in pool
				.limit_orders_cache
				.as_ref()
				.into_iter()
				.filter_map(|(side, limit_orders_cache)| {
					limit_orders_cache
						.get(lp)
						.cloned()
						.map(|limit_orders_cache| (side, limit_orders_cache))
				})
				.collect::<Vec<_>>()
			{
				let asset_pair = AssetPair { canonical_asset_pair, base_side: side };

				for (id, tick) in limit_orders_cache {
					Self::inner_update_limit_order(
						&mut pool,
						lp,
						&asset_pair,
						id,
						tick,
						IncreaseOrDecrease::Decrease(Default::default()),
						false,
					)?;
				}
			}

			Pools::<T>::insert(canonical_asset_pair, pool);
		}

		Ok(())
	}

	#[allow(clippy::too_many_arguments)]
	fn inner_update_limit_order(
		pool: &mut Pool<T>,
		lp: &T::AccountId,
		asset_pair: &AssetPair<T>,
		id: OrderId,
		tick: cf_amm::common::Tick,
		amount_change: IncreaseOrDecrease<cf_amm::common::Amount>,
		allow_noop: bool,
	) -> Result<AssetAmount, DispatchError> {
		let (amount_change, position_info, collected) = match amount_change {
			IncreaseOrDecrease::Increase(amount) => {
				let (collected, position_info) = match pool.pool_state.collect_and_mint_limit_order(
					&(lp.clone(), id),
					asset_pair.base_side,
					Order::Sell,
					tick,
					amount,
				) {
					Ok(ok) => Ok(ok),
					Err(error) => Err(match error {
						limit_orders::PositionError::NonExistent =>
							if allow_noop {
								return Ok(Default::default())
							} else {
								Error::<T>::OrderDoesNotExist
							},
						limit_orders::PositionError::InvalidTick => Error::<T>::InvalidTick,
						limit_orders::PositionError::Other(
							limit_orders::MintError::MaximumLiquidity,
						) => Error::<T>::MaximumGrossLiquidity,
						limit_orders::PositionError::Other(
							limit_orders::MintError::MaximumPoolInstances,
						) => Error::<T>::AssetRatioUnachieveable,
					}),
				}?;

				let debited_asset_amount =
					asset_pair.try_debit_asset(lp, asset_pair.base_side, amount)?;

				(IncreaseOrDecrease::Increase(debited_asset_amount), position_info, collected)
			},
			IncreaseOrDecrease::Decrease(amount) => {
				let (withdrawn_amount, collected, position_info) =
					match pool.pool_state.collect_and_burn_limit_order(
						&(lp.clone(), id),
						asset_pair.base_side,
						Order::Sell,
						tick,
						amount,
					) {
						Ok(ok) => Ok(ok),
						Err(error) => Err(match error {
							limit_orders::PositionError::NonExistent =>
								if allow_noop {
									return Ok(Default::default())
								} else {
									Error::<T>::OrderDoesNotExist
								},
							limit_orders::PositionError::InvalidTick => Error::InvalidTick,
							limit_orders::PositionError::Other(error) => match error {},
						}),
					}?;

				let withdrawn_asset_amount =
					asset_pair.try_credit_asset(lp, asset_pair.base_side, withdrawn_amount)?;

				(IncreaseOrDecrease::Decrease(withdrawn_asset_amount), position_info, collected)
			},
		};

		// Process the update
		Self::process_limit_order_update(
			pool,
			asset_pair,
			lp,
			asset_pair.base_side,
			id,
			tick,
			collected,
			position_info,
			amount_change,
		)?;

		Ok(*amount_change.abs())
	}

	#[allow(clippy::too_many_arguments)]
	fn inner_update_range_order(
		pool: &mut Pool<T>,
		lp: &T::AccountId,
		asset_pair: &AssetPair<T>,
		id: OrderId,
		tick_range: Range<cf_amm::common::Tick>,
		size_change: IncreaseOrDecrease<range_orders::Size>,
		allow_noop: bool,
	) -> Result<AssetAmounts, DispatchError> {
		let (liquidity_change, position_info, assets_change, collected) = match size_change {
			IncreaseOrDecrease::Increase(size) => {
				let (assets_debited, minted_liquidity, collected, position_info) =
					match pool.pool_state.collect_and_mint_range_order(
						&(lp.clone(), id),
						tick_range.clone(),
						size,
						|required_amounts| asset_pair.try_debit_assets(lp, required_amounts),
					) {
						Ok(ok) => Ok(ok),
						Err(error) => Err(match error {
							range_orders::PositionError::InvalidTickRange =>
								Error::<T>::InvalidTickRange.into(),
							range_orders::PositionError::NonExistent =>
								if allow_noop {
									return Ok(Default::default())
								} else {
									Error::<T>::OrderDoesNotExist.into()
								},
							range_orders::PositionError::Other(
								range_orders::MintError::CallbackFailed(e),
							) => e,
							range_orders::PositionError::Other(
								range_orders::MintError::MaximumGrossLiquidity,
							) => Error::<T>::MaximumGrossLiquidity.into(),
							range_orders::PositionError::Other(
								cf_amm::range_orders::MintError::AssetRatioUnachieveable,
							) => Error::<T>::AssetRatioUnachieveable.into(),
						}),
					}?;

				(
					IncreaseOrDecrease::Increase(minted_liquidity),
					position_info,
					assets_debited,
					collected,
				)
			},
			IncreaseOrDecrease::Decrease(size) => {
				let (assets_withdrawn, burnt_liquidity, collected, position_info) = match pool
					.pool_state
					.collect_and_burn_range_order(&(lp.clone(), id), tick_range.clone(), size)
				{
					Ok(ok) => Ok(ok),
					Err(error) => Err(match error {
						range_orders::PositionError::InvalidTickRange =>
							Error::<T>::InvalidTickRange,
						range_orders::PositionError::NonExistent =>
							if allow_noop {
								return Ok(Default::default())
							} else {
								Error::<T>::OrderDoesNotExist
							},
						range_orders::PositionError::Other(e) => match e {
							range_orders::BurnError::AssetRatioUnachieveable =>
								Error::<T>::AssetRatioUnachieveable,
						},
					}),
				}?;

				let assets_withdrawn = asset_pair.try_credit_assets(lp, assets_withdrawn)?;

				(
					IncreaseOrDecrease::Decrease(burnt_liquidity),
					position_info,
					assets_withdrawn,
					collected,
				)
			},
		};

		let collected_fees = asset_pair.try_credit_assets(lp, collected.fees)?;

		if position_info.liquidity == 0 {
			if let Some(range_orders) = pool.range_orders_cache.get_mut(lp) {
				range_orders.remove(&id);
				if range_orders.is_empty() {
					pool.range_orders_cache.remove(lp);
				}
			}
		} else {
			let range_orders = pool.range_orders_cache.entry(lp.clone()).or_default();
			range_orders.insert(id, tick_range.clone());
		}

		let zero_change = *liquidity_change.abs() == 0;

		if !zero_change || collected_fees != Default::default() {
			Self::deposit_event(Event::<T>::RangeOrderUpdated {
				lp: lp.clone(),
				base_asset: asset_pair.canonical_asset_pair.side_to_asset(asset_pair.base_side),
				pair_asset: asset_pair.canonical_asset_pair.side_to_asset(!asset_pair.base_side),
				id,
				tick_range,
				size_change: {
					if zero_change {
						None
					} else {
						Some(liquidity_change.map(|liquidity| RangeOrderChange {
							liquidity,
							amounts: assets_change,
						}))
					}
				},
				liquidity_total: position_info.liquidity,
				collected_fees,
			});
		}

		Ok(assets_change)
	}

	#[transactional]
	pub fn swap_with_network_fee(
		from: any::Asset,
		to: any::Asset,
		input_amount: AssetAmount,
	) -> Result<SwapOutput, DispatchError> {
		Ok(match (from, to) {
			(_, STABLE_ASSET) | (STABLE_ASSET, _) => {
				let output = Self::take_network_fee(Self::swap_single_leg(from, to, input_amount)?);
				SwapOutput { intermediary: None, output }
			},
			_ => {
				let intermediary = Self::swap_single_leg(from, STABLE_ASSET, input_amount)?;
				let output =
					Self::swap_single_leg(STABLE_ASSET, to, Self::take_network_fee(intermediary))?;
				SwapOutput { intermediary: Some(intermediary), output }
			},
		})
	}

	fn try_mutate_pool<
		R,
		E: From<pallet::Error<T>>,
		F: FnOnce(&AssetPair<T>) -> Result<(), E>,
		G: FnOnce(&AssetPair<T>, &mut Pool<T>) -> Result<R, E>,
	>(
		base_asset: any::Asset,
		pair_asset: any::Asset,
		f: F,
		g: G,
	) -> Result<R, E> {
		let asset_pair = AssetPair::<T>::new(base_asset, pair_asset)?;
		f(&asset_pair)?;
		Pools::<T>::try_mutate(asset_pair.canonical_asset_pair, |maybe_pool| {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;
			g(&asset_pair, pool)
		})
	}

	fn try_mutate_order<R, F: FnOnce(&AssetPair<T>, &mut Pool<T>) -> Result<R, DispatchError>>(
		lp: &T::AccountId,
		base_asset: any::Asset,
		pair_asset: any::Asset,
		f: F,
	) -> Result<R, DispatchError> {
		T::LpBalance::ensure_has_refund_address_for_pair(lp, base_asset, pair_asset)?;
		Self::try_mutate_pool(
			base_asset,
			pair_asset,
			|asset_pair| Self::inner_sweep(lp, asset_pair.base_side),
			f,
		)
	}

	pub fn current_price(from: Asset, to: Asset) -> Option<Price> {
		let asset_pair = AssetPair::new(from, to).ok()?;
		Pools::<T>::get(asset_pair.canonical_asset_pair)
			.and_then(|mut pool| pool.pool_state.current_price(asset_pair.base_side, Order::Sell))
	}

	pub fn required_asset_ratio_for_range_order(
		base_asset: any::Asset,
		pair_asset: any::Asset,
		tick_range: Range<cf_amm::common::Tick>,
	) -> Option<Result<AssetsMap<Amount>, DispatchError>> {
		let asset_pair = AssetPair::<T>::new(base_asset, pair_asset).ok()?;
		let pool = Pools::<T>::get(asset_pair.canonical_asset_pair)?;

		Some(
			pool.pool_state
				.required_asset_ratio_for_range_order(tick_range)
				.map_err(|error| {
					match error {
						range_orders::RequiredAssetRatioError::InvalidTickRange =>
							Error::<T>::InvalidTickRange,
					}
					.into()
				})
				.map(|side_map| asset_pair.side_map_to_assets_map(side_map)),
		)
	}

	pub fn pool_depth(
		base_asset: any::Asset,
		pair_asset: any::Asset,
		tick_range: Range<cf_amm::common::Tick>,
	) -> Option<Result<AssetsMap<UnidirectionalPoolDepth>, DispatchError>> {
		let asset_pair = AssetPair::<T>::new(base_asset, pair_asset).ok()?;
		let mut pool = Pools::<T>::get(asset_pair.canonical_asset_pair)?;

		let limit_orders = pool.pool_state.limit_order_depth(tick_range.clone()).map_err(|error| {
			match error {
				limit_orders::DepthError::InvalidTickRange => Error::<T>::InvalidTickRange,
				limit_orders::DepthError::InvalidTick => Error::<T>::InvalidTick,
			}
			.into()
		});

		let range_orders = pool.pool_state.range_order_depth(tick_range).map_err(|error| {
			match error {
				range_orders::DepthError::InvalidTickRange => Error::<T>::InvalidTickRange,
				range_orders::DepthError::InvalidTick => Error::<T>::InvalidTick,
			}
			.into()
		});

		Some(limit_orders.and_then(|limit_orders| {
			range_orders.map(|range_orders| {
				asset_pair.side_map_to_assets_map(SideMap::<()>::default().map(|side, ()| {
					let to_single_depth =
						|(price, depth)| UnidirectionalSubPoolDepth { price, depth };
					UnidirectionalPoolDepth {
						limit_orders: to_single_depth(limit_orders[side]),
						range_orders: to_single_depth(range_orders[side]),
					}
				}))
			})
		}))
	}

	pub fn pool_liquidity_providers(
		base_asset: any::Asset,
		pair_asset: any::Asset,
	) -> Result<BTreeSet<T::AccountId>, Error<T>> {
		let asset_pair = AssetPair::<T>::new(base_asset, pair_asset)?;
		let pool =
			Pools::<T>::get(asset_pair.canonical_asset_pair).ok_or(Error::<T>::PoolDoesNotExist)?;

		Ok(Iterator::chain(
			pool.limit_orders_cache.as_ref().into_iter().flat_map(|(side, limit_orders)| {
				let pool = &pool;
				limit_orders
					.iter()
					.filter(move |(lp, positions)| {
						positions.iter().any(move |(id, tick)| {
							!pool
								.pool_state
								.limit_order(&((*lp).clone(), *id), side, Order::Sell, *tick)
								.unwrap()
								.1
								.amount
								.is_zero()
						})
					})
					.map(|(lp, _positions)| lp.clone())
			}),
			pool.range_orders_cache.keys().cloned(),
		)
		.collect())
	}

	pub fn pools() -> Vec<(Asset, Asset)> {
		Pools::<T>::iter_keys()
			.map(|canonical_asset_pair| {
				(canonical_asset_pair.assets[Side::Zero], canonical_asset_pair.assets[Side::One])
			})
			.collect()
	}

	pub fn pool_info(base_asset: any::Asset, pair_asset: any::Asset) -> Option<PoolInfo> {
		let pool = Pools::<T>::get(CanonicalAssetPair::new(base_asset, pair_asset).ok()?)?;
		Some(PoolInfo {
			limit_order_fee_hundredth_pips: pool.pool_state.limit_order_fee(),
			range_order_fee_hundredth_pips: pool.pool_state.range_order_fee(),
		})
	}

	pub fn pool_liquidity(base_asset: any::Asset, pair_asset: any::Asset) -> Option<PoolLiquidity> {
		let asset_pair = AssetPair::new(base_asset, pair_asset).ok()?;
		let pool = Pools::<T>::get(asset_pair.canonical_asset_pair)?;
		Some(PoolLiquidity {
			limit_orders: AssetsMap::<()>::default().map_with_side(&asset_pair, |side, ()| {
				pool.pool_state.limit_order_liquidity(side, Order::Sell)
			}),
			range_orders: pool.pool_state.range_order_liquidity(),
		})
	}

	pub fn pool_orders(
		base_asset: any::Asset,
		pair_asset: any::Asset,
		lp: &T::AccountId,
	) -> Option<PoolOrders> {
		let asset_pair = AssetPair::new(base_asset, pair_asset).ok()?;
		let pool = Pools::<T>::get(asset_pair.canonical_asset_pair)?;
		Some(PoolOrders {
			limit_orders: AssetsMap::<()>::default().map_with_side(&asset_pair, |side, ()| {
				pool.limit_orders_cache[side]
					.get(lp)
					.into_iter()
					.flat_map(|limit_orders| {
						limit_orders.iter().map(|(id, tick)| {
							let (_collected, position_info) = pool
								.pool_state
								.limit_order(&(lp.clone(), *id), side, Order::Sell, *tick)
								.unwrap();
							(*id, *tick, position_info.amount)
						})
					})
					.collect()
			}),
			range_orders: pool
				.range_orders_cache
				.get(lp)
				.into_iter()
				.flat_map(|range_orders| {
					range_orders.iter().map(|(id, tick_range)| {
						let (_collected, position_info) = pool
							.pool_state
							.range_order(&(lp.clone(), *id), tick_range.clone())
							.unwrap();
						(*id, tick_range.clone(), position_info.liquidity)
					})
				})
				.collect(),
		})
	}

	pub fn pool_range_order_liquidity_value(
		base_asset: any::Asset,
		pair_asset: any::Asset,
		tick_range: Range<Tick>,
		liquidity: Liquidity,
	) -> Option<Result<AssetsMap<Amount>, DispatchError>> {
		let asset_pair = AssetPair::new(base_asset, pair_asset).ok()?;
		let pool = Pools::<T>::get(asset_pair.canonical_asset_pair)?;
		Some(
			pool.pool_state
				.range_order_liquidity_value(tick_range, liquidity)
				.map_err(|error| {
					match error {
						range_orders::LiquidityToAmountsError::InvalidTickRange =>
							Error::<T>::InvalidTickRange,
						range_orders::LiquidityToAmountsError::LiquidityTooLarge =>
							Error::<T>::MaximumGrossLiquidity,
					}
					.into()
				})
				.map(|side_map| asset_pair.side_map_to_assets_map(side_map)),
		)
	}

	/// Process changes to limit order:
	/// - Payout collected `fee` and `bought_amount`
	/// - Update cache storage for Pool
	/// - Deposit the correct event.
	#[allow(clippy::too_many_arguments)]
	fn process_limit_order_update(
		pool: &mut Pool<T>,
		asset_pair: &AssetPair<T>,
		lp: &T::AccountId,
		side: Side,
		order: OrderId,
		tick: Tick,
		collected: Collected,
		position_info: PositionInfo,
		amount_change: IncreaseOrDecrease<AssetAmount>,
	) -> DispatchResult {
		let collected_fees = asset_pair.try_credit_asset(lp, !side, collected.fees)?;
		let bought_amount = asset_pair.try_credit_asset(lp, !side, collected.bought_amount)?;
		let limit_orders = &mut pool.limit_orders_cache[side];
		if position_info.amount.is_zero() {
			if let Some(lp_limit_orders) = limit_orders.get_mut(lp) {
				lp_limit_orders.remove(&order);
				if lp_limit_orders.is_empty() {
					limit_orders.remove(lp);
				}
			}
		} else {
			limit_orders.entry(lp.clone()).or_default().insert(order, tick);
		}

		let zero_change = *amount_change.abs() == 0;

		if !zero_change ||
			collected_fees != Default::default() ||
			bought_amount != Default::default()
		{
			Self::deposit_event(Event::<T>::LimitOrderUpdated {
				lp: lp.clone(),
				sell_asset: asset_pair.canonical_asset_pair.side_to_asset(side),
				buy_asset: asset_pair.canonical_asset_pair.side_to_asset(!side),
				id: order,
				tick,
				amount_change: {
					if zero_change {
						None
					} else {
						Some(amount_change)
					}
				},
				amount_total: position_info.amount.try_into()?,
				collected_fees,
				bought_amount,
			});
		}
		Ok(())
	}
}

pub mod utilities {
	use super::*;

	pub fn calculate_network_fee(
		fee_percentage: Permill,
		input: AssetAmount,
	) -> (AssetAmount, AssetAmount) {
		let fee = fee_percentage * input;
		(input - fee, fee)
	}
}
