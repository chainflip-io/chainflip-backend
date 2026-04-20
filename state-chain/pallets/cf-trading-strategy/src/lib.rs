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

pub mod migrations;

pub mod weights;

mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[macro_use]
extern crate proptest;

use cf_amm::common::AssetPair;
use cf_primitives::{Asset, AssetAmount, OrderId, StablecoinDefaults, Tick, STABLE_ASSET};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AccountRoleRegistry, BalanceApi, Chainflip, DeregistrationCheck,
	IncreaseOrDecrease, LpOrdersWeightsProvider, LpRegistration, PoolApi, PriceFeedApi, Side,
};

use frame_support::{
	pallet_prelude::*,
	sp_runtime::{traits::One, FixedU64, Permill},
	traits::HandleLifetime,
};
use frame_system::{pallet_prelude::*, WeightInfo as SystemWeightInfo};
use sp_std::{
	collections::{
		btree_map::{self, BTreeMap},
		btree_set::BTreeSet,
	},
	vec::Vec,
};
use weights::WeightInfo;

pub use pallet::*;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

// Note that strategies can only create a limited number of orders per asset/side so we can just
// have fixed order ids (at least until we develop more advanced strategies).
const STRATEGY_ORDER_ID_0: OrderId = 0;
const STRATEGY_ORDER_ID_1: OrderId = 1;

impl_pallet_safe_mode!(PalletSafeMode; strategy_updates_enabled, strategy_closure_enabled, strategy_execution_enabled);

#[derive(Debug, PartialEq, Eq)]
pub struct StrategyLimitOrder<AccountId> {
	pub base_asset: Asset,
	pub quote_asset: Asset,
	pub account_id: AccountId,
	pub side: Side,
	pub order_id: OrderId,
	pub tick: Tick,
	pub amount: AssetAmount,
}

#[derive(
	Clone,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	serde::Serialize,
	serde::Deserialize,
	PartialEq,
	Eq,
)]
pub enum TradingStrategy {
	TickZeroCentered {
		spread_tick: Tick,
		base_asset: Asset,
	},
	SimpleBuySell {
		buy_tick: Tick,
		sell_tick: Tick,
		base_asset: Asset,
	},
	InventoryBased {
		min_buy_tick: Tick,
		max_buy_tick: Tick,
		min_sell_tick: Tick,
		max_sell_tick: Tick,
		base_asset: Asset,
	},
	OracleTracking {
		min_buy_offset_tick: Tick,
		max_buy_offset_tick: Tick,
		min_sell_offset_tick: Tick,
		max_sell_offset_tick: Tick,
		base_asset: Asset,
		quote_asset: Asset,
	},
}

#[derive(
	Clone,
	RuntimeDebugNoBound,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum PalletConfigUpdate {
	MinimumDeploymentAmountForStrategy { asset: Asset, amount: Option<AssetAmount> },
	MinimumAddedFundsToStrategy { asset: Asset, amount: Option<AssetAmount> },
	LimitOrderUpdateThreshold { asset: Asset, amount: AssetAmount },
}

impl TradingStrategy {
	pub fn validate_funding<T: Config>(
		&self,
		amounts: &BTreeMap<Asset, AssetAmount>,
	) -> Result<(), Error<T>> {
		if amounts.is_empty() {
			return Err(Error::<T>::InvalidAssetsForStrategy);
		}
		let supported_assets = self.supported_assets();
		if amounts.keys().all(|asset| supported_assets.contains(asset)) {
			Ok(())
		} else {
			Err(Error::<T>::InvalidAssetsForStrategy)
		}
	}

	pub fn supported_assets(&self) -> BTreeSet<Asset> {
		match self {
			TradingStrategy::TickZeroCentered { base_asset, .. } |
			TradingStrategy::SimpleBuySell { base_asset, .. } |
			TradingStrategy::InventoryBased { base_asset, .. } =>
				BTreeSet::from_iter([*base_asset, STABLE_ASSET]),
			TradingStrategy::OracleTracking { base_asset, quote_asset, .. } =>
				BTreeSet::from_iter([*base_asset, *quote_asset]),
		}
	}

	/// Returns the asset pair this strategy operates on.
	pub fn asset_pair(&self) -> Option<AssetPair> {
		match self {
			TradingStrategy::TickZeroCentered { base_asset, .. } |
			TradingStrategy::SimpleBuySell { base_asset, .. } |
			TradingStrategy::InventoryBased { base_asset, .. } => AssetPair::new(*base_asset, STABLE_ASSET),
			TradingStrategy::OracleTracking { base_asset, quote_asset, .. } =>
				AssetPair::new(*base_asset, *quote_asset),
		}
	}

	/// Whether this strategy needs existing pool orders fetched before execution.
	fn needs_order_prefetch(&self) -> bool {
		matches!(
			self,
			TradingStrategy::InventoryBased { .. } | TradingStrategy::OracleTracking { .. }
		)
	}

	/// Upper-bound weight estimate for one execution of this strategy.
	fn max_weight_estimate(&self, limit_order_update_weight: Weight) -> Weight {
		match self {
			TradingStrategy::TickZeroCentered { .. } | TradingStrategy::SimpleBuySell { .. } =>
				limit_order_update_weight * 2,
			TradingStrategy::InventoryBased { .. } | TradingStrategy::OracleTracking { .. } =>
				limit_order_update_weight * 4,
		}
	}

	fn validate_params<T: Config>(&self) -> Result<(), Error<T>> {
		match self {
			TradingStrategy::TickZeroCentered { spread_tick, base_asset } => {
				if *spread_tick < 0 || *spread_tick > cf_amm_math::MAX_TICK {
					return Err(Error::<T>::InvalidTick)
				}
				ensure!(*base_asset != STABLE_ASSET, Error::<T>::InvalidAssetsForStrategy);
			},
			TradingStrategy::SimpleBuySell { buy_tick, sell_tick, base_asset } => {
				if buy_tick >= sell_tick ||
					*buy_tick > cf_amm_math::MAX_TICK ||
					*sell_tick > cf_amm_math::MAX_TICK ||
					*buy_tick < cf_amm_math::MIN_TICK ||
					*sell_tick < cf_amm_math::MIN_TICK
				{
					return Err(Error::<T>::InvalidTick)
				}
				ensure!(*base_asset != STABLE_ASSET, Error::<T>::InvalidAssetsForStrategy);
			},
			TradingStrategy::InventoryBased {
				min_buy_tick,
				max_buy_tick,
				min_sell_tick,
				max_sell_tick,
				base_asset,
			} => {
				validate_inventory_tick_ranges::<T>(
					*min_buy_tick,
					*max_buy_tick,
					*min_sell_tick,
					*max_sell_tick,
				)?;
				ensure!(*base_asset != STABLE_ASSET, Error::<T>::InvalidAssetsForStrategy);
			},
			TradingStrategy::OracleTracking {
				min_buy_offset_tick,
				max_buy_offset_tick,
				min_sell_offset_tick,
				max_sell_offset_tick,
				base_asset,
				quote_asset,
			} => {
				validate_inventory_tick_ranges::<T>(
					*min_buy_offset_tick,
					*max_buy_offset_tick,
					*min_sell_offset_tick,
					*max_sell_offset_tick,
				)?;
				ensure!(*base_asset != STABLE_ASSET, Error::<T>::InvalidAssetsForStrategy);
				ensure!(*quote_asset == STABLE_ASSET, Error::<T>::InvalidAssetsForStrategy);
				ensure!(
					T::PriceFeedApi::get_relative_price(*base_asset, *quote_asset).is_some(),
					Error::<T>::InvalidAssetsForStrategy
				);
			},
		}
		Ok(())
	}
}

fn derive_strategy_id<T: Config>(lp: &T::AccountId) -> T::AccountId {
	use frame_support::{sp_runtime::traits::TrailingZeroInput, Hashable};

	let nonce = frame_system::Pallet::<T>::account_nonce(lp);
	// Combination of lp + nonce is unique for every successful call, so this should
	// generate unique ids:
	Decode::decode(&mut TrailingZeroInput::new(
		(*b"chainflip/strategy_account", lp.clone(), nonce).blake2_256().as_ref(),
	))
	.unwrap()
}

#[frame_support::pallet]
pub mod pallet {
	use frame_support::sp_runtime::SaturatedConversion;

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		type BalanceApi: BalanceApi<AccountId = Self::AccountId>;

		/// LP address registration and verification.
		type LpRegistrationApi: LpRegistration<AccountId = Self::AccountId>;

		type PoolApi: PoolApi<AccountId = Self::AccountId>;

		type PriceFeedApi: PriceFeedApi;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
		type LpOrdersWeights: LpOrdersWeightsProvider;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	// Stores all deployed strategies by the liquidity provider's id (owner) and
	// the strategy id.
	#[pallet::storage]
	pub type Strategies<T: Config> = StorageDoubleMap<
		_,
		Identity,
		T::AccountId,
		Identity,
		T::AccountId,
		TradingStrategy,
		OptionQuery,
	>;

	/// Stores thresholds in terms of the asset amount, used to determine whether a trading strategy
	/// for a given asset has enough funds in "free balance" to make it worthwhile
	/// updating/creating a limit order with them. Note that we use store map as a single value
	/// since it is often more convenient to read multiple assets at once (and this map is small).
	/// An asset that is not in this map is disabled from being updated.
	#[pallet::storage]
	pub type LimitOrderUpdateThresholds<T: Config> = StorageValue<
		_,
		BTreeMap<Asset, AssetAmount>,
		ValueQuery,
		StablecoinDefaults<1_000_000_000>, // $1,000 USD
	>;

	/// Stores minimum amount per asset necessary to deploy a strategy if only one of the
	/// assets is provided. If more then one asset is provided, we allow splitting the requirement
	/// between them: e.g. it is possible to start a strategy with only 30% of the required amount
	/// of asset A, as long as there is at least 70% of the required amount of asset B.
	/// An asset that is not in this map is disabled from being deployed.
	#[pallet::storage]
	pub type MinimumDeploymentAmountForStrategy<T: Config> = StorageValue<
		_,
		BTreeMap<Asset, AssetAmount>,
		ValueQuery,
		StablecoinDefaults<20_000_000_000>, // $20,000 USD
	>;

	/// Stores the minimum amount per asset that can be added to an existing strategy.
	/// An asset that is not in this map is disabled from adding funds.
	#[pallet::storage]
	pub type MinimumAddedFundsToStrategy<T: Config> = StorageValue<
		_,
		BTreeMap<Asset, AssetAmount>,
		ValueQuery,
		StablecoinDefaults<10_000_000>, // $10 USD
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_current_block: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut weight_used: Weight = Weight::zero();

			// We assume this consumes 0 weight since safe mode is likely in cache
			if !T::SafeMode::get().strategy_execution_enabled {
				return weight_used
			}

			weight_used += T::DbWeight::get().reads(1);
			let order_update_thresholds = LimitOrderUpdateThresholds::<T>::get();

			weight_used += T::DbWeight::get().reads(1);
			let limit_order_update_weight = T::LpOrdersWeights::update_limit_order_weight();

			// Cache of orders per asset to avoid redundant reads
			let mut order_cache: BTreeMap<Asset, Vec<StrategyLimitOrder<T::AccountId>>> =
				BTreeMap::new();

			// Pass 1: collect strategy accounts per asset pair so we can batch pool order queries.
			let mut fetch_orders_for_strategies: BTreeMap<AssetPair, BTreeSet<T::AccountId>> =
				BTreeMap::new();
			Strategies::<T>::iter().for_each(|(_, strategy_id, strategy)| {
				if strategy.needs_order_prefetch() {
					if let Some(asset_pair) = strategy.asset_pair() {
						fetch_orders_for_strategies
							.entry(asset_pair)
							.or_insert_with(BTreeSet::new)
							.insert(strategy_id);
					}
				}
			});

			// Pass 2: execute each strategy.
			for (_, strategy_id, strategy) in Strategies::<T>::iter() {
				let new_weight_estimate = weight_used
					.saturating_add(strategy.max_weight_estimate(limit_order_update_weight));
				if remaining_weight.checked_sub(&new_weight_estimate).is_none() {
					break;
				}

				// Populate the order cache for strategies that require existing pool orders.
				let existing_orders: Vec<&StrategyLimitOrder<T::AccountId>> = if strategy
					.needs_order_prefetch()
				{
					let Some(asset_pair) = strategy.asset_pair() else {
						log_or_panic!(
							"Failed to determine asset pair for strategy {:?}, skipping",
							strategy_id
						);
						continue;
					};
					let base_asset = asset_pair.base();
					let quote_asset = asset_pair.quote();
					if let btree_map::Entry::Vacant(entry) = order_cache.entry(base_asset) {
						weight_used += T::DbWeight::get().reads(1);
						match T::PoolApi::limit_orders(
							base_asset,
							quote_asset,
							fetch_orders_for_strategies
								.get(&asset_pair)
								.unwrap_or(&BTreeSet::new()),
						) {
							Ok(pool_orders) => {
								entry.insert(
									pool_orders
										.into_iter()
										.map(|(side, order)| StrategyLimitOrder {
											base_asset,
											quote_asset,
											account_id: order.lp,
											side,
											order_id: order.id.saturated_into(),
											tick: order.tick,
											amount: order.sell_amount.saturated_into(),
										})
										.collect(),
								);
							},
							Err(e) => {
								log_or_panic!(
									"Failed to get limit orders for asset {:?}: {:?}",
									base_asset,
									e
								);
								continue;
							},
						}
					}

					// Grab the orders for this strategy.
					order_cache
						.get(&asset_pair.base())
						.map(|orders| {
							orders.iter().filter(|order| order.account_id == strategy_id).collect()
						})
						.unwrap_or_default()
				} else {
					Default::default()
				};

				Self::execute_strategy(
					&strategy,
					&strategy_id,
					&existing_orders,
					&order_update_thresholds,
					limit_order_update_weight,
					&mut weight_used,
				);
			}

			weight_used
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		StrategyDeployed {
			account_id: T::AccountId,
			strategy_id: T::AccountId,
			strategy: TradingStrategy,
		},
		FundsAddedToStrategy {
			strategy_id: T::AccountId,
			amounts: BTreeMap<Asset, AssetAmount>,
		},
		StrategyClosed {
			strategy_id: T::AccountId,
		},
		PalletConfigUpdated {
			update: PalletConfigUpdate,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		StrategyNotFound,
		AmountBelowDeploymentThreshold,
		AmountBelowAddedFundsThreshold,
		InvalidAssetsForStrategy,
		InvalidTick,
		/// The liquidity provider has active strategies and cannot be deregistered.
		LpHasActiveStrategies,
		/// Strategies are disabled due to safe mode
		TradingStrategiesDisabled,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::deploy_strategy())]
		pub fn deploy_strategy(
			origin: OriginFor<T>,
			strategy: TradingStrategy,
			funding: BTreeMap<Asset, AssetAmount>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().strategy_updates_enabled,
				Error::<T>::TradingStrategiesDisabled
			);

			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			// Check that the LP has a refund address for each asset
			for asset in strategy.supported_assets() {
				T::LpRegistrationApi::ensure_has_refund_address_for_asset(lp, asset)?;
			}

			strategy.validate_params::<T>()?;

			let strategy_id = {
				// Check that strategy is created with sufficient funds:
				ensure!(
					Self::validate_minimum_funding(
						&strategy,
						&funding,
						&MinimumDeploymentAmountForStrategy::<T>::get(),
					)
					.ok_or(Error::<T>::InvalidAssetsForStrategy)?,
					Error::<T>::AmountBelowDeploymentThreshold
				);

				let strategy_id = derive_strategy_id::<T>(lp);

				if !frame_system::Pallet::<T>::account_exists(&strategy_id) {
					let _ = frame_system::Provider::<T>::created(&strategy_id);
				}

				Self::deposit_event(Event::<T>::StrategyDeployed {
					account_id: lp.clone(),
					strategy_id: strategy_id.clone(),
					strategy: strategy.clone(),
				});

				Strategies::<T>::insert(lp, strategy_id.clone(), strategy.clone());

				strategy_id
			};

			Self::add_funds_to_existing_strategy(lp, &strategy_id, strategy, funding)
		}

		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::close_strategy())]
		pub fn close_strategy(origin: OriginFor<T>, strategy_id: T::AccountId) -> DispatchResult {
			ensure!(
				T::SafeMode::get().strategy_closure_enabled,
				Error::<T>::TradingStrategiesDisabled
			);

			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let strategy =
				Strategies::<T>::take(lp, &strategy_id).ok_or(Error::<T>::StrategyNotFound)?;

			T::PoolApi::cancel_all_limit_orders(&strategy_id)?;

			for asset in strategy.supported_assets() {
				let balance = T::BalanceApi::get_balance(&strategy_id, asset);
				T::BalanceApi::transfer(&strategy_id, lp, asset, balance)?;
			}

			frame_system::Provider::<T>::killed(&strategy_id).unwrap_or_else(|e| {
				// This shouldn't happen, and not much we can do if it does except fix it on a
				// subsequent release. Consequences are minor.
				log::error!(
					"Unexpected reference count error while closing a strategy {:?}: {:?}.",
					strategy_id,
					e
				);
			});

			Self::deposit_event(Event::<T>::StrategyClosed { strategy_id: strategy_id.clone() });

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::add_funds_to_strategy())]
		pub fn add_funds_to_strategy(
			origin: OriginFor<T>,
			strategy_id: T::AccountId,
			funding: BTreeMap<Asset, AssetAmount>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().strategy_updates_enabled,
				Error::<T>::TradingStrategiesDisabled
			);
			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let strategy =
				Strategies::<T>::get(lp, &strategy_id).ok_or(Error::<T>::StrategyNotFound)?;

			ensure!(
				Self::validate_minimum_funding(
					&strategy,
					&funding,
					&MinimumAddedFundsToStrategy::<T>::get(),
				)
				.ok_or(Error::<T>::InvalidAssetsForStrategy)?,
				Error::<T>::AmountBelowAddedFundsThreshold
			);

			Self::add_funds_to_existing_strategy(lp, &strategy_id, strategy, funding)
		}

		/// Apply a list of configuration updates to the pallet.
		///
		/// Requires Governance.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as frame_system::Config>::SystemWeightInfo::set_storage(updates.len() as u32))]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			updates: BoundedVec<PalletConfigUpdate, ConstU32<100>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for update in updates {
				match update {
					PalletConfigUpdate::MinimumDeploymentAmountForStrategy { asset, amount } => {
						MinimumDeploymentAmountForStrategy::<T>::mutate(|thresholds| {
							if let Some(amount) = amount {
								thresholds.insert(asset, amount);
							} else {
								thresholds.remove(&asset);
							}
						});
					},
					PalletConfigUpdate::MinimumAddedFundsToStrategy { asset, amount } => {
						MinimumAddedFundsToStrategy::<T>::mutate(|thresholds| {
							if let Some(amount) = amount {
								thresholds.insert(asset, amount);
							} else {
								thresholds.remove(&asset);
							}
						});
					},
					PalletConfigUpdate::LimitOrderUpdateThreshold { asset, amount } => {
						LimitOrderUpdateThresholds::<T>::mutate(|thresholds| {
							thresholds.insert(asset, amount);
						});
					},
				}
				Self::deposit_event(Event::<T>::PalletConfigUpdated { update });
			}

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn add_funds_to_existing_strategy(
		lp: &T::AccountId,
		strategy_id: &T::AccountId,
		strategy: TradingStrategy,
		funding: BTreeMap<Asset, AssetAmount>,
	) -> DispatchResult {
		strategy.validate_funding::<T>(&funding)?;

		for (asset, amount) in &funding {
			T::BalanceApi::transfer(lp, strategy_id, *asset, *amount)?;
		}

		Self::deposit_event(Event::<T>::FundsAddedToStrategy {
			strategy_id: strategy_id.clone(),
			amounts: funding.into_iter().collect(),
		});

		Ok(())
	}

	fn validate_minimum_funding(
		strategy: &TradingStrategy,
		funding: &BTreeMap<Asset, AssetAmount>,
		minimum: &BTreeMap<Asset, AssetAmount>,
	) -> Option<bool> {
		// Fail if any of the strategies assets do not have a minimum amount set
		if !strategy
			.supported_assets()
			.into_iter()
			.all(|asset| minimum.contains_key(&asset))
		{
			return None
		}

		// Check if the funding contains an unsupported asset
		if funding.is_empty() ||
			funding.keys().any(|asset| !strategy.supported_assets().contains(asset))
		{
			return None
		}

		Some(
			strategy.supported_assets().into_iter().fold(FixedU64::default(), |acc, asset| {
				let min_required = *minimum.get(&asset).expect("checked above");
				let funds = *funding.get(&asset).unwrap_or(&0);
				let fraction_of_required = if funds >= min_required {
					FixedU64::one()
				} else {
					FixedU64::from_rational(funds, min_required)
				};
				acc + fraction_of_required
			}) >= FixedU64::one(),
		)
	}

	/// Dispatch execution to the appropriate per-strategy function.
	fn execute_strategy(
		strategy: &TradingStrategy,
		strategy_id: &T::AccountId,
		existing_orders: &[&StrategyLimitOrder<T::AccountId>],
		thresholds: &BTreeMap<Asset, AssetAmount>,
		limit_order_update_weight: Weight,
		weight_used: &mut Weight,
	) {
		match strategy {
			TradingStrategy::TickZeroCentered { spread_tick, base_asset } =>
				Self::execute_simple_order(
					*base_asset,
					-spread_tick,
					*spread_tick,
					strategy_id,
					thresholds,
					limit_order_update_weight,
					weight_used,
				),
			TradingStrategy::SimpleBuySell { buy_tick, sell_tick, base_asset } =>
				Self::execute_simple_order(
					*base_asset,
					*buy_tick,
					*sell_tick,
					strategy_id,
					thresholds,
					limit_order_update_weight,
					weight_used,
				),
			TradingStrategy::InventoryBased {
				min_buy_tick,
				max_buy_tick,
				min_sell_tick,
				max_sell_tick,
				base_asset,
			} => Self::execute_inventory_based(
				*base_asset,
				*min_buy_tick,
				*max_buy_tick,
				*min_sell_tick,
				*max_sell_tick,
				strategy_id,
				existing_orders,
				thresholds,
				limit_order_update_weight,
				weight_used,
			),
			TradingStrategy::OracleTracking {
				min_buy_offset_tick,
				max_buy_offset_tick,
				min_sell_offset_tick,
				max_sell_offset_tick,
				base_asset,
				quote_asset,
			} => Self::execute_oracle_tracking(
				*base_asset,
				*quote_asset,
				*min_buy_offset_tick,
				*max_buy_offset_tick,
				*min_sell_offset_tick,
				*max_sell_offset_tick,
				strategy_id,
				existing_orders,
				thresholds,
				limit_order_update_weight,
				weight_used,
			),
		}
	}

	/// Execute a simple fixed-tick strategy (TickZeroCentered or SimpleBuySell).
	///
	/// For each side, if the free balance exceeds the threshold, place or update a single limit
	/// order at the given tick with the full free balance.
	fn execute_simple_order(
		base_asset: Asset,
		buy_tick: Tick,
		sell_tick: Tick,
		strategy_id: &T::AccountId,
		thresholds: &BTreeMap<Asset, AssetAmount>,
		limit_order_update_weight: Weight,
		weight_used: &mut Weight,
	) {
		for (side, tick) in [(Side::Buy, buy_tick), (Side::Sell, sell_tick)] {
			let sell_asset = if side == Side::Buy { STABLE_ASSET } else { base_asset };

			*weight_used += T::DbWeight::get().reads(1);
			let balance = T::BalanceApi::get_balance(strategy_id, sell_asset);

			// Minimum threshold of 1 to prevent updating with 0 amounts
			let threshold =
				core::cmp::max(thresholds.get(&sell_asset).copied().unwrap_or(u128::MAX), 1);

			if balance >= threshold {
				*weight_used += limit_order_update_weight;

				// We expect this to fail if the pool does not exist
				let _result = T::PoolApi::update_limit_order(
					strategy_id,
					base_asset,
					STABLE_ASSET,
					side,
					STRATEGY_ORDER_ID_0,
					Some(tick),
					IncreaseOrDecrease::Increase(balance),
				);
			}
		}
	}

	/// Execute the InventoryBased strategy.
	///
	/// Uses fixed ticks (no oracle offset) and native asset amounts (no USD conversion).
	fn execute_inventory_based(
		base_asset: Asset,
		min_buy_tick: Tick,
		max_buy_tick: Tick,
		min_sell_tick: Tick,
		max_sell_tick: Tick,
		strategy_id: &T::AccountId,
		existing_orders: &[&StrategyLimitOrder<T::AccountId>],
		thresholds: &BTreeMap<Asset, AssetAmount>,
		limit_order_update_weight: Weight,
		weight_used: &mut Weight,
	) {
		Self::execute_with_inventory_logic(
			base_asset,
			STABLE_ASSET,
			min_buy_tick,
			max_buy_tick,
			min_sell_tick,
			max_sell_tick,
			Tick::from(0), // no oracle offset
			false,         // no USD conversion
			false,         // ticks are fixed, no need to check for tick changes
			strategy_id,
			existing_orders,
			thresholds,
			limit_order_update_weight,
			weight_used,
		);
	}

	/// Execute the OracleTracking strategy.
	///
	/// Fetches the current oracle price to use as a tick offset, then delegates to the shared
	/// inventory logic with USD conversion and tick-change detection enabled. Cancels all open
	/// orders and returns early if the oracle price is stale or unavailable.
	fn execute_oracle_tracking(
		base_asset: Asset,
		quote_asset: Asset,
		min_buy_offset_tick: Tick,
		max_buy_offset_tick: Tick,
		min_sell_offset_tick: Tick,
		max_sell_offset_tick: Tick,
		strategy_id: &T::AccountId,
		existing_orders: &[&StrategyLimitOrder<T::AccountId>],
		thresholds: &BTreeMap<Asset, AssetAmount>,
		limit_order_update_weight: Weight,
		weight_used: &mut Weight,
	) {
		*weight_used += T::DbWeight::get().reads(1);
		let oracle_tick_opt = match T::PriceFeedApi::get_relative_price(base_asset, quote_asset) {
			None => {
				log_or_panic!(
					"Failed to get oracle price for asset {:?}, skipping strategy {:?}",
					base_asset,
					strategy_id
				);
				None
			},
			Some(oracle) if oracle.stale => None,
			Some(oracle) => oracle.price.into_tick(),
		};

		let relative_tick = match oracle_tick_opt {
			Some(tick) => tick,
			None => {
				// Stale or unavailable price: cancel all open orders and skip.
				let _res = T::PoolApi::cancel_all_limit_orders(strategy_id);
				return;
			},
		};

		Self::execute_with_inventory_logic(
			base_asset,
			quote_asset,
			min_buy_offset_tick,
			max_buy_offset_tick,
			min_sell_offset_tick,
			max_sell_offset_tick,
			relative_tick,
			true, // convert balances to USD for cross-asset comparison
			true, // update orders when oracle price moves the ticks
			strategy_id,
			existing_orders,
			thresholds,
			limit_order_update_weight,
			weight_used,
		);
	}

	/// Shared execution core for InventoryBased and OracleTracking strategies.
	///
	/// Determines whether orders need updating (due to free balance exceeding the threshold, or
	/// because `check_tick_changes` is set and the oracle has moved the ticks), cancels existing
	/// orders, and places new ones according to the inventory-based logic.
	fn execute_with_inventory_logic(
		base_asset: Asset,
		quote_asset: Asset,
		min_buy_tick: Tick,
		max_buy_tick: Tick,
		min_sell_tick: Tick,
		max_sell_tick: Tick,
		relative_tick: Tick,
		convert_to_usd: bool,
		check_tick_changes: bool,
		strategy_id: &T::AccountId,
		existing_orders: &[&StrategyLimitOrder<T::AccountId>],
		thresholds: &BTreeMap<Asset, AssetAmount>,
		limit_order_update_weight: Weight,
		weight_used: &mut Weight,
	) {
		use frame_support::sp_runtime::SaturatedConversion;

		// This relies on autosweeping for limit orders
		let orders_total_quote: AssetAmount = existing_orders
			.iter()
			.map(|order| if order.side == Side::Buy { order.amount } else { 0 })
			.sum();
		let orders_total_base: AssetAmount = existing_orders
			.iter()
			.map(|order| if order.side == Side::Sell { order.amount } else { 0 })
			.sum();

		// Get the free balance
		let quote_balance_asset = T::BalanceApi::get_balance(strategy_id, quote_asset);
		let base_balance_asset = T::BalanceApi::get_balance(strategy_id, base_asset);
		let total_quote_asset = quote_balance_asset.saturating_add(orders_total_quote);
		let total_base_asset = base_balance_asset.saturating_add(orders_total_base);
		*weight_used += T::DbWeight::get().reads(2);

		// Minimum threshold of 1 to prevent updating with 0 amounts
		let base_threshold =
			core::cmp::max(thresholds.get(&base_asset).copied().unwrap_or(u128::MAX), 1);
		let quote_threshold =
			core::cmp::max(thresholds.get(&quote_asset).copied().unwrap_or(u128::MAX), 1);

		let (update_due_to_balance, total_quote, total_base) = if convert_to_usd {
			// Convert to USD amounts so we can compare assets with different decimals.
			let usd_value_of = |asset, amount, default| {
				T::PriceFeedApi::get_price(asset)
					.map(|oracle| oracle.price.output_amount_ceil(amount).saturated_into())
					.unwrap_or(default)
			};

			let quote_balance_usd = usd_value_of(quote_asset, quote_balance_asset, 0);
			let base_balance_usd = usd_value_of(base_asset, base_balance_asset, 0);
			let total_quote_usd = usd_value_of(quote_asset, total_quote_asset, 0);
			let total_base_usd = usd_value_of(base_asset, total_base_asset, 0);

			let quote_threshold_usd = usd_value_of(quote_asset, quote_threshold, u128::MAX);
			let base_threshold_usd = usd_value_of(base_asset, base_threshold, u128::MAX);

			let update_due_to_balance =
				quote_balance_usd + base_balance_usd >= base_threshold_usd.min(quote_threshold_usd);
			(update_due_to_balance, total_quote_usd, total_base_usd)
		} else {
			// For the inventory based strategy, we require both assets to be
			// equivalent (have similar prices and the same number of decimals).
			let update_due_to_balance =
				quote_balance_asset + base_balance_asset >= base_threshold.min(quote_threshold);
			(update_due_to_balance, total_quote_asset, total_base_asset)
		};

		// Use the balance of assets to calculate the desired limit orders
		let total = total_quote.saturating_add(total_base);
		let new_orders: Vec<_> = inventory_based_strategy_logic(
			total_quote,
			total,
			relative_tick + min_buy_tick,
			relative_tick + max_buy_tick,
			Side::Buy,
			strategy_id.clone(),
			base_asset,
			quote_asset,
		)
		.into_iter()
		.chain(inventory_based_strategy_logic(
			total_base,
			total,
			relative_tick + min_sell_tick,
			relative_tick + max_sell_tick,
			Side::Sell,
			strategy_id.clone(),
			base_asset,
			quote_asset,
		))
		.collect();

		// Check if the ticks changed to justify updating the orders.
		let ticks_need_update = check_tick_changes && {
			existing_orders
				.iter()
				.map(|order| (order.tick, order.side))
				.collect::<BTreeSet<_>>() !=
				new_orders.iter().map(|order| (order.tick, order.side)).collect::<BTreeSet<_>>()
		};

		if update_due_to_balance || ticks_need_update {
			// Close all open orders for the strategy
			if let Err(e) = T::PoolApi::cancel_all_limit_orders(strategy_id) {
				log_or_panic!(
					"Failed to cancel all limit orders for strategy {:?}: {:?}",
					strategy_id,
					e
				);
				return;
			}
			*weight_used += limit_order_update_weight * 3;

			// Create the new desired orders
			let mut remaining_base_amount = total_base_asset;
			let mut remaining_quote_amount = total_quote_asset;
			new_orders.into_iter().for_each(
				|StrategyLimitOrder { base_asset, side, order_id, tick, amount, .. }| {
					// Convert USD amounts back to native asset amounts for order placement.
					// Track remaining amounts to avoid placing orders beyond available balance.
					let amount = if convert_to_usd {
						let (asset, remaining) = if side == Side::Sell {
							(base_asset, &mut remaining_base_amount)
						} else {
							(quote_asset, &mut remaining_quote_amount)
						};
						let amount = T::PriceFeedApi::get_price(asset)
							.map(|oracle| oracle.price.input_amount_floor(amount).saturated_into())
							.unwrap_or(amount)
							.min(*remaining);
						*remaining = remaining.saturating_sub(amount);
						amount
					} else {
						amount
					};

					*weight_used += limit_order_update_weight;
					let _result = T::PoolApi::update_limit_order(
						strategy_id,
						base_asset,
						quote_asset,
						side,
						order_id,
						Some(tick),
						IncreaseOrDecrease::Increase(amount),
					);
				},
			);
		}
	}
}

/// Validates the four tick-range parameters shared by InventoryBased and OracleTracking.
fn validate_inventory_tick_ranges<T: Config>(
	min_buy_tick: Tick,
	max_buy_tick: Tick,
	min_sell_tick: Tick,
	max_sell_tick: Tick,
) -> Result<(), Error<T>> {
	let average_buy_tick = average_tick(min_buy_tick, max_buy_tick, false /* round down */);
	let average_sell_tick = average_tick(min_sell_tick, max_sell_tick, true /* round up */);

	if min_buy_tick > max_buy_tick ||
		min_sell_tick > max_sell_tick ||
		min_sell_tick < average_buy_tick ||
		max_buy_tick > average_sell_tick ||
		max_buy_tick > cf_amm_math::MAX_TICK ||
		max_sell_tick > cf_amm_math::MAX_TICK ||
		min_buy_tick < cf_amm_math::MIN_TICK ||
		min_sell_tick < cf_amm_math::MIN_TICK
	{
		return Err(Error::<T>::InvalidTick)
	}
	Ok(())
}

/// Logic for one side of the inventory-based strategy.
///
/// Given the amount of asset on this side compared to the total amount on both sides, returns
/// the limit orders that should be created. The logic is as follows:
/// If there is too much asset on this side, ie amount > half_total, then we want 2 limit
/// orders:
/// 1. A simple order at the average tick between min_tick and max_tick, with half of the total
///    amount.
/// 2. A dynamic order at a tick that is more aggressive than the average tick with the remaining
///    amount.
///
/// The reason for splitting the order in 2 is to avoid having all of the asset at the most
///    aggressive tick get executed and become an order on the opposite side also at the most
///    aggressive tick, stuck in a non-profitable loop. By splitting the order at the half
///    total we maximize the chance of the strategy balancing out to 50/50 over time.
///
/// If there is not too much asset on this side, ie amount <= half_total, then we want 1 limit
/// order:
/// 1. A dynamic order at a tick that is more defensive than the average tick. This is the same
///    logic as the dynamic order above.
fn inventory_based_strategy_logic<AccountId: Clone>(
	amount: AssetAmount,
	total: AssetAmount,
	min_tick: Tick,
	max_tick: Tick,
	side: Side,
	account_id: AccountId,
	base_asset: Asset,
	quote_asset: Asset,
) -> Vec<StrategyLimitOrder<AccountId>> {
	if total == 0 {
		return Vec::new();
	}
	let mut orders: BTreeMap<Tick, StrategyLimitOrder<AccountId>> = BTreeMap::new();
	let half_total = total / 2;

	// Simple order logic:
	let remaining_amount = if amount >= half_total {
		// Get the average tick, making sure to round the tick defensively
		let round_up = side == Side::Sell;
		let average_tick = average_tick(min_tick, max_tick, round_up);
		orders.insert(
			average_tick,
			StrategyLimitOrder {
				account_id: account_id.clone(),
				base_asset,
				quote_asset,
				side,
				order_id: STRATEGY_ORDER_ID_1,
				tick: average_tick,
				amount: half_total,
			},
		);
		amount.saturating_sub(half_total)
	} else {
		amount
	};

	// Dynamic order logic:
	if remaining_amount > 0 {
		// Calculate the tick based on the fraction of the total amount
		let tick_adjustment =
			(Permill::from_rational(amount, total) * ((max_tick - min_tick).unsigned_abs())) as i32;
		let dynamic_tick =
			if side == Side::Buy { min_tick + tick_adjustment } else { max_tick - tick_adjustment };
		// Merge the order if its at the same tick as the simple order, or just add a new
		// order.
		orders
			.entry(dynamic_tick)
			.and_modify(|order| order.amount += remaining_amount)
			.or_insert(StrategyLimitOrder {
				account_id: account_id.clone(),
				base_asset,
				quote_asset,
				side,
				order_id: STRATEGY_ORDER_ID_0,
				tick: dynamic_tick,
				amount: remaining_amount,
			});
	}

	orders.into_values().collect()
}

// Returns the average tick between two ticks, with rounding control.
fn average_tick(tick_1: Tick, tick_2: Tick, round_up: bool) -> Tick {
	let tick = tick_1.saturating_add(tick_2);
	if round_up {
		// Round up
		if tick < 0 {
			(tick) / 2
		} else {
			(tick.saturating_add(1)) / 2
		}
	} else {
		// Round down
		if tick < 0 {
			(tick.saturating_sub(1)) / 2
		} else {
			(tick) / 2
		}
	}
}

pub struct TradingStrategyDeregistrationCheck<T>(PhantomData<T>);

impl<T: Config> DeregistrationCheck for TradingStrategyDeregistrationCheck<T> {
	type AccountId = T::AccountId;
	type Error = Error<T>;
	fn check(account_id: &T::AccountId) -> Result<(), Self::Error> {
		use frame_support::StorageDoubleMap;
		ensure!(!Strategies::<T>::contains_prefix(account_id), Error::<T>::LpHasActiveStrategies);
		Ok(())
	}
}
