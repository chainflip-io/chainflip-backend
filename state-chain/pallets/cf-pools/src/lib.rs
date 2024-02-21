#![cfg_attr(not(feature = "std"), no_std)]
use core::ops::Range;

use cf_amm::{
	common::{Amount, PoolPairsMap, Price, Side, SqrtPriceQ64F96, Tick},
	limit_orders,
	limit_orders::{Collected, PositionInfo},
	range_orders,
	range_orders::Liquidity,
	PoolState,
};
use cf_primitives::{chains::assets::any, Asset, AssetAmount, SwapOutput, STABLE_ASSET};
use cf_traits::{impl_pallet_safe_mode, Chainflip, LpBalanceApi, PoolApi, SwappingApi};
use frame_support::{
	dispatch::GetDispatchInfo,
	pallet_prelude::*,
	sp_runtime::{Permill, Saturating, TransactionOutcome},
	storage::{with_storage_layer, with_transaction_unchecked},
	traits::{Defensive, OriginTrait, StorageVersion, UnfilteredDispatchable},
	transactional,
};

use frame_system::pallet_prelude::OriginFor;
use serde::{Deserialize, Serialize};
use sp_arithmetic::traits::{AtLeast32BitUnsigned, UniqueSaturatedInto, Zero};
use sp_std::{boxed::Box, collections::btree_set::BTreeSet, vec::Vec};

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

// TODO Add custom serialize/deserialize and encode/decode implementations that preserve canonical
// nature.
#[derive(Copy, Clone, Debug, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Eq, Hash)]
pub struct AssetPair {
	assets: PoolPairsMap<Asset>,
}
impl AssetPair {
	pub fn new(base_asset: Asset, quote_asset: Asset) -> Option<Self> {
		Some(AssetPair {
			assets: match (base_asset, quote_asset) {
				(STABLE_ASSET, STABLE_ASSET) => None,
				(_unstable_asset, STABLE_ASSET) =>
					Some(PoolPairsMap { base: base_asset, quote: quote_asset }),
				_ => None,
			}?,
		})
	}

	pub fn try_new<T: Config>(base_asset: Asset, quote_asset: Asset) -> Result<Self, Error<T>> {
		Self::new(base_asset, quote_asset).ok_or(Error::<T>::PoolDoesNotExist)
	}

	pub fn from_swap(from: Asset, to: Asset) -> Option<(Self, Side)> {
		#[allow(clippy::manual_map)]
		if let Some(asset_pair) = Self::new(from, to) {
			Some((asset_pair, Side::Sell))
		} else if let Some(asset_pair) = Self::new(to, from) {
			Some((asset_pair, Side::Buy))
		} else {
			None
		}
	}

	pub fn to_swap(base_asset: Asset, quote_asset: Asset, side: Side) -> (Asset, Asset) {
		match side {
			Side::Buy => (quote_asset, base_asset),
			Side::Sell => (base_asset, quote_asset),
		}
	}

	pub fn assets(&self) -> PoolPairsMap<Asset> {
		self.assets
	}
}

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
pub struct AskBidMap<S> {
	pub asks: S,
	pub bids: S,
}
impl<T> AskBidMap<T> {
	/// Takes a map from an asset to details regarding selling that asset, and returns a map from
	/// ask/bid to the details associated with the asks or the bids
	pub fn from_sell_map(map: PoolPairsMap<T>) -> Self {
		Self { asks: map.base, bids: map.quote }
	}

	pub fn from_fn<F: FnMut(Side) -> T>(mut f: F) -> Self {
		Self::from_sell_map(PoolPairsMap { base: f(Side::Sell), quote: f(Side::Buy) })
	}

	pub fn map<S, F: FnMut(T) -> S>(self, mut f: F) -> AskBidMap<S> {
		AskBidMap { asks: f(self.asks), bids: f(self.bids) }
	}
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

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
	pub struct LimitOrderUpdate<T: Config> {
		pub lp: T::AccountId,
		pub id: OrderId,
		pub call: Call<T>,
	}

	#[derive(Clone, Debug, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct Pool<T: Config> {
		/// A cache of all the range orders that exist in the pool. This must be kept up to date
		/// with the underlying pool.
		pub range_orders_cache: BTreeMap<T::AccountId, BTreeMap<OrderId, Range<Tick>>>,
		/// A cache of all the limit orders that exist in the pool. This must be kept up to date
		/// with the underlying pool. These are grouped by the asset the limit order is selling
		pub limit_orders_cache: PoolPairsMap<BTreeMap<T::AccountId, BTreeMap<OrderId, Tick>>>,
		pub pool_state: PoolState<(T::AccountId, OrderId)>,
	}

	pub type OrderId = u64;

	pub type AssetAmounts = PoolPairsMap<AssetAmount>;

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
		pub amounts: AssetAmounts,
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
	pub type Pools<T: Config> = StorageMap<_, Twox64Concat, AssetPair, Pool<T>, OptionQuery>;

	/// FLIP ready to be burned.
	#[pallet::storage]
	pub(super) type FlipToBurn<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	/// Interval at which we buy FLIP in order to burn it.
	#[pallet::storage]
	pub(super) type FlipBuyInterval<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Network fees, in USDC terms, that have been collected and are ready to be converted to FLIP.
	#[pallet::storage]
	pub type CollectedNetworkFee<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	/// Queue of limit orders, indexed by block number waiting to get minted or burned.
	#[pallet::storage]
	pub(super) type ScheduledLimitOrderUpdates<T: Config> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<LimitOrderUpdate<T>>, ValueQuery>;

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

			weight_used.saturating_accrue(T::DbWeight::get().reads(1));
			for LimitOrderUpdate { ref lp, id, call } in
				ScheduledLimitOrderUpdates::<T>::take(current_block)
			{
				let call_weight = call.get_dispatch_info().weight;
				let _result = with_storage_layer(move || {
					call.dispatch_bypass_filter(OriginTrait::signed(lp.clone()))
				})
				.map(|_| {
					Self::deposit_event(Event::<T>::ScheduledLimitOrderUpdateDispatchSuccess {
						lp: lp.clone(),
						order_id: id,
					});
				})
				.map_err(|err| {
					Self::deposit_event(Event::<T>::ScheduledLimitOrderUpdateDispatchFailure {
						lp: lp.clone(),
						order_id: id,
						error: err.error,
					});
				});
				weight_used.saturating_accrue(call_weight);
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
		/// Updating Limit Orders is disabled.
		UpdatingLimitOrdersDisabled,
		/// Updating Range Orders is disabled.
		UpdatingRangeOrdersDisabled,
		/// Unsupported call.
		UnsupportedCall,
		/// The update can't be scheduled because it has expired (dispatch_at is in the past).
		LimitOrderUpdateExpired,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		UpdatedBuyInterval {
			buy_interval: BlockNumberFor<T>,
		},
		NewPoolCreated {
			base_asset: Asset,
			quote_asset: Asset,
			fee_hundredth_pips: u32,
			initial_price: Price,
		},
		/// Indicates the details of a change made to a range order. A single update extrinsic may
		/// produce multiple of these events, particularly for example if the update changes the
		/// price/range of the order.
		RangeOrderUpdated {
			lp: T::AccountId,
			base_asset: Asset,
			quote_asset: Asset,
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
			base_asset: Asset,
			quote_asset: Asset,
			side: Side,
			id: OrderId,
			tick: Tick,
			sell_amount_change: Option<IncreaseOrDecrease<AssetAmount>>,
			sell_amount_total: AssetAmount,
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
			quote_asset: Asset,
			fee_hundredth_pips: u32,
		},
		/// A scheduled update to a limit order succeeded.
		ScheduledLimitOrderUpdateDispatchSuccess {
			lp: T::AccountId,
			order_id: OrderId,
		},
		/// A scheduled update to a limit order failed.
		ScheduledLimitOrderUpdateDispatchFailure {
			lp: T::AccountId,
			order_id: OrderId,
			error: DispatchError,
		},
		/// A limit order set or update was scheduled.
		LimitOrderSetOrUpdateScheduled {
			lp: T::AccountId,
			order_id: OrderId,
			dispatch_at: BlockNumberFor<T>,
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
			quote_asset: any::Asset,
			fee_hundredth_pips: u32,
			initial_price: Price,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
			Pools::<T>::try_mutate(asset_pair, |maybe_pool| {
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
				quote_asset,
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
			quote_asset: Asset,
			id: OrderId,
			option_tick_range: Option<core::ops::Range<Tick>>,
			size_change: IncreaseOrDecrease<RangeOrderSize>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().range_order_update_enabled,
				Error::<T>::UpdatingRangeOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_order(&lp, base_asset, quote_asset, |asset_pair, pool| {
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
									maximum: withdrawn_asset_amounts.map(Into::into),
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
								maximum: maximum.map(Into::into),
								minimum: minimum.map(Into::into),
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
			quote_asset: Asset,
			id: OrderId,
			option_tick_range: Option<core::ops::Range<Tick>>,
			size: RangeOrderSize,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().range_order_update_enabled,
				Error::<T>::UpdatingRangeOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_order(&lp, base_asset, quote_asset, |asset_pair, pool| {
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
								maximum: maximum.map(Into::into),
								minimum: minimum.map(Into::into),
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
			base_asset: any::Asset,
			quote_asset: any::Asset,
			side: Side,
			id: OrderId,
			option_tick: Option<Tick>,
			amount_change: IncreaseOrDecrease<AssetAmount>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().limit_order_update_enabled,
				Error::<T>::UpdatingLimitOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_order(&lp, base_asset, quote_asset, |asset_pair, pool| {
				let tick = match (
					pool.limit_orders_cache[side.to_sold_pair()]
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
								side,
								id,
								previous_tick,
								IncreaseOrDecrease::Decrease(cf_amm::common::Amount::MAX),
								/* allow_noop */ false,
							)?;
							Self::inner_update_limit_order(
								pool,
								&lp,
								asset_pair,
								side,
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
					side,
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
			base_asset: any::Asset,
			quote_asset: any::Asset,
			side: Side,
			id: OrderId,
			option_tick: Option<Tick>,
			sell_amount: AssetAmount,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().limit_order_update_enabled,
				Error::<T>::UpdatingLimitOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			Self::try_mutate_order(&lp, base_asset, quote_asset, |asset_pair, pool| {
				let tick = match (
					pool.limit_orders_cache[side.to_sold_pair()]
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
							side,
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
					side,
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
			quote_asset: Asset,
			fee_hundredth_pips: u32,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				PoolState::<(T::AccountId, OrderId)>::validate_fees(fee_hundredth_pips),
				Error::<T>::InvalidFeeAmount
			);
			let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
			Self::try_mutate_pool(asset_pair, |asset_pair: &AssetPair, pool| {
				pool.pool_state
					.set_fees(fee_hundredth_pips)
					.map_err(|_| Error::<T>::InvalidFeeAmount)?
					.try_map_with_pair(|asset, collected_fees| {
						for ((lp, id), tick, collected, position_info) in collected_fees.into_iter()
						{
							Self::process_limit_order_update(
								pool,
								asset_pair,
								&lp,
								asset.sell_order(),
								id,
								tick,
								collected,
								position_info,
								IncreaseOrDecrease::Increase(0),
							)?;
						}
						Result::<(), DispatchError>::Ok(())
					})
			})?;

			Self::deposit_event(Event::<T>::PoolFeeSet {
				base_asset,
				quote_asset,
				fee_hundredth_pips,
			});

			Ok(())
		}

		/// Schedules a limit order update to be executed at a later block.
		///
		/// The update is defined by the passed call, which can be one either `set_limit_order` or
		/// `update_limit_order` extrinsic at a later block. The call is executed at the specified
		/// block number, and the validity of the order is checked at the block number it enters
		/// the state-chain.
		///
		/// `dispatch_at` specifies the block at which to schedule the update. If the
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [UnsupportedCall](pallet_cf_pools::Error::UnsupportedCall)
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::schedule())]
		pub fn schedule_limit_order_update(
			origin: OriginFor<T>,
			call: Box<Call<T>>,
			dispatch_at: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			let current_block_number = frame_system::Pallet::<T>::block_number();
			ensure!(dispatch_at >= current_block_number, Error::<T>::LimitOrderUpdateExpired);

			let schedule_or_dispatch = |call: Call<T>, id: OrderId| {
				if current_block_number == dispatch_at {
					call.dispatch_bypass_filter(OriginTrait::signed(lp))
				} else {
					ScheduledLimitOrderUpdates::<T>::append(
						dispatch_at,
						LimitOrderUpdate { lp: lp.clone(), id, call },
					);
					Self::deposit_event(Event::<T>::LimitOrderSetOrUpdateScheduled {
						lp,
						order_id: id,
						dispatch_at,
					});
					Ok(().into())
				}
			};

			match *call {
				Call::update_limit_order { id, .. } => schedule_or_dispatch(*call, id),
				Call::set_limit_order { id, .. } => schedule_or_dispatch(*call, id),
				_ => Err(Error::<T>::UnsupportedCall)?,
			}
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
		let (asset_pair, order) =
			AssetPair::from_swap(from, to).ok_or(Error::<T>::PoolDoesNotExist)?;
		Self::try_mutate_pool(asset_pair, |_asset_pair, pool| {
			let (output_amount, remaining_amount) =
				pool.pool_state.swap(order, input_amount.into(), None);
			remaining_amount
				.is_zero()
				.then_some(())
				.ok_or(Error::<T>::InsufficientLiquidity)?;
			let output_amount = output_amount.try_into().map_err(|_| Error::<T>::OutputOverflow)?;
			Self::deposit_event(Event::<T>::AssetSwapped { from, to, input_amount, output_amount });
			Ok(output_amount)
		})
	}
}

impl<T: Config> PoolApi for Pallet<T> {
	type AccountId = T::AccountId;

	fn sweep(who: &T::AccountId) -> DispatchResult {
		Self::inner_sweep(who)
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
	/// The total fees earned in this pool by range orders.
	pub range_order_total_fees_earned: PoolPairsMap<Amount>,
	/// The total fees earned in this pool by limit orders.
	pub limit_order_total_fees_earned: PoolPairsMap<Amount>,
	/// The total amount of assets that have been bought by range orders in this pool.
	pub range_total_swap_inputs: PoolPairsMap<Amount>,
	/// The total amount of assets that have been bought by limit orders in this pool.
	pub limit_total_swap_inputs: PoolPairsMap<Amount>,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct LimitOrder<T: Config> {
	pub lp: T::AccountId,
	pub id: Amount, // TODO: Intro type alias
	pub tick: Tick,
	pub sell_amount: Amount,
	pub fees_earned: Amount,
	pub original_sell_amount: Amount,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct RangeOrder<T: Config> {
	pub lp: T::AccountId,
	pub id: Amount,
	pub range: Range<Tick>,
	pub liquidity: Liquidity,
	pub fees_earned: PoolPairsMap<Amount>,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct PoolOrders<T: Config> {
	/// Limit orders are groups by which asset they are selling.
	pub limit_orders: AskBidMap<Vec<LimitOrder<T>>>,
	/// Range orders can be both buy and/or sell therefore they not split. The current range order
	/// price determines if they are buy and/or sell.
	pub range_orders: Vec<RangeOrder<T>>,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct LimitOrderLiquidity {
	tick: Tick,
	amount: Amount,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct RangeOrderLiquidity {
	tick: Tick,
	liquidity: Amount, /* TODO: Change (Using Amount as it is U256 so we get the right
	                    * serialization) */
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolLiquidity {
	/// An ordered lists of the amount of assets available at each tick, if a tick contains zero
	/// liquidity it will not be included in the list. Note limit order liquidity is split by which
	/// asset the liquidity is "selling".
	pub limit_orders: AskBidMap<Vec<LimitOrderLiquidity>>,
	/// An ordered list of the amount of range order liquidity available from a tick until the next
	/// tick in the list. Note range orders can be both buy and/or sell therefore they not split by
	/// sold asset. The current range order price determines if the liquidity can be used for
	/// buying and/or selling,
	pub range_orders: Vec<RangeOrderLiquidity>,
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

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolOrder {
	pub amount: Amount,
	pub sqrt_price: SqrtPriceQ64F96,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolOrderbook {
	pub bids: Vec<PoolOrder>,
	pub asks: Vec<PoolOrder>,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolPriceV1 {
	pub price: Price,
	pub sqrt_price: SqrtPriceQ64F96,
	pub tick: Tick,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode, TypeInfo, PartialEq, Eq, Debug)]
pub struct PoolPriceV2 {
	pub sell: Option<SqrtPriceQ64F96>,
	pub buy: Option<SqrtPriceQ64F96>,
}

impl<T: Config> Pallet<T> {
	fn inner_sweep(lp: &T::AccountId) -> DispatchResult {
		// Collect to avoid undefined behaviour (See StorsgeMap::iter_keys documentation)
		for asset_pair in Pools::<T>::iter_keys().collect::<Vec<_>>() {
			let mut pool = Pools::<T>::get(asset_pair).unwrap();

			if let Some(range_orders_cache) = pool.range_orders_cache.get(lp).cloned() {
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

			for (assets, limit_orders_cache) in pool
				.limit_orders_cache
				.as_ref()
				.into_iter()
				.filter_map(|(assets, limit_orders_cache)| {
					limit_orders_cache
						.get(lp)
						.cloned()
						.map(|limit_orders_cache| (assets, limit_orders_cache))
				})
				.collect::<Vec<_>>()
			{
				for (id, tick) in limit_orders_cache {
					Self::inner_update_limit_order(
						&mut pool,
						lp,
						&asset_pair,
						assets.sell_order(),
						id,
						tick,
						IncreaseOrDecrease::Decrease(Default::default()),
						false,
					)?;
				}
			}

			Pools::<T>::insert(asset_pair, pool);
		}

		Ok(())
	}

	#[allow(clippy::too_many_arguments)]
	fn inner_update_limit_order(
		pool: &mut Pool<T>,
		lp: &T::AccountId,
		asset_pair: &AssetPair,
		side: Side,
		id: OrderId,
		tick: cf_amm::common::Tick,
		sold_amount_change: IncreaseOrDecrease<cf_amm::common::Amount>,
		allow_noop: bool,
	) -> Result<AssetAmount, DispatchError> {
		let (sold_amount_change, position_info, collected) =
			match sold_amount_change {
				IncreaseOrDecrease::Increase(sold_amount) => {
					let (collected, position_info) = match pool
						.pool_state
						.collect_and_mint_limit_order(&(lp.clone(), id), side, tick, sold_amount)
					{
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
							) => Error::<T>::MaximumPoolInstances,
						}),
					}?;

					let debited_amount: AssetAmount = sold_amount.try_into()?;
					T::LpBalance::try_debit_account(
						lp,
						asset_pair.assets()[side.to_sold_pair()],
						debited_amount,
					)?;

					(IncreaseOrDecrease::Increase(debited_amount), position_info, collected)
				},
				IncreaseOrDecrease::Decrease(sold_amount) => {
					let (sold_amount, collected, position_info) = match pool
						.pool_state
						.collect_and_burn_limit_order(&(lp.clone(), id), side, tick, sold_amount)
					{
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

					let withdrawn_amount: AssetAmount = sold_amount.try_into()?;
					T::LpBalance::try_credit_account(
						lp,
						asset_pair.assets()[side.to_sold_pair()],
						withdrawn_amount,
					)?;

					(IncreaseOrDecrease::Decrease(withdrawn_amount), position_info, collected)
				},
			};

		// Process the update
		Self::process_limit_order_update(
			pool,
			asset_pair,
			lp,
			side,
			id,
			tick,
			collected,
			position_info,
			sold_amount_change,
		)?;

		Ok(*sold_amount_change.abs())
	}

	#[allow(clippy::too_many_arguments)]
	fn inner_update_range_order(
		pool: &mut Pool<T>,
		lp: &T::AccountId,
		asset_pair: &AssetPair,
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
						|required_amounts| {
							asset_pair.assets().zip(required_amounts).try_map(
								|(asset, required_amount)| {
									AssetAmount::try_from(required_amount)
										.map_err(Into::into)
										.and_then(|required_amount| {
											T::LpBalance::try_debit_account(
												lp,
												asset,
												required_amount,
											)
											.map(|()| required_amount)
										})
								},
							)
						},
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

				let assets_withdrawn = asset_pair.assets().zip(assets_withdrawn).try_map(
					|(asset, amount_withdrawn)| {
						AssetAmount::try_from(amount_withdrawn).map_err(Into::into).and_then(
							|amount_withdrawn| {
								T::LpBalance::try_credit_account(lp, asset, amount_withdrawn)
									.map(|()| amount_withdrawn)
							},
						)
					},
				)?;

				(
					IncreaseOrDecrease::Decrease(burnt_liquidity),
					position_info,
					assets_withdrawn,
					collected,
				)
			},
		};

		let collected_fees =
			asset_pair.assets().zip(collected.fees).try_map(|(asset, collected_fees)| {
				AssetAmount::try_from(collected_fees).map_err(Into::into).and_then(
					|collected_fees| {
						T::LpBalance::record_fees(lp, collected_fees, asset);
						T::LpBalance::try_credit_account(lp, asset, collected_fees)
							.map(|()| collected_fees)
					},
				)
			})?;

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
				base_asset: asset_pair.assets().base,
				quote_asset: asset_pair.assets().quote,
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
			(_, STABLE_ASSET) => {
				let output = Self::take_network_fee(Self::swap_single_leg(from, to, input_amount)?);
				SwapOutput { intermediary: None, output }
			},
			(STABLE_ASSET, _) => {
				let output = Self::swap_single_leg(from, to, Self::take_network_fee(input_amount))?;
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
		F: FnOnce(&AssetPair, &mut Pool<T>) -> Result<R, E>,
	>(
		asset_pair: AssetPair,
		f: F,
	) -> Result<R, E> {
		Pools::<T>::try_mutate(asset_pair, |maybe_pool| {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;
			f(&asset_pair, pool)
		})
	}

	fn try_mutate_order<R, F: FnOnce(&AssetPair, &mut Pool<T>) -> Result<R, DispatchError>>(
		lp: &T::AccountId,
		base_asset: any::Asset,
		quote_asset: any::Asset,
		f: F,
	) -> Result<R, DispatchError> {
		T::LpBalance::ensure_has_refund_address_for_pair(lp, base_asset, quote_asset)?;
		let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
		Self::inner_sweep(lp)?;
		Self::try_mutate_pool(asset_pair, f)
	}

	pub fn current_price(from: Asset, to: Asset) -> Option<PoolPriceV1> {
		let (asset_pair, order) = AssetPair::from_swap(from, to)?;
		Pools::<T>::get(asset_pair).and_then(|mut pool| {
			let (price, sqrt_price, tick) = pool.pool_state.current_price(order)?;
			Some(PoolPriceV1 { price, sqrt_price, tick })
		})
	}

	pub fn pool_price(base_asset: Asset, quote_asset: Asset) -> Result<PoolPriceV2, DispatchError> {
		let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
		let mut pool = Pools::<T>::get(asset_pair).ok_or(Error::<T>::PoolDoesNotExist)?;
		Ok(PoolPriceV2 {
			sell: pool.pool_state.current_price(Side::Sell).map(|(_, sqrt_price, _)| sqrt_price),
			buy: pool.pool_state.current_price(Side::Buy).map(|(_, sqrt_price, _)| sqrt_price),
		})
	}

	pub fn required_asset_ratio_for_range_order(
		base_asset: any::Asset,
		quote_asset: any::Asset,
		tick_range: Range<cf_amm::common::Tick>,
	) -> Result<PoolPairsMap<Amount>, DispatchError> {
		let pool_state = Pools::<T>::get(AssetPair::try_new::<T>(base_asset, quote_asset)?)
			.ok_or(Error::<T>::PoolDoesNotExist)?
			.pool_state;

		pool_state
			.required_asset_ratio_for_range_order(tick_range)
			.map_err(|error| {
				match error {
					range_orders::RequiredAssetRatioError::InvalidTickRange =>
						Error::<T>::InvalidTickRange,
				}
				.into()
			})
			.map(Into::into)
	}

	pub fn pool_orderbook(
		base_asset: any::Asset,
		quote_asset: any::Asset,
		orders: u32,
	) -> Result<PoolOrderbook, DispatchError> {
		let orders = sp_std::cmp::max(sp_std::cmp::min(orders, 16384), 1);

		let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
		let pool_state =
			Pools::<T>::get(asset_pair).ok_or(Error::<T>::PoolDoesNotExist)?.pool_state;

		// TODO: Need to change limit order pool implmentation so Amount::MAX is guaranteed to drain
		// pool (so the calculated amounts here are guaranteed to reflect the accurate
		// maximum_bough_amounts)

		Ok(PoolOrderbook {
			asks: {
				let mut pool_state = pool_state.clone();
				let sqrt_prices = pool_state.logarithm_sqrt_price_sequence(Side::Buy, orders);

				sqrt_prices
					.into_iter()
					.filter_map(|sqrt_price| {
						let (sold_base_amount, remaining_quote_amount) =
							pool_state.swap(Side::Buy, Amount::MAX, Some(sqrt_price));

						let bought_quote_amount = Amount::MAX - remaining_quote_amount;

						if sold_base_amount.is_zero() || bought_quote_amount.is_zero() {
							None
						} else {
							Some(PoolOrder {
								amount: sold_base_amount,
								sqrt_price: cf_amm::common::bounded_sqrt_price(
									bought_quote_amount,
									sold_base_amount,
								),
							})
						}
					})
					.collect()
			},
			bids: {
				let mut pool_state = pool_state;
				let sqrt_prices = pool_state.logarithm_sqrt_price_sequence(Side::Sell, orders);

				sqrt_prices
					.into_iter()
					.filter_map(|sqrt_price| {
						let (sold_quote_amount, remaining_base_amount) =
							pool_state.swap(Side::Sell, Amount::MAX, Some(sqrt_price));

						let bought_base_amount = Amount::MAX - remaining_base_amount;

						if sold_quote_amount.is_zero() || bought_base_amount.is_zero() {
							None
						} else {
							Some(PoolOrder {
								amount: bought_base_amount,
								sqrt_price: cf_amm::common::bounded_sqrt_price(
									sold_quote_amount,
									bought_base_amount,
								),
							})
						}
					})
					.collect()
			},
		})
	}

	pub fn pool_depth(
		base_asset: any::Asset,
		quote_asset: any::Asset,
		tick_range: Range<cf_amm::common::Tick>,
	) -> Result<AskBidMap<UnidirectionalPoolDepth>, DispatchError> {
		let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
		let mut pool = Pools::<T>::get(asset_pair).ok_or(Error::<T>::PoolDoesNotExist)?;

		let limit_orders =
			pool.pool_state
				.limit_order_depth(tick_range.clone())
				.map_err(|error| match error {
					limit_orders::DepthError::InvalidTickRange => Error::<T>::InvalidTickRange,
					limit_orders::DepthError::InvalidTick => Error::<T>::InvalidTick,
				})?;

		let range_orders =
			pool.pool_state.range_order_depth(tick_range).map_err(|error| match error {
				range_orders::DepthError::InvalidTickRange => Error::<T>::InvalidTickRange,
				range_orders::DepthError::InvalidTick => Error::<T>::InvalidTick,
			})?;

		Ok(AskBidMap::from_sell_map(limit_orders.zip(range_orders).map(
			|(limit_orders, range_orders)| {
				let to_single_depth = |(price, depth)| UnidirectionalSubPoolDepth { price, depth };
				UnidirectionalPoolDepth {
					limit_orders: to_single_depth(limit_orders),
					range_orders: to_single_depth(range_orders),
				}
			},
		)))
	}

	pub fn pool_liquidity_providers(
		base_asset: any::Asset,
		quote_asset: any::Asset,
	) -> Result<BTreeSet<T::AccountId>, Error<T>> {
		let asset_pair = AssetPair::try_new(base_asset, quote_asset)?;
		let pool = Pools::<T>::get(asset_pair).ok_or(Error::<T>::PoolDoesNotExist)?;

		Ok(Iterator::chain(
			pool.limit_orders_cache.as_ref().into_iter().flat_map(|(assets, limit_orders)| {
				let pool = &pool;
				limit_orders
					.iter()
					.filter(move |(lp, positions)| {
						positions.iter().any(move |(id, tick)| {
							!pool
								.pool_state
								.limit_order(&((*lp).clone(), *id), assets.sell_order(), *tick)
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

	pub fn pools() -> Vec<PoolPairsMap<Asset>> {
		Pools::<T>::iter_keys().map(|asset_pair| asset_pair.assets()).collect()
	}

	pub fn pool_info(
		base_asset: any::Asset,
		quote_asset: any::Asset,
	) -> Result<PoolInfo, DispatchError> {
		let pool = Pools::<T>::get(AssetPair::try_new::<T>(base_asset, quote_asset)?)
			.ok_or(Error::<T>::PoolDoesNotExist)?;
		Ok(PoolInfo {
			limit_order_fee_hundredth_pips: pool.pool_state.limit_order_fee(),
			range_order_fee_hundredth_pips: pool.pool_state.range_order_fee(),
			range_order_total_fees_earned: pool.pool_state.range_order_total_fees_earned(),
			limit_order_total_fees_earned: pool.pool_state.limit_order_total_fees_earned(),
			range_total_swap_inputs: pool.pool_state.range_order_swap_inputs(),
			limit_total_swap_inputs: pool.pool_state.limit_order_swap_inputs(),
		})
	}

	pub fn pool_liquidity(
		base_asset: any::Asset,
		quote_asset: any::Asset,
	) -> Result<PoolLiquidity, DispatchError> {
		let pool = Pools::<T>::get(AssetPair::try_new::<T>(base_asset, quote_asset)?)
			.ok_or(Error::<T>::PoolDoesNotExist)?;
		Ok(PoolLiquidity {
			limit_orders: AskBidMap::from_fn(|order| {
				pool.pool_state
					.limit_order_liquidity(order)
					.into_iter()
					.map(|(tick, amount)| LimitOrderLiquidity { tick, amount })
					.collect()
			}),
			range_orders: pool
				.pool_state
				.range_order_liquidity()
				.into_iter()
				.map(|(tick, liquidity)| RangeOrderLiquidity { tick, liquidity: liquidity.into() })
				.collect(),
		})
	}

	/// Returns the limit and range orders for a given Liquidity Provider within the given pool.
	pub fn pool_orders(
		base_asset: any::Asset,
		quote_asset: any::Asset,
		option_lp: Option<T::AccountId>,
	) -> Result<PoolOrders<T>, DispatchError> {
		let pool = Pools::<T>::get(AssetPair::try_new::<T>(base_asset, quote_asset)?)
			.ok_or(Error::<T>::PoolDoesNotExist)?;
		let option_lp = option_lp.as_ref();
		Ok(PoolOrders {
			limit_orders: AskBidMap::from_sell_map(pool.limit_orders_cache.as_ref().map_with_pair(
				|asset, limit_orders_cache| {
					cf_utilities::conditional::conditional(
						option_lp,
						|lp| {
							limit_orders_cache
								.get(lp)
								.into_iter()
								.flatten()
								.map(|(id, tick)| (lp.clone(), *id, *tick))
						},
						|()| {
							limit_orders_cache.iter().flat_map(move |(lp, orders)| {
								orders.iter().map({
									let lp = lp.clone();
									move |(id, tick)| (lp.clone(), *id, *tick)
								})
							})
						},
					)
					.filter_map(|(lp, id, tick)| {
						let (collected, position_info) = pool
							.pool_state
							.limit_order(&(lp.clone(), id), asset.sell_order(), tick)
							.unwrap();
						if position_info.amount.is_zero() {
							None
						} else {
							Some(LimitOrder {
								lp: lp.clone(),
								id: id.into(),
								tick,
								sell_amount: position_info.amount,
								fees_earned: collected.accumulative_fees,
								original_sell_amount: collected.original_amount,
							})
						}
					})
					.collect()
				},
			)),
			range_orders: cf_utilities::conditional::conditional(
				option_lp,
				|lp| {
					pool.range_orders_cache
						.get(lp)
						.into_iter()
						.flatten()
						.map(|(id, range)| (lp.clone(), *id, range.clone()))
				},
				|()| {
					pool.range_orders_cache.iter().flat_map(move |(lp, orders)| {
						orders.iter().map({
							let lp = lp.clone();
							move |(id, range)| (lp.clone(), *id, range.clone())
						})
					})
				},
			)
			.map(|(lp, id, tick_range)| {
				let (collected, position_info) =
					pool.pool_state.range_order(&(lp.clone(), id), tick_range.clone()).unwrap();
				RangeOrder {
					lp: lp.clone(),
					id: id.into(),
					range: tick_range.clone(),
					liquidity: position_info.liquidity,
					fees_earned: collected.accumulative_fees,
				}
			})
			.collect(),
		})
	}

	pub fn pool_range_order_liquidity_value(
		base_asset: any::Asset,
		quote_asset: any::Asset,
		tick_range: Range<Tick>,
		liquidity: Liquidity,
	) -> Result<PoolPairsMap<Amount>, DispatchError> {
		let pool = Pools::<T>::get(AssetPair::try_new::<T>(base_asset, quote_asset)?)
			.ok_or(Error::<T>::PoolDoesNotExist)?;
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
			.map(Into::into)
	}

	/// Process changes to limit order:
	/// - Payout collected `fee` and `bought_amount`
	/// - Update cache storage for Pool
	/// - Deposit the correct event.
	#[allow(clippy::too_many_arguments)]
	fn process_limit_order_update(
		pool: &mut Pool<T>,
		asset_pair: &AssetPair,
		lp: &T::AccountId,
		order: Side,
		id: OrderId,
		tick: Tick,
		collected: Collected,
		position_info: PositionInfo,
		amount_change: IncreaseOrDecrease<AssetAmount>,
	) -> DispatchResult {
		let collected_fees: AssetAmount = collected.fees.try_into()?;
		let asset = asset_pair.assets()[!order.to_sold_pair()];
		T::LpBalance::record_fees(lp, collected_fees, asset);
		T::LpBalance::try_credit_account(lp, asset, collected_fees)?;

		let bought_amount: AssetAmount = collected.bought_amount.try_into()?;
		T::LpBalance::try_credit_account(
			lp,
			asset_pair.assets()[!order.to_sold_pair()],
			bought_amount,
		)?;

		let limit_orders = &mut pool.limit_orders_cache[order.to_sold_pair()];
		if position_info.amount.is_zero() {
			if let Some(lp_limit_orders) = limit_orders.get_mut(lp) {
				lp_limit_orders.remove(&id);
				if lp_limit_orders.is_empty() {
					limit_orders.remove(lp);
				}
			}
		} else {
			limit_orders.entry(lp.clone()).or_default().insert(id, tick);
		}

		let zero_change = *amount_change.abs() == 0;

		if !zero_change ||
			collected_fees != Default::default() ||
			bought_amount != Default::default()
		{
			Self::deposit_event(Event::<T>::LimitOrderUpdated {
				lp: lp.clone(),
				base_asset: asset_pair.assets().base,
				quote_asset: asset_pair.assets().quote,
				side: order,
				id,
				tick,
				sell_amount_change: {
					if zero_change {
						None
					} else {
						Some(amount_change)
					}
				},
				sell_amount_total: position_info.amount.try_into()?,
				collected_fees,
				bought_amount,
			});
		}
		Ok(())
	}
}

impl<T: Config> cf_traits::AssetConverter for Pallet<T> {
	fn estimate_swap_input_for_desired_output<
		Amount: Into<AssetAmount> + AtLeast32BitUnsigned + Copy,
	>(
		input_asset: impl Into<Asset>,
		output_asset: impl Into<Asset>,
		desired_output_amount: Amount,
	) -> Option<Amount> {
		let input_asset = input_asset.into();
		let output_asset = output_asset.into();

		if input_asset == output_asset {
			return Some(desired_output_amount)
		}
		// Because we don't know input amount, we swap in the
		// opposite direction, which should give us a good enough
		// approximation of the required input amount:
		with_transaction_unchecked(|| {
			TransactionOutcome::Rollback(
				Self::swap_with_network_fee(
					output_asset,
					input_asset,
					desired_output_amount.into(),
				)
				.ok(),
			)
		})
		.map(|swap_output| swap_output.output.unique_saturated_into())
	}

	/// Try to convert the input asset to the output asset, subject to an available input amount and
	/// desired output amount. The actual output amount is not guaranteed to be close to the desired
	/// amount.
	///
	/// Returns the remaining input amount and the resultant output amount.
	fn convert_asset_to_approximate_output<
		Amount: Into<AssetAmount> + AtLeast32BitUnsigned + Copy,
	>(
		input_asset: impl Into<Asset>,
		available_input_amount: Amount,
		output_asset: impl Into<Asset>,
		desired_output_amount: Amount,
	) -> Option<(Amount, Amount)> {
		use frame_support::sp_runtime::helpers_128bit::multiply_by_rational_with_rounding;

		if desired_output_amount.is_zero() {
			return Some((available_input_amount, Zero::zero()))
		}
		if available_input_amount.is_zero() {
			return None
		}

		let input_asset = input_asset.into();
		let output_asset = output_asset.into();
		if input_asset == output_asset {
			if desired_output_amount < available_input_amount {
				return Some((
					available_input_amount.saturating_sub(desired_output_amount),
					desired_output_amount,
				))
			} else {
				return Some((Zero::zero(), available_input_amount))
			}
		}

		let available_output_amount = with_transaction_unchecked(|| {
			TransactionOutcome::Rollback(
				Self::swap_with_network_fee(
					input_asset,
					output_asset,
					available_input_amount.into(),
				)
				.ok(),
			)
		})?
		.output;

		let input_amount_to_convert = multiply_by_rational_with_rounding(
			desired_output_amount.into(),
			available_input_amount.into(),
			available_output_amount,
			sp_arithmetic::Rounding::Down,
		)
		.defensive_proof(
			"Unexpected overflow occurred during asset conversion. Please report this to Chainflip Labs."
		)?;

		Some((
			available_input_amount.saturating_sub(input_amount_to_convert.unique_saturated_into()),
			Self::swap_with_network_fee(
				input_asset,
				output_asset,
				sp_std::cmp::min(input_amount_to_convert, available_input_amount.into()),
			)
			.ok()?
			.output
			.unique_saturated_into(),
		))
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
