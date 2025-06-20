// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
use cf_amm::{
	common::{PoolPairsMap, Side},
	limit_orders::{self, Collected, PositionInfo},
	math::{bounded_sqrt_price, Amount, Price, SqrtPriceQ64F96, Tick, MAX_SQRT_PRICE},
	range_orders::{self, Liquidity},
	PoolState,
};
use cf_chains::assets::any::AssetMap;
use cf_primitives::{chains::assets::any, Asset, AssetAmount, STABLE_ASSET};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AccountRoleRegistry, BalanceApi, Chainflip, LpOrdersWeightsProvider,
	PoolApi, SwapRequestHandler, SwappingApi,
};
use sp_runtime::Saturating;

use cf_traits::LpRegistration;
pub use cf_traits::{IncreaseOrDecrease, OrderId};
use core::ops::Range;
use frame_support::{
	pallet_prelude::*,
	traits::{OnKilledAccount, OriginTrait, StorageVersion},
	transactional,
};
use frame_system::{
	pallet_prelude::{BlockNumberFor, OriginFor},
	WeightInfo as SystemWeightInfo,
};
pub use pallet::*;
use serde::{Deserialize, Serialize};
use sp_arithmetic::traits::{SaturatedConversion, Zero};
use sp_std::vec::Vec;

mod benchmarking;
pub mod migrations;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

impl_pallet_safe_mode!(PalletSafeMode; range_order_update_enabled, limit_order_update_enabled);

type SweepingThresholds = BoundedBTreeMap<Asset, AssetAmount, ConstU32<100>>;
pub struct StablecoinDefaults<const N: u128>;
impl<const N: u128> Get<SweepingThresholds> for StablecoinDefaults<N> {
	fn get() -> SweepingThresholds {
		cf_primitives::StablecoinDefaults::<N>::get().try_into().unwrap()
	}
}

// Limit on how far in the future an LP can schedule a limit order update/close.
const SCHEDULE_UPDATE_LIMIT_BLOCKS: u32 = 3600; // 6 hours

pub const MAX_ORDERS_DELETE: u32 = 100;
#[derive(
	serde::Serialize,
	serde::Deserialize,
	Copy,
	Clone,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Hash,
)]
#[serde(untagged)]
pub enum CloseOrder {
	Limit { base_asset: any::Asset, quote_asset: any::Asset, side: Side, id: OrderId },
	Range { base_asset: any::Asset, quote_asset: any::Asset, id: OrderId },
}
// TODO Add custom serialize/deserialize and encode/decode implementations that preserve canonical
// nature.
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
	Hash,
	PartialOrd,
	Ord,
)]
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

#[derive(Clone, Encode, DebugNoBound, Decode, TypeInfo, PartialEq, Eq)]
#[scale_info(skip_type_params(T))]
struct LimitOrderUpdate<T: Config> {
	pub lp: T::AccountId,
	pub base_asset: any::Asset,
	pub quote_asset: any::Asset,
	pub side: Side,
	pub id: OrderId,
	pub details: LimitOrderUpdateDetails<BlockNumberFor<T>>,
}

#[derive(Clone, Encode, DebugNoBound, Decode, TypeInfo, PartialEq, Eq)]
enum LimitOrderUpdateDetails<BlockNumber: sp_std::fmt::Debug> {
	Update { option_tick: Option<Tick>, amount_change: IncreaseOrDecrease<AssetAmount> },
	Set { option_tick: Option<Tick>, sell_amount: AssetAmount, close_order_at: Option<BlockNumber> },
	Close,
}

impl<T: Config> LimitOrderUpdate<T> {
	pub fn dispatch(self) -> (Weight, DispatchResult) {
		let (weight, result) = match self.details {
			LimitOrderUpdateDetails::Set { option_tick, sell_amount, close_order_at } => {
				let result = Pallet::<T>::set_limit_order(
					OriginTrait::signed(self.lp.clone()),
					self.base_asset,
					self.quote_asset,
					self.side,
					self.id,
					option_tick,
					sell_amount,
					None, // Dispatch now
					close_order_at,
				);
				(T::WeightInfo::set_limit_order(), result)
			},
			LimitOrderUpdateDetails::Update { option_tick, amount_change } => {
				let result = Pallet::<T>::update_limit_order(
					OriginTrait::signed(self.lp.clone()),
					self.base_asset,
					self.quote_asset,
					self.side,
					self.id,
					option_tick,
					amount_change,
					None, // Dispatch now
				);
				(T::WeightInfo::update_limit_order(), result)
			},
			LimitOrderUpdateDetails::Close => {
				let result = Pallet::<T>::update_limit_order(
					OriginTrait::signed(self.lp.clone()),
					self.base_asset,
					self.quote_asset,
					self.side,
					self.id,
					None,
					IncreaseOrDecrease::Decrease(AssetAmount::MAX),
					None, // Dispatch now
				);
				let result = if let Err(err) = result {
					if err == Error::<T>::OrderDoesNotExist.into() {
						// Ignore the error if the order doesn't exist, as this is expected.
						Ok(())
					} else {
						Err(err)
					}
				} else {
					result
				};
				(T::WeightInfo::update_limit_order(), result)
			},
		};
		match result {
			Ok(()) => {
				Pallet::<T>::deposit_event(Event::<T>::ScheduledLimitOrderUpdateDispatchSuccess {
					lp: self.lp,
					order_id: self.id,
				});
			},
			Err(err) => {
				Pallet::<T>::deposit_event(Event::<T>::ScheduledLimitOrderUpdateDispatchFailure {
					lp: self.lp,
					order_id: self.id,
					error: err,
				});
			},
		}
		(weight, result)
	}

	pub fn schedule(self, dispatch_at: BlockNumberFor<T>) {
		Pallet::<T>::deposit_event(Event::<T>::LimitOrderSetOrUpdateScheduled {
			lp: self.lp.clone(),
			order_id: self.id,
			dispatch_at,
		});
		ScheduledLimitOrderUpdates::<T>::append(dispatch_at, self);
	}

	pub fn schedule_or_dispatch(self, dispatch_at: Option<BlockNumberFor<T>>) -> DispatchResult {
		if let Some(dispatch_at) = dispatch_at {
			let current_block_number = frame_system::Pallet::<T>::block_number();
			ensure!(
				dispatch_at >= current_block_number &&
					dispatch_at <=
						current_block_number.saturating_add(BlockNumberFor::<T>::from(
							SCHEDULE_UPDATE_LIMIT_BLOCKS
						)),
				Error::<T>::InvalidDispatchAt
			);

			if let LimitOrderUpdateDetails::Set { close_order_at: Some(close_order_at), .. } =
				self.details
			{
				ensure!(
					close_order_at > dispatch_at &&
						close_order_at <=
							current_block_number.saturating_add(BlockNumberFor::<T>::from(
								SCHEDULE_UPDATE_LIMIT_BLOCKS
							)),
					Error::<T>::InvalidCloseOrderAt
				);
			}

			if current_block_number == dispatch_at {
				let (_weight, result) = self.dispatch();
				result
			} else {
				self.schedule(dispatch_at);
				Ok(())
			}
		} else {
			let (_weight, result) = self.dispatch();
			result
		}
	}
}

#[derive(Clone, RuntimeDebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletConfigUpdate {
	LimitOrderAutoSweepingThreshold { asset: Asset, amount: AssetAmount },
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(7);

#[frame_support::pallet]
pub mod pallet {
	use cf_amm::{
		math::Tick,
		range_orders::{self, Liquidity},
	};
	use sp_std::collections::btree_map::BTreeMap;

	use super::*;

	#[derive(Clone, DebugNoBound, Encode, Decode, TypeInfo, PartialEq)]
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

	impl RangeOrderSize {
		/// Returns whether or not the maximum amount of assets contained is 0.
		pub fn max_is_zero(&self) -> bool {
			match self {
				RangeOrderSize::AssetAmounts { maximum: PoolPairsMap { base, quote }, .. } =>
					*base + *quote,
				RangeOrderSize::Liquidity { liquidity } => *liquidity,
			}
			.is_zero()
		}
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

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Access to the account balances.
		type LpBalance: BalanceApi<AccountId = Self::AccountId>;

		/// LP address registration and verification.
		type LpRegistrationApi: LpRegistration<AccountId = Self::AccountId>;

		type SwapRequestHandler: SwapRequestHandler;

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

	/// Queue of limit orders, indexed by block number waiting to get minted or burned.
	#[pallet::storage]
	pub(super) type ScheduledLimitOrderUpdates<T: Config> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<LimitOrderUpdate<T>>, ValueQuery>;

	/// Maximum price impact for a single swap, measured in number of ticks. Configurable
	/// for each pool.
	#[pallet::storage]
	pub(super) type MaximumPriceImpact<T: Config> =
		StorageMap<_, Twox64Concat, AssetPair, u32, OptionQuery>;

	/// Stores thresholds for each asset used in auto-sweeping: if after a swap the amount
	/// collectable from a limit order reaches/exceeds the threshold, the order it automatically
	/// swept
	#[pallet::storage]
	pub(super) type LimitOrderAutoSweepingThresholds<T: Config> =
		StorageValue<_, SweepingThresholds, ValueQuery, StablecoinDefaults<1_000_000_000>>; // $1000 USD

	#[pallet::storage]
	/// Historical earned fees for an account.
	pub type HistoricalEarnedFees<T: Config> =
		StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let mut weight_used: Weight = T::DbWeight::get().reads(1);

			Self::auto_sweep_limit_orders();

			for update in ScheduledLimitOrderUpdates::<T>::take(current_block) {
				let (call_weight, _result) = update.dispatch();
				weight_used.saturating_accrue(call_weight);
			}
			weight_used
		}
	}

	#[pallet::error]
	pub enum Error<T> {
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
		AssetRatioUnachievable,
		/// Updating Limit Orders is disabled.
		UpdatingLimitOrdersDisabled,
		/// Updating Range Orders is disabled.
		UpdatingRangeOrdersDisabled,
		/// Unsupported call.
		UnsupportedCall,
		/// The update can't be scheduled because the given dispatch_at block is in the past or too
		/// far in the future (3600 blocks).
		InvalidDispatchAt,
		/// The given close_order_at is in the past, not larger than dispatch_at or too far in the
		/// future (3600 blocks).
		InvalidCloseOrderAt,
		/// The range order size is invalid.
		InvalidSize,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
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
		/// The Price Impact limit has been set for a pool.
		PriceImpactLimitSet {
			asset_pair: AssetPair,
			limit: Option<u32>,
		},
		/// An order wasn't deleted (order not found)
		OrderDeletionFailed {
			order: CloseOrder,
		},
		PalletConfigUpdated {
			update: PalletConfigUpdate,
		},
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new pool.
		/// Requires Governance.
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

			Self::create_pool(base_asset, quote_asset, fee_hundredth_pips, initial_price)
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
			T::LpRegistrationApi::ensure_has_refund_address_for_assets(
				&lp,
				[base_asset, quote_asset],
			)?;
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
							let (withdrawn_asset_amounts, _) = Self::inner_update_range_order(
								pool,
								&lp,
								asset_pair,
								id,
								previous_tick_range,
								IncreaseOrDecrease::Decrease(range_orders::Size::Liquidity {
									liquidity: Liquidity::MAX,
								}),
								NoOpStatus::Error,
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
								NoOpStatus::Allow,
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
					NoOpStatus::Error,
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
			T::LpRegistrationApi::ensure_has_refund_address_for_assets(
				&lp,
				[base_asset, quote_asset],
			)?;
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
							NoOpStatus::Error,
						)?;

						Ok(option_new_tick_range.unwrap_or(previous_tick_range))
					},
				}?;
				let (_, new_order_liquidity) = Self::inner_update_range_order(
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
					NoOpStatus::Allow,
				)?;

				// Asset input and resultant liquidity changes should be consistent.
				// This condition can be breached in cases where eg. the assets amounts are rounded
				// to zero liquidity.
				ensure!(
					(size.max_is_zero() && new_order_liquidity.is_zero()) ||
						(!size.max_is_zero() && !new_order_liquidity.is_zero()),
					Error::<T>::InvalidSize
				);

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
		/// `dispatch_at` specifies the block at which to schedule the update.
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
			dispatch_at: Option<BlockNumberFor<T>>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().limit_order_update_enabled,
				Error::<T>::UpdatingLimitOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			T::LpRegistrationApi::ensure_has_refund_address_for_assets(
				&lp,
				[base_asset, quote_asset],
			)?;

			if let Some(dispatch_at) = dispatch_at {
				LimitOrderUpdate::<T> {
					lp: lp.clone(),
					base_asset,
					quote_asset,
					side,
					id,
					details: LimitOrderUpdateDetails::Update { option_tick, amount_change },
				}
				.schedule_or_dispatch(Some(dispatch_at))
			} else {
				Self::inner_update_limit_order(
					&lp,
					base_asset,
					quote_asset,
					side,
					id,
					option_tick,
					amount_change,
				)
			}
		}

		/// Optionally move the order to a different tick and then set its amount of liquidity. The
		/// appropriate assets will be debited or credited from your balance as needed. If the
		/// order_id isn't being used at the moment you must specify a tick, otherwise it will not
		/// know what tick you want the order to be over. Note limit order order_id's are
		/// independent of range order order_id's. In addition to that, order_id's for buy and sell
		/// limit orders i.e. those in different directions are independent. Therefore you may have
		/// two limit orders with the same order_id in the same pool, one to buy Eth and one to sell
		/// Eth for example.
		/// `dispatch_at` specifies the block at which to schedule the update.
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
			dispatch_at: Option<BlockNumberFor<T>>,
			close_order_at: Option<BlockNumberFor<T>>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().limit_order_update_enabled,
				Error::<T>::UpdatingLimitOrdersDisabled
			);
			let lp = T::AccountRoleRegistry::ensure_liquidity_provider(origin.clone())?;
			T::LpRegistrationApi::ensure_has_refund_address_for_assets(
				&lp,
				[base_asset, quote_asset],
			)?;

			if let Some(dispatch_at) = dispatch_at {
				LimitOrderUpdate::<T> {
					lp: lp.clone(),
					base_asset,
					quote_asset,
					side,
					id,
					details: LimitOrderUpdateDetails::Set {
						option_tick,
						sell_amount,
						close_order_at,
					},
				}
				.schedule_or_dispatch(Some(dispatch_at))?;
			} else {
				Self::inner_set_limit_order(
					&lp,
					base_asset,
					quote_asset,
					side,
					id,
					option_tick,
					sell_amount,
				)?;

				if let Some(close_order_at) = close_order_at {
					let current_block_number = frame_system::Pallet::<T>::block_number();
					ensure!(
						close_order_at > current_block_number &&
							close_order_at <=
								current_block_number.saturating_add(
									BlockNumberFor::<T>::from(SCHEDULE_UPDATE_LIMIT_BLOCKS)
								),
						Error::<T>::InvalidCloseOrderAt
					);

					LimitOrderUpdate::<T> {
						lp: lp.clone(),
						base_asset,
						quote_asset,
						side,
						id,
						details: LimitOrderUpdateDetails::Close,
					}
					.schedule(close_order_at);
				}
			}
			Ok(())
		}

		/// Sets the Liquidity Pool fees. Also collect earned fees and bought amount for
		/// all positions within the fee and accredit them to the liquidity provider.
		/// Requires governance origin.
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
			Self::try_mutate_pool(asset_pair, |_asset_pair: &AssetPair, pool| {
				pool.pool_state
					.set_range_order_fees(fee_hundredth_pips)
					.map_err(|_| Error::<T>::InvalidFeeAmount)
			})?;

			Self::deposit_event(Event::<T>::PoolFeeSet {
				base_asset,
				quote_asset,
				fee_hundredth_pips,
			});

			Ok(())
		}

		/// Sets per-pool limits (in number of ticks) that determine the allowed change in price of
		/// the bought asset during a swap.
		///
		/// Note that due to how the limit is applied, total measured price impact of a swap can
		/// exceed the limit. The limit is applied to whichever is *smaller* out of the following
		/// two metrics:
		/// - The number of ticks between a swap's mean execution price and the pool price *before*
		///   the swap.
		/// - The number of ticks between a swap's mean execution price and the pool price *after*
		///   the swap.
		///
		/// This ensures that small outlying pockets of liquidity cannot individually trigger the
		/// limit. If the limit is exceeded the swap will fail and will be retried in the next
		/// block.
		///
		/// Setting the limit to `None` disables it.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::set_maximum_price_impact(limits.len() as u32))]
		pub fn set_maximum_price_impact(
			origin: OriginFor<T>,
			limits: BoundedVec<(Asset, Option<u32>), ConstU32<10>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for (asset, ticks) in limits {
				let asset_pair = AssetPair::try_new::<T>(asset, STABLE_ASSET)?;
				MaximumPriceImpact::<T>::set(asset_pair, ticks);
				Self::deposit_event(Event::<T>::PriceImpactLimitSet { asset_pair, limit: ticks });
			}

			Ok(())
		}

		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::cancel_orders_batch(orders.len() as u32))]
		pub fn cancel_orders_batch(
			origin: OriginFor<T>,
			orders: BoundedVec<CloseOrder, ConstU32<MAX_ORDERS_DELETE>>,
		) -> DispatchResult {
			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			ensure!(
				T::SafeMode::get().limit_order_update_enabled,
				Error::<T>::UpdatingLimitOrdersDisabled
			);
			for order in orders {
				match order {
					CloseOrder::Limit { base_asset, quote_asset, side, id } => {
						let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
						Self::try_mutate_pool(asset_pair, |asset_pair, pool| {
							match pool.limit_orders_cache[side.to_sold_pair()]
								.get(lp)
								.and_then(|limit_orders| limit_orders.get(&id))
								.copied()
							{
								None => {
									Self::deposit_event(Event::<T>::OrderDeletionFailed { order });
									Ok::<(), DispatchError>(())
								},
								Some(previous_tick) => {
									Self::inner_update_limit_order_at_tick(
										pool,
										lp,
										asset_pair,
										side,
										id,
										previous_tick,
										IncreaseOrDecrease::Decrease(Amount::MAX),
										NoOpStatus::Allow,
									)?;
									Ok(())
								},
							}
						})?;
					},
					CloseOrder::Range { base_asset, quote_asset, id } => {
						let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
						Self::try_mutate_pool(asset_pair, |asset_pair, pool| {
							match pool
								.range_orders_cache
								.get(lp)
								.and_then(|range_orders| range_orders.get(&id))
							{
								None => {
									Self::deposit_event(Event::<T>::OrderDeletionFailed { order });
									Ok::<(), DispatchError>(())
								},
								Some(previous_tick_range) => {
									Self::inner_update_range_order(
										pool,
										lp,
										asset_pair,
										id,
										previous_tick_range.clone(),
										IncreaseOrDecrease::Decrease(
											range_orders::Size::Liquidity {
												liquidity: Liquidity::MAX,
											},
										),
										NoOpStatus::Allow,
									)?;
									Ok(())
								},
							}
						})?;
					},
				};
			}

			Ok(())
		}

		/// Apply a list of configuration updates to the pallet.
		///
		/// Requires Governance.
		#[pallet::call_index(11)]
		#[pallet::weight(<T as frame_system::Config>::SystemWeightInfo::set_storage(updates.len() as u32))]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			updates: BoundedVec<PalletConfigUpdate, ConstU32<100>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for update in updates {
				match update {
					PalletConfigUpdate::LimitOrderAutoSweepingThreshold { asset, amount } => {
						LimitOrderAutoSweepingThresholds::<T>::mutate(|thresholds| {
							thresholds.try_insert(asset, amount).expect("Every asset will fit");
						});
					},
				}
				Self::deposit_event(Event::<T>::PalletConfigUpdated { update });
			}

			Ok(())
		}
	}
}

impl<T: Config> SwappingApi for Pallet<T> {
	#[transactional]
	fn swap_single_leg(
		from: any::Asset,
		to: any::Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		use cf_amm::math::tick_at_sqrt_price;

		let (asset_pair, order) =
			AssetPair::from_swap(from, to).ok_or(Error::<T>::PoolDoesNotExist)?;
		Self::try_mutate_pool(asset_pair, |_asset_pair, pool| {
			let output_amount = if input_amount == 0 {
				0
			} else {
				let input_amount: Amount = input_amount.into();

				let tick_before = pool
					.pool_state
					.current_price(order)
					.ok_or(Error::<T>::InsufficientLiquidity)?
					.2;
				let (output_amount, _remaining_amount) =
					pool.pool_state.swap(order, input_amount, None);
				let tick_after = pool
					.pool_state
					.current_price(order)
					.ok_or(Error::<T>::InsufficientLiquidity)?
					.2;

				let swap_tick =
					tick_at_sqrt_price(PoolState::<(T::AccountId, OrderId)>::swap_sqrt_price(
						order,
						input_amount,
						output_amount,
					));
				let bounded_swap_tick = if tick_after < tick_before {
					core::cmp::min(core::cmp::max(tick_after, swap_tick), tick_before)
				} else {
					core::cmp::min(core::cmp::max(tick_before, swap_tick), tick_after)
				};

				if let Some(maximum_price_impact) = MaximumPriceImpact::<T>::get(asset_pair) {
					if core::cmp::min(
						bounded_swap_tick.abs_diff(tick_after),
						bounded_swap_tick.abs_diff(tick_before),
					) > maximum_price_impact
					{
						return Err(Error::<T>::InsufficientLiquidity.into());
					}
				}

				output_amount.try_into().map_err(|_| Error::<T>::OutputOverflow)?
			};
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

	fn open_order_count(
		who: &Self::AccountId,
		asset_pair: &PoolPairsMap<Asset>,
	) -> Result<u32, DispatchError> {
		let pool_orders =
			Self::pool_orders(asset_pair.base, asset_pair.quote, Some(who.clone()), true)?;
		Ok(pool_orders.limit_orders.asks.len() as u32 +
			pool_orders.limit_orders.bids.len() as u32 +
			pool_orders.range_orders.len() as u32)
	}

	fn open_order_balances(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		let mut result: AssetMap<AssetAmount> = AssetMap::from_fn(|_| 0);

		for base_asset in Asset::all().filter(|asset| *asset != Asset::Usdc) {
			let pool_orders =
				match Self::pool_orders(base_asset, Asset::Usdc, Some(who.clone()), false) {
					Ok(orders) => orders,
					Err(_) => continue,
				};
			for ask in pool_orders.limit_orders.asks {
				result[base_asset] = result[base_asset]
					.saturating_add(ask.sell_amount.saturated_into::<AssetAmount>());
			}
			for bid in pool_orders.limit_orders.bids {
				result[Asset::Usdc] = result[Asset::Usdc]
					.saturating_add(bid.sell_amount.saturated_into::<AssetAmount>());
			}
			for range_order in pool_orders.range_orders {
				let pair = Self::pool_range_order_liquidity_value(
					base_asset,
					Asset::Usdc,
					range_order.range,
					range_order.liquidity,
				)
				.expect("Cannot fail we are sure the pool exists and the orders too");
				result[base_asset] =
					result[base_asset].saturating_add(pair.base.saturated_into::<AssetAmount>());
				result[Asset::Usdc] =
					result[Asset::Usdc].saturating_add(pair.quote.saturated_into::<AssetAmount>());
			}
		}
		result
	}

	fn pools() -> Vec<PoolPairsMap<Asset>> {
		Pools::<T>::iter_keys().map(|asset_pair| asset_pair.assets()).collect()
	}

	fn cancel_all_limit_orders(account: &Self::AccountId) -> DispatchResult {
		// Collect to avoid undefined behaviour (See StorageMap::iter_keys documentation).
		// Note that we read one pool at a time to optimise memory usage.
		for asset_pair in Pools::<T>::iter_keys().collect::<Vec<_>>() {
			let mut pool = Pools::<T>::get(asset_pair).unwrap();

			for (asset, orders) in pool
				.limit_orders_cache
				.as_ref()
				.into_iter()
				.filter_map(|(asset, cache)| {
					cache.get(account).cloned().map(|orders| (asset, orders))
				})
				.collect::<Vec<_>>()
			{
				for (id, tick) in orders {
					Self::inner_update_limit_order_at_tick(
						&mut pool,
						account,
						&asset_pair,
						asset.sell_order(),
						id,
						tick,
						IncreaseOrDecrease::Decrease(Amount::MAX),
						crate::NoOpStatus::Allow,
					)?;
				}
			}

			Pools::<T>::insert(asset_pair, pool);
		}

		Ok(())
	}

	fn update_limit_order(
		account: &Self::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		option_tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<AssetAmount>,
	) -> DispatchResult {
		Self::inner_update_limit_order(
			account,
			base_asset,
			quote_asset,
			side,
			id,
			option_tick,
			amount_change,
		)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_pool(
		base_asset: Asset,
		quote_asset: Asset,
		fee_hundredth_pips: u32,
		initial_price: cf_primitives::Price,
	) -> DispatchResult {
		Self::create_pool(base_asset, quote_asset, fee_hundredth_pips, initial_price)
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
	pub tick: Tick,
	pub amount: Amount,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Deserialize, Serialize)]
pub struct RangeOrderLiquidity {
	pub tick: Tick,
	pub liquidity: Amount, /* TODO: Change (Using Amount as it is U256 so we get the right
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

pub type PoolPriceV2 = PoolPrice<SqrtPriceQ64F96>;

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolPrice<P> {
	pub sell: Option<P>,
	pub buy: Option<P>,
	pub range_order: SqrtPriceQ64F96,
}

impl<P> PoolPrice<P> {
	pub fn map_sell_and_buy_prices<R>(self, f: impl Fn(P) -> R) -> PoolPrice<R> {
		PoolPrice { sell: self.sell.map(&f), buy: self.buy.map(&f), range_order: self.range_order }
	}
}

#[derive(PartialEq, Eq)]
enum NoOpStatus {
	Allow,
	Error,
}

impl<T: Config> Pallet<T> {
	fn create_pool(
		base_asset: any::Asset,
		quote_asset: any::Asset,
		fee_hundredth_pips: u32,
		initial_price: Price,
	) -> DispatchResult {
		use cf_amm::NewError;

		let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
		Pools::<T>::try_mutate(asset_pair, |maybe_pool| {
			ensure!(maybe_pool.is_none(), Error::<T>::PoolAlreadyExists);

			*maybe_pool = Some(Pool {
				range_orders_cache: Default::default(),
				limit_orders_cache: Default::default(),
				pool_state: PoolState::new(fee_hundredth_pips, initial_price).map_err(
					|e| match e {
						NewError::LimitOrders(limit_orders::NewError::InvalidFeeAmount) =>
							Error::<T>::InvalidFeeAmount,
						NewError::RangeOrders(range_orders::NewError::InvalidFeeAmount) =>
							Error::<T>::InvalidFeeAmount,
						NewError::RangeOrders(range_orders::NewError::InvalidInitialPrice) =>
							Error::<T>::InvalidInitialPrice,
					},
				)?,
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

	fn inner_sweep(lp: &T::AccountId) -> DispatchResult {
		// Collect to avoid undefined behaviour (See StorageMap::iter_keys documentation).
		// Note that we read one pool at a time to optimise memory usage.
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
						NoOpStatus::Error,
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
					Self::sweep_limit_order(
						&mut pool,
						lp,
						&asset_pair,
						assets.sell_order(),
						id,
						tick,
					)?;
				}
			}

			Pools::<T>::insert(asset_pair, pool);
		}

		Ok(())
	}

	fn collect_and_mint_limit_order_with_dispatch_error(
		pool: &mut Pool<T>,
		lp: &T::AccountId,
		side: Side,
		id: OrderId,
		tick: Tick,
		sold_amount: Amount,
		noop_status: NoOpStatus,
	) -> Result<(Collected, PositionInfo), DispatchError> {
		let (collected, position_info) = match pool.pool_state.collect_and_mint_limit_order(
			&(lp.clone(), id),
			side,
			tick,
			sold_amount,
		) {
			Ok(ok) => Ok(ok),
			Err(error) => Err(match error {
				limit_orders::PositionError::NonExistent =>
					if noop_status == NoOpStatus::Allow {
						return Ok(Default::default())
					} else {
						Error::<T>::OrderDoesNotExist
					},
				limit_orders::PositionError::InvalidTick => Error::<T>::InvalidTick,
				limit_orders::PositionError::Other(limit_orders::MintError::MaximumLiquidity) =>
					Error::<T>::MaximumGrossLiquidity,
				limit_orders::PositionError::Other(
					limit_orders::MintError::MaximumPoolInstances,
				) => Error::<T>::MaximumPoolInstances,
			}),
		}?;

		Ok((collected, position_info))
	}

	pub fn inner_set_limit_order(
		lp: &T::AccountId,
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
		Self::try_mutate_order(lp, base_asset, quote_asset, |asset_pair, pool| {
			let tick = match (
				pool.limit_orders_cache[side.to_sold_pair()]
					.get(lp)
					.and_then(|limit_orders| limit_orders.get(&id))
					.copied(),
				option_tick,
			) {
				(None, None) => Err(Error::<T>::UnspecifiedOrderPrice),
				(None, Some(tick)) => Ok(tick),
				(Some(previous_tick), option_new_tick) => {
					Self::inner_update_limit_order_at_tick(
						pool,
						lp,
						asset_pair,
						side,
						id,
						previous_tick,
						IncreaseOrDecrease::Decrease(Amount::MAX),
						NoOpStatus::Error,
					)?;

					Ok(option_new_tick.unwrap_or(previous_tick))
				},
			}?;
			Self::inner_update_limit_order_at_tick(
				pool,
				lp,
				asset_pair,
				side,
				id,
				tick,
				IncreaseOrDecrease::Increase(sell_amount.into()),
				NoOpStatus::Allow,
			)?;

			Ok(())
		})
	}

	/// Updates limit order closing the previous one if necessary (in case of tick change)
	fn inner_update_limit_order(
		lp: &T::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		option_tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<AssetAmount>,
	) -> DispatchResult {
		let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
		Self::inner_sweep(lp)?;
		Self::try_mutate_pool(asset_pair, |asset_pair, pool| {
			let tick = match (
				pool.limit_orders_cache[side.to_sold_pair()]
					.get(lp)
					.and_then(|limit_orders| limit_orders.get(&id))
					.copied(),
				option_tick,
			) {
				(None, None) => Err(Error::<T>::UnspecifiedOrderPrice),
				(None, Some(tick)) | (Some(tick), None) => Ok(tick),
				(Some(previous_tick), Some(new_tick)) => {
					if previous_tick != new_tick {
						let withdrawn_asset_amount = Self::inner_update_limit_order_at_tick(
							pool,
							lp,
							asset_pair,
							side,
							id,
							previous_tick,
							IncreaseOrDecrease::Decrease(Amount::MAX),
							NoOpStatus::Error,
						)?;
						Self::inner_update_limit_order_at_tick(
							pool,
							lp,
							asset_pair,
							side,
							id,
							new_tick,
							IncreaseOrDecrease::Increase(withdrawn_asset_amount.into()),
							NoOpStatus::Allow,
						)?;
					}

					Ok(new_tick)
				},
			}?;
			Self::inner_update_limit_order_at_tick(
				pool,
				lp,
				asset_pair,
				side,
				id,
				tick,
				amount_change.map(|amount| amount.into()),
				NoOpStatus::Error,
			)?;

			Ok(())
		})
	}

	fn sweep_limit_order(
		pool: &mut Pool<T>,
		lp: &T::AccountId,
		asset_pair: &AssetPair,
		side: Side,
		id: OrderId,
		tick: Tick,
	) -> DispatchResult {
		let sold_amount_change = Self::inner_update_limit_order_at_tick(
			pool,
			lp,
			asset_pair,
			side,
			id,
			tick,
			IncreaseOrDecrease::Decrease(Default::default()),
			NoOpStatus::Error,
		)?;

		// We requested no change in the amount we "sell" (sweeping only collects
		// funds in the amount we "buy"), so that's the outcome we expect:
		if sold_amount_change != 0 {
			log_or_panic!("Unexpected sold amount change after sweeping");
		}

		Ok(())
	}

	/// Updates limit order assuming that tick stays the same
	fn inner_update_limit_order_at_tick(
		pool: &mut Pool<T>,
		lp: &T::AccountId,
		asset_pair: &AssetPair,
		side: Side,
		id: OrderId,
		tick: Tick,
		sold_amount_change: IncreaseOrDecrease<Amount>,
		noop_status: NoOpStatus,
	) -> Result<AssetAmount, DispatchError> {
		let (sold_amount_change, position_info, collected) = match sold_amount_change {
			IncreaseOrDecrease::Increase(sold_amount) => {
				let (collected, position_info) =
					Self::collect_and_mint_limit_order_with_dispatch_error(
						pool,
						lp,
						side,
						id,
						tick,
						sold_amount,
						noop_status,
					)?;

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
							if noop_status == NoOpStatus::Allow {
								return Ok(Default::default())
							} else {
								Error::<T>::OrderDoesNotExist
							},
						limit_orders::PositionError::InvalidTick => Error::InvalidTick,
						limit_orders::PositionError::Other(error) => match error {},
					}),
				}?;

				let withdrawn_amount: AssetAmount = sold_amount.try_into()?;
				T::LpBalance::credit_account(
					lp,
					asset_pair.assets()[side.to_sold_pair()],
					withdrawn_amount,
				);

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

	fn inner_update_range_order(
		pool: &mut Pool<T>,
		lp: &T::AccountId,
		asset_pair: &AssetPair,
		id: OrderId,
		tick_range: Range<Tick>,
		size_change: IncreaseOrDecrease<range_orders::Size>,
		noop_status: NoOpStatus,
	) -> Result<(AssetAmounts, Liquidity), DispatchError> {
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
								if noop_status == NoOpStatus::Allow {
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
								cf_amm::range_orders::MintError::AssetRatioUnachievable,
							) => Error::<T>::AssetRatioUnachievable.into(),
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
							if noop_status == NoOpStatus::Allow {
								return Ok(Default::default())
							} else {
								Error::<T>::OrderDoesNotExist
							},
						range_orders::PositionError::Other(e) => match e {
							range_orders::BurnError::AssetRatioUnachievable =>
								Error::<T>::AssetRatioUnachievable,
						},
					}),
				}?;

				let assets_withdrawn = asset_pair.assets().zip(assets_withdrawn).try_map(
					|(asset, amount_withdrawn)| {
						AssetAmount::try_from(amount_withdrawn)
							.map_err(Into::<DispatchError>::into)
							.inspect(|&amount_withdrawn| {
								T::LpBalance::credit_account(lp, asset, amount_withdrawn);
							})
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
						HistoricalEarnedFees::<T>::mutate(lp, asset, |balance| {
							*balance = balance.saturating_add(collected_fees)
						});
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

		Ok((assets_change, *liquidity_change.abs()))
	}

	pub fn try_add_limit_order(
		account_id: &T::AccountId,
		base_asset: any::Asset,
		quote_asset: any::Asset,
		side: Side,
		id: OrderId,
		tick: Tick,
		sell_amount: Amount,
	) -> Result<(), DispatchError> {
		Self::try_mutate_pool(AssetPair::try_new::<T>(base_asset, quote_asset)?, |_, pool| {
			Self::collect_and_mint_limit_order_with_dispatch_error(
				pool,
				account_id,
				side,
				id,
				tick,
				sell_amount,
				NoOpStatus::Error,
			)?;

			Ok(())
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

	fn try_mutate_pools<
		E: From<pallet::Error<T>>,
		F: FnMut(&AssetPair, &mut Pool<T>) -> Result<(), E>,
	>(
		mut f: F,
	) {
		for asset_pair in Pools::<T>::iter_keys().collect::<Vec<_>>() {
			let _ = Pools::<T>::try_mutate(asset_pair, |maybe_pool| {
				let pool =
					maybe_pool.as_mut().expect("Pools must exist since we are iterating over them");

				f(&asset_pair, pool)
			});
		}
	}

	fn try_mutate_order<R, F: FnOnce(&AssetPair, &mut Pool<T>) -> Result<R, DispatchError>>(
		lp: &T::AccountId,
		base_asset: any::Asset,
		quote_asset: any::Asset,
		f: F,
	) -> Result<R, DispatchError> {
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

	pub fn pool_price(
		base_asset: Asset,
		quote_asset: Asset,
	) -> Result<PoolPrice<PoolPriceV1>, DispatchError> {
		let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
		let mut pool = Pools::<T>::get(asset_pair).ok_or(Error::<T>::PoolDoesNotExist)?;
		Ok(PoolPrice {
			sell: pool
				.pool_state
				.current_price(Side::Sell)
				.map(|(price, sqrt_price, tick)| PoolPriceV1 { price, sqrt_price, tick }),
			buy: pool
				.pool_state
				.current_price(Side::Buy)
				.map(|(price, sqrt_price, tick)| PoolPriceV1 { price, sqrt_price, tick }),
			range_order: pool.pool_state.current_range_order_pool_price(),
		})
	}

	pub fn required_asset_ratio_for_range_order(
		base_asset: any::Asset,
		quote_asset: any::Asset,
		tick_range: Range<Tick>,
	) -> Result<PoolPairsMap<Amount>, DispatchError> {
		let pool_state = Pools::<T>::get(AssetPair::try_new::<T>(base_asset, quote_asset)?)
			.ok_or(Error::<T>::PoolDoesNotExist)?
			.pool_state;

		pool_state.required_asset_ratio_for_range_order(tick_range).map_err(|error| {
			match error {
				range_orders::RequiredAssetRatioError::InvalidTickRange =>
					Error::<T>::InvalidTickRange,
			}
			.into()
		})
	}

	pub fn pool_orderbook(
		base_asset: any::Asset,
		quote_asset: any::Asset,
		orders: u32,
	) -> Result<PoolOrderbook, DispatchError> {
		let orders = orders.clamp(1, 16384);

		let asset_pair = AssetPair::try_new::<T>(base_asset, quote_asset)?;
		let pool_state =
			Pools::<T>::get(asset_pair).ok_or(Error::<T>::PoolDoesNotExist)?.pool_state;

		// TODO: Need to change limit order pool implementation so Amount::MAX is guaranteed to
		// drain pool (so the calculated amounts here are guaranteed to reflect the accurate
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
								sqrt_price: bounded_sqrt_price(
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
								sqrt_price: bounded_sqrt_price(
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
		tick_range: Range<Tick>,
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
		filled_orders: bool,
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
						if filled_orders || !position_info.amount.is_zero() {
							Some(LimitOrder {
								lp: lp.clone(),
								id: id.into(),
								tick,
								sell_amount: position_info.amount,
								fees_earned: collected.accumulative_fees,
								original_sell_amount: collected.original_amount,
							})
						} else {
							None
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
	}

	/// Process changes to limit order:
	/// - Payout collected `fee` and `bought_amount`
	/// - Update cache storage for Pool
	/// - Deposit the correct event.
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
		HistoricalEarnedFees::<T>::mutate(lp, asset, |balance| {
			*balance = balance.saturating_add(collected_fees)
		});
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

	fn auto_sweep_limit_orders() {
		// Auto-sweeping limit orders in case collected amount reaches a threshold:
		let autosweeping_thresholds = LimitOrderAutoSweepingThresholds::<T>::get();

		Self::try_mutate_pools(|asset_pair, pool| {
			let collected_orders = {
				// Clone pool since we don't actually want to make changes
				// to it yet:
				let mut pool_state = pool.pool_state.clone();
				pool_state.collect_all_limit_orders()
			};

			for (base_or_quote, results_for_order) in collected_orders {
				for ((lp, order_id), tick, collected, _pos_info) in results_for_order {
					let asset_to_collect = asset_pair.assets()[!base_or_quote];

					let threshold = autosweeping_thresholds
						.get(&asset_to_collect)
						.copied()
						// Default to not sweeping
						.unwrap_or(AssetAmount::MAX)
						.into();

					if collected.bought_amount >= threshold {
						Self::sweep_limit_order(
							pool,
							&lp,
							asset_pair,
							base_or_quote.sell_order(),
							order_id,
							tick,
						)?;
					}
				}
			}

			Ok::<_, DispatchError>(())
		});
	}
}

pub struct DeleteHistoricalEarnedFees<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnKilledAccount<T::AccountId> for DeleteHistoricalEarnedFees<T> {
	fn on_killed_account(who: &T::AccountId) {
		let _ = HistoricalEarnedFees::<T>::clear_prefix(who, u32::MAX, None);
	}
}

impl<T: Config> LpOrdersWeightsProvider for Pallet<T> {
	fn update_limit_order_weight() -> Weight {
		T::WeightInfo::update_limit_order()
	}
}

impl<T: Config> cf_traits::PoolPriceProvider for Pallet<T> {
	fn pool_price(
		base_asset: Asset,
		quote_asset: Asset,
	) -> Result<cf_traits::PoolPrice, DispatchError> {
		use cf_amm::math::sqrt_price_to_price;

		// NOTE: we can default to max price because None is only ever returned by
		// Self::pool_price when the range order is at its maximum tick (irrespective
		// of whether the pool has liquidity)
		Self::pool_price(base_asset, quote_asset).map(|price| cf_traits::PoolPrice {
			sell: price
				.sell
				.map(|p| p.price)
				.unwrap_or_else(|| sqrt_price_to_price(MAX_SQRT_PRICE)),
			buy: price
				.buy
				.map(|p| p.price)
				.unwrap_or_else(|| sqrt_price_to_price(MAX_SQRT_PRICE)),
		})
	}
}
