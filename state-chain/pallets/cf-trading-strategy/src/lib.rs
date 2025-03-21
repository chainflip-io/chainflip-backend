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

use cf_primitives::{Asset, AssetAmount, Tick, STABLE_ASSET};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AccountRoleRegistry, BalanceApi, Chainflip, DeregistrationCheck,
	IncreaseOrDecrease, LpOrdersWeightsProvider, LpRegistration, OrderId, PoolApi, Side,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{traits::One, FixedU64},
	traits::HandleLifetime,
};
use frame_system::pallet_prelude::*;
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};
use weights::WeightInfo;

pub use pallet::*;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

// Note that strategies can only create one order per asset/side so we can just
// have a fixed order id (at least until we develop more advanced strategies).
const STRATEGY_ORDER_ID: OrderId = 0;

impl_pallet_safe_mode!(PalletSafeMode; trading_strategies_enabled);

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub enum TradingStrategy {
	SellAndBuyAtTicks { sell_tick: Tick, buy_tick: Tick, base_asset: Asset },
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
	pub fn supported_assets(&self) -> Vec<Asset> {
		match self {
			TradingStrategy::SellAndBuyAtTicks { base_asset, .. } => {
				vec![*base_asset, STABLE_ASSET]
			},
		}
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

type AssetToAmountMap = BTreeMap<Asset, AssetAmount>;

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
	pub(super) type Strategies<T: Config> = StorageDoubleMap<
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
	#[pallet::storage]
	pub(super) type LimitOrderUpdateThresholds<T: Config> =
		StorageValue<_, AssetToAmountMap, ValueQuery>;

	/// Stores minimum amount per asset necessary to deploy a strategy if only one of the
	/// assets is provided. If more then one asset is provided, we allow splitting the requirement
	/// between them: e.g. it is possible to start a strategy with only 30% of the required amount
	/// of asset A, as long as there is at least 70% of the required amount of asset B.
	#[pallet::storage]
	pub(super) type MinimumDeploymentAmountForStrategy<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_current_block: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut weight_used: Weight = T::DbWeight::get().reads(1);

			if !T::SafeMode::get().trading_strategies_enabled {
				return weight_used
			}

			weight_used += T::DbWeight::get().reads(1);
			let order_update_thresholds = LimitOrderUpdateThresholds::<T>::get();

			weight_used += T::DbWeight::get().reads(1);

			let limit_order_update_weight = T::LpOrdersWeights::update_limit_order_weight();

			for (_, strategy_id, strategy) in Strategies::<T>::iter() {
				match strategy {
					TradingStrategy::SellAndBuyAtTicks { sell_tick, buy_tick, base_asset } => {
						let new_weight_estimate =
							weight_used.saturating_add(limit_order_update_weight * 2);

						if remaining_weight.checked_sub(&new_weight_estimate).is_none() {
							break;
						}

						for (side, tick) in [(Side::Buy, buy_tick), (Side::Sell, sell_tick)] {
							let sell_asset =
								if side == Side::Buy { STABLE_ASSET } else { base_asset };

							weight_used += T::DbWeight::get().reads(1);
							let balance = T::BalanceApi::get_balance(&strategy_id, sell_asset);

							// Default to 1 to prevent updating with 0 amounts
							let threshold =
								order_update_thresholds.get(&sell_asset).copied().unwrap_or(1);

							if balance >= threshold {
								weight_used += limit_order_update_weight;

								if T::PoolApi::update_limit_order(
									&strategy_id,
									base_asset,
									STABLE_ASSET,
									side,
									STRATEGY_ORDER_ID,
									Some(tick),
									IncreaseOrDecrease::Increase(balance),
								)
								.is_err()
								{
									// Should be impossible to get an error since we just
									// checked the balance above
									log_or_panic!(
										"Failed to update limit order for strategy {strategy_id:?}"
									);
								}
							}
						}
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
	}

	#[pallet::error]
	pub enum Error<T> {
		StrategyNotFound,
		AmountBelowDeploymentThreshold,
		InvalidAssetsForStrategy,
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
				T::SafeMode::get().trading_strategies_enabled,
				Error::<T>::TradingStrategiesDisabled
			);

			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			for asset in strategy.supported_assets() {
				T::LpRegistrationApi::ensure_has_refund_address_for_asset(lp, asset)?;
			}

			let strategy_id = {
				// Check that strategy is created with sufficient funds:
				{
					let fraction_of_required = |asset, provided| {
						let min_required = MinimumDeploymentAmountForStrategy::<T>::get(asset);

						if min_required == 0 {
							FixedU64::one()
						} else {
							FixedU64::from_rational(provided, min_required)
						}
					};

					ensure!(
						strategy.supported_assets().into_iter().fold(
							FixedU64::default(),
							|acc, asset| {
								acc + fraction_of_required(
									asset,
									*funding.get(&asset).unwrap_or(&0),
								)
							}
						) >= FixedU64::one(),
						Error::<T>::AmountBelowDeploymentThreshold
					);
				}

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
				T::SafeMode::get().trading_strategies_enabled,
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
				T::SafeMode::get().trading_strategies_enabled,
				Error::<T>::TradingStrategiesDisabled
			);
			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let strategy =
				Strategies::<T>::get(lp, &strategy_id).ok_or(Error::<T>::StrategyNotFound)?;

			Self::add_funds_to_existing_strategy(lp, &strategy_id, strategy, funding)
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
