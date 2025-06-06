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

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use core::cmp::min;
use std::path::absolute;

use cf_primitives::{Asset, AssetAmount, StablecoinDefaults, Tick, STABLE_ASSET};
use cf_traits::{
	impl_pallet_safe_mode, AccountRoleRegistry, BalanceApi, Chainflip, DeregistrationCheck,
	IncreaseOrDecrease, LpOrdersWeightsProvider, LpRegistration, OrderId, PoolApi, Side,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{traits::One, FixedPointNumber, FixedU64, Permill},
	traits::HandleLifetime,
};
use frame_system::{pallet_prelude::*, WeightInfo as SystemWeightInfo};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec,
};
use weights::WeightInfo;

pub use pallet::*;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

// Note that strategies can only create a limited number of orders per asset/side so we can just
// have a fixed order id (at least until we develop more advanced strategies).
const STRATEGY_ORDER_ID_0: OrderId = 0;
const STRATEGY_ORDER_ID_1: OrderId = 1;

impl_pallet_safe_mode!(PalletSafeMode; strategy_updates_enabled, strategy_closure_enabled, strategy_execution_enabled);

#[derive(
	Clone, Debug, Encode, Decode, TypeInfo, serde::Serialize, serde::Deserialize, PartialEq, Eq,
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
}

#[derive(Clone, RuntimeDebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
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
				// TODO JAMIE: let neg min spread
				if min_buy_tick >= max_buy_tick ||
					min_sell_tick >= max_sell_tick ||
					*max_buy_tick > cf_amm_math::MAX_TICK ||
					*max_sell_tick > cf_amm_math::MAX_TICK ||
					*min_buy_tick < cf_amm_math::MIN_TICK ||
					*min_sell_tick < cf_amm_math::MIN_TICK
				{
					return Err(Error::<T>::InvalidTick)
				}
				ensure!(*base_asset != STABLE_ASSET, Error::<T>::InvalidAssetsForStrategy);
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
	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type BalanceApi: BalanceApi<AccountId = Self::AccountId>;

		/// LP address registration and verification.
		type LpRegistrationApi: LpRegistration<AccountId = Self::AccountId>;

		type PoolApi: PoolApi<AccountId = Self::AccountId>;

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

	/// Stores thresholds used to determine whether a trading strategy for a given asset
	/// has enough funds in "free balance" to make it worthwhile updating/creating a limit order
	/// with them. Note that we use store map as a single value since it is often more convenient to
	/// read multiple assets at once (and this map is small).
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

			for (_, strategy_id, strategy) in Strategies::<T>::iter() {
				match strategy {
					TradingStrategy::TickZeroCentered { base_asset, .. } |
					TradingStrategy::SimpleBuySell { base_asset, .. } => {
						let new_weight_estimate =
							weight_used.saturating_add(limit_order_update_weight * 2);

						if remaining_weight.checked_sub(&new_weight_estimate).is_none() {
							break;
						}
						let (buy_tick, sell_tick) = match strategy {
							TradingStrategy::TickZeroCentered { spread_tick, .. } =>
								(-spread_tick, spread_tick),
							TradingStrategy::SimpleBuySell { buy_tick, sell_tick, .. } =>
								(buy_tick, sell_tick),
							_ => unreachable!("Unreachable due to match above"),
						};
						for (side, tick) in [(Side::Buy, buy_tick), (Side::Sell, sell_tick)] {
							let sell_asset =
								if side == Side::Buy { STABLE_ASSET } else { base_asset };

							weight_used += T::DbWeight::get().reads(1);
							let balance = T::BalanceApi::get_balance(&strategy_id, sell_asset);

							// Minimum threshold of 1 to prevent updating with 0 amounts
							let threshold = core::cmp::max(
								order_update_thresholds
									.get(&sell_asset)
									.copied()
									.unwrap_or(u128::MAX),
								1,
							);

							if balance >= threshold {
								weight_used += limit_order_update_weight;

								// We expect this to fail if the pool does not exist
								let _result = T::PoolApi::update_limit_order(
									&strategy_id,
									base_asset,
									STABLE_ASSET,
									side,
									STRATEGY_ORDER_ID_0,
									Some(tick),
									IncreaseOrDecrease::Increase(balance),
								);
							}
						}
					},
					TradingStrategy::InventoryBased {
						min_buy_tick,
						max_buy_tick,
						min_sell_tick,
						max_sell_tick,
						base_asset,
					} => {
						let new_weight_estimate =
							weight_used.saturating_add(limit_order_update_weight * 3);

						if remaining_weight.checked_sub(&new_weight_estimate).is_none() {
							break;
						}

						let balance_quote = T::BalanceApi::get_balance(&strategy_id, STABLE_ASSET);
						let balance_base = T::BalanceApi::get_balance(&strategy_id, base_asset);
						let cf_traits::LimitOrders {
							base: open_orders_base,
							quote: open_orders_quote,
						} = T::PoolApi::get_open_limit_orders(
							base_asset,
							STABLE_ASSET,
							strategy_id.clone(),
						)
						.unwrap_or_default();

						let sum_quote = balance_quote +
							open_orders_quote
								.iter()
								.fold(0, |acc, (_, order)| acc + order.sell_amount);

						let sum_base = balance_base +
							open_orders_base
								.iter()
								.fold(0, |acc, (_, order)| acc + order.sell_amount);

						let new_orders = Self::inventory_base_strategy_logic(
							base_asset,
							sum_base,
							sum_quote,
							min_buy_tick,
							max_buy_tick,
							min_sell_tick,
							max_sell_tick,
							&order_update_thresholds,
						);

						[
							(Side::Buy, new_orders.base, open_orders_base),
							(Side::Sell, new_orders.quote, open_orders_quote),
						]
						.iter()
						.for_each(|(side, new_orders, open_orders)| {
							[STRATEGY_ORDER_ID_0, STRATEGY_ORDER_ID_1].iter().for_each(|order_id| {
								match (new_orders.get(&order_id), open_orders.get(&order_id)) {
									(Some(desired_order), Some(open_order))
										if desired_order.tick != open_order.tick =>
									{
										// Close the old order and create a new one at the new tick
										let _result = T::PoolApi::cancel_limit_order(
											&strategy_id,
											base_asset,
											STABLE_ASSET,
											*side,
											*order_id,
											open_order.tick,
										);
										let _result = T::PoolApi::update_limit_order(
											&strategy_id,
											base_asset,
											STABLE_ASSET,
											*side,
											STRATEGY_ORDER_ID_0,
											Some(desired_order.tick),
											IncreaseOrDecrease::Increase(desired_order.sell_amount),
										);
									},
									(Some(desired_order), None) => {
										// create a new order
										let _result = T::PoolApi::update_limit_order(
											&strategy_id,
											base_asset,
											STABLE_ASSET,
											*side,
											STRATEGY_ORDER_ID_0,
											Some(desired_order.tick),
											IncreaseOrDecrease::Increase(desired_order.sell_amount),
										);
									},
									(None, Some(open_order)) => {
										// Close the order
										let _result = T::PoolApi::cancel_limit_order(
											&strategy_id,
											base_asset,
											STABLE_ASSET,
											*side,
											*order_id,
											open_order.tick,
										);
									},
									_ => {
										// No action needed
									},
								}
							})
						});
					},
				}
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

	fn inventory_base_strategy_logic(
		base_asset: Asset,
		base_amount: AssetAmount,
		quote_amount: AssetAmount,
		min_buy_tick: Tick,
		max_buy_tick: Tick,
		min_sell_tick: Tick,
		max_sell_tick: Tick,
		order_update_thresholds: &BTreeMap<Asset, AssetAmount>,
	) -> cf_traits::LimitOrders {
		let mut desired_orders =
			cf_traits::LimitOrders { base: BTreeMap::new(), quote: BTreeMap::new() };

		let half_total = (quote_amount + base_amount) / 2;

		[
			(base_asset, base_amount, min_buy_tick, max_buy_tick),
			(STABLE_ASSET, quote_amount, min_sell_tick, max_sell_tick),
		]
		.into_iter()
		.for_each(|(asset, amount, tick_1, tick_2)| {
			let fraction_of_total = if base_amount + quote_amount == 0 {
				//0.into()
				Permill::zero()
			} else {
				//FixedU64::from_rational(amount, quote_amount + base_amount)
				if asset == base_asset {
					Permill::from_rational(amount, base_amount + quote_amount)
				} else {
					Permill::one() - Permill::from_rational(amount, base_amount + quote_amount)
				}
			};
			//let percent = fraction_of_total.checked_mul_int(100).unwrap();
			//println!("fraction: {:?}%", percent);

			println!(
				"InventoryBased strategy logic for asset: {:?}, amount: {}, ticks: ({}, {})",
				asset, amount, tick_1, tick_2
			);

			let order_list = if asset == base_asset {
				&mut desired_orders.base
			} else {
				&mut desired_orders.quote
			};

			// Simple order logic:
			if amount >= half_total {
				order_list.insert(
					STRATEGY_ORDER_ID_1,
					cf_traits::LimitOrder {
						tick: (tick_1.saturating_add(tick_2)) / 2,
						sell_amount: half_total,
					},
				);
			}

			// Minimum threshold of 1 to prevent updating with 0 amounts
			let threshold = core::cmp::max(
				order_update_thresholds.get(&asset).copied().unwrap_or(u128::MAX),
				1,
			);

			// Dynamic order logic:
			let remaining_amount = amount % half_total;
			if remaining_amount >= threshold {
				// if let Some(tick) =
				// 	fraction_of_total.checked_mul_int(tick_2 - tick_1).map(|tick| tick + tick_1)
				// {
				let tick_offset = tick_1.abs();
				let tick = (fraction_of_total * (((tick_2 - tick_1).abs()) as u32)) as i32 + tick_1;
				order_list.insert(
					STRATEGY_ORDER_ID_0,
					cf_traits::LimitOrder { tick, sell_amount: remaining_amount },
				);
				//}
			}
		});

		// TODO JAMIE: Sanity check that the orders are within the ranges.

		desired_orders
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
