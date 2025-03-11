#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

use frame_support::sp_runtime::FixedU64;

use cf_primitives::{Asset, AssetAmount, Tick, STABLE_ASSET};
use cf_traits::{AccountRoleRegistry, BalanceApi, Chainflip, IncreaseOrDecrease, OrderId, Side};

pub use pallet::*;
use weights::WeightInfo;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

// Note that strategies can only create one order per asset/side so we can just
// have a fixed order id (at least until we develop more advanced strategies).
const STRATEGY_ORDER_ID: OrderId = 0;

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq)]
struct TradingStrategyEntry<AccountId> {
	base_asset: Asset,
	owner: AccountId,
	strategy: TradingStrategy,
}

#[derive(Clone, Debug, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub enum TradingStrategy {
	SellAndBuyAtTicks { sell_tick: Tick, buy_tick: Tick },
}

fn derive_strategy_id<T: Config>(lp: &T::AccountId, nonce: T::Nonce) -> T::AccountId {
	use frame_support::{sp_runtime::traits::TrailingZeroInput, Hashable};

	// Combination of lp + nonce is unique for every successful call, so this should
	// generate unique ids:
	Decode::decode(&mut TrailingZeroInput::new(
		(*b"chainflip/strategy_account", lp.clone(), nonce).blake2_256().as_ref(),
	))
	.unwrap()
}

#[frame_support::pallet]
pub mod pallet {

	use cf_runtime_utilities::log_or_panic;
	use cf_traits::PoolApi;
	use frame_support::sp_runtime::traits::One;

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;

		type BalanceApi: BalanceApi<AccountId = Self::AccountId>;

		type PoolApi: PoolApi<AccountId = Self::AccountId>;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	// Stores all deployed strategies by their id
	#[pallet::storage]
	pub(super) type Strategies<T: Config> =
		StorageMap<_, Identity, T::AccountId, TradingStrategyEntry<T::AccountId>, OptionQuery>;

	/// Stores thresholds used to determine whether a trading strategy for a given asset
	/// has enough funds in "free balance" to make it worthwhile updating/creating a limit order
	/// with them. Note that we use store map as a single value since it is often more convenient to
	/// read multiple assets at once (and this map is small).
	#[pallet::storage]
	pub(super) type LimitOrderUpdateThresholds<T: Config> =
		StorageValue<_, BoundedBTreeMap<Asset, AssetAmount, ConstU32<1000>>, ValueQuery>;

	/// Stores minimum amount per asset necessary to deploy a strategy if only one of the two
	/// assets is provided. If both assets are provided, we allow splitting the requirement between
	/// them: e.g. it is possible to start a strategy with only 30% of the required amount of asset
	/// A, as long as there is at least 70% of the required amount of asset B.
	#[pallet::storage]
	pub(super) type MinimumDeploymentAmountForStrategy<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_current_block: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut weight_used: Weight = T::DbWeight::get().reads(1);

			// TODO: use safe mode here checking if auto strategies are enabled

			let order_update_thresholds = LimitOrderUpdateThresholds::<T>::get();

			weight_used += T::DbWeight::get().reads(1);

			// TODO: use correct weight from pools pallet
			let limit_order_update_weight = Weight::zero();

			for (strategy_id, TradingStrategyEntry { base_asset, strategy, .. }) in
				Strategies::<T>::iter()
			{
				match strategy {
					TradingStrategy::SellAndBuyAtTicks { sell_tick, buy_tick } => {
						let new_weight_estimate =
							weight_used.saturating_add(limit_order_update_weight * 2);

						let mut update_limit_order_from_balance = |sell_asset, side, tick| {
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
						};

						if remaining_weight.checked_sub(&new_weight_estimate).is_none() {
							break;
						}

						update_limit_order_from_balance(base_asset, Side::Sell, sell_tick);
						update_limit_order_from_balance(STABLE_ASSET, Side::Buy, buy_tick);
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
			base_asset: Asset,
			strategy: TradingStrategy,
		},
		FundsAddedToStrategy {
			strategy_id: T::AccountId,
			base_asset: Asset,
			base_asset_amount: AssetAmount,
			quote_asset_amount: AssetAmount,
		},
		StrategyClosed {
			strategy_id: T::AccountId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		StrategyNotFound,
		AmountBelowDeploymentThreshold,
		InvalidOwner,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::zero())] // TODO: benchmark
		pub fn deploy_trading_strategy(
			origin: OriginFor<T>,
			base_asset_amount: AssetAmount,
			quote_asset_amount: AssetAmount,
			base_asset: Asset,
			strategy: TradingStrategy,
		) -> DispatchResult {
			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let quote_asset = STABLE_ASSET;

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
						fraction_of_required(base_asset, base_asset_amount) +
							fraction_of_required(quote_asset, quote_asset_amount) >=
							FixedU64::one(),
						Error::<T>::AmountBelowDeploymentThreshold
					);
				}

				let nonce = frame_system::Pallet::<T>::account_nonce(lp);

				let strategy_id = derive_strategy_id::<T>(lp, nonce);

				Self::deposit_event(Event::<T>::StrategyDeployed {
					account_id: lp.clone(),
					strategy_id: strategy_id.clone(),
					base_asset,
					strategy: strategy.clone(),
				});

				Strategies::<T>::insert(
					strategy_id.clone(),
					TradingStrategyEntry { base_asset, owner: lp.clone(), strategy },
				);

				strategy_id
			};

			Self::add_funds_to_existing_strategy(
				lp,
				&strategy_id,
				base_asset,
				base_asset_amount,
				quote_asset_amount,
			)
		}

		#[pallet::call_index(2)]
		#[pallet::weight(Weight::zero())] // TODO: benchmark
		pub fn close_strategy(origin: OriginFor<T>, strategy_id: T::AccountId) -> DispatchResult {
			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let TradingStrategyEntry { base_asset, owner, strategy } =
				Strategies::<T>::take(&strategy_id).ok_or(Error::<T>::StrategyNotFound)?;

			ensure!(lp == &owner, Error::<T>::InvalidOwner);

			// TODO: instead of reading ticks from the strategy, we could extend PoolApi with
			// a method to close all (limit) orders (which might be necessary for more complex
			// strategies).
			let TradingStrategy::SellAndBuyAtTicks { buy_tick, sell_tick } = strategy;

			let cancel_limit_orders = |side, tick| {
				// TODO: check if order cancellation is infallible?
				T::PoolApi::cancel_limit_order(
					&strategy_id,
					base_asset,
					STABLE_ASSET,
					side,
					STRATEGY_ORDER_ID,
					tick,
				)
			};

			cancel_limit_orders(Side::Buy, buy_tick)?;
			cancel_limit_orders(Side::Sell, sell_tick)?;

			for asset in [base_asset, STABLE_ASSET] {
				let balance = T::BalanceApi::get_balance(&strategy_id, asset);
				T::BalanceApi::try_debit_account(&strategy_id, asset, balance)?;
				T::BalanceApi::credit_account(lp, asset, balance);
			}

			Self::deposit_event(Event::<T>::StrategyClosed { strategy_id: strategy_id.clone() });

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(Weight::zero())] // TODO: benchmark
		pub fn add_funds_to_strategy(
			origin: OriginFor<T>,
			base_asset_amount: AssetAmount,
			quote_asset_amount: AssetAmount,
			strategy_id: T::AccountId,
		) -> DispatchResult {
			let lp = &T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let strategy =
				Strategies::<T>::get(&strategy_id).ok_or(Error::<T>::StrategyNotFound)?;

			ensure!(lp == &strategy.owner, Error::<T>::InvalidOwner);

			Self::add_funds_to_existing_strategy(
				lp,
				&strategy_id,
				strategy.base_asset,
				base_asset_amount,
				quote_asset_amount,
			)
		}
	}
}

impl<T: Config> Pallet<T> {
	fn add_funds_to_existing_strategy(
		lp: &T::AccountId,
		strategy_id: &T::AccountId,
		base_asset: Asset,
		base_asset_amount: AssetAmount,
		quote_asset_amount: AssetAmount,
	) -> DispatchResult {
		if base_asset_amount > 0 {
			T::BalanceApi::try_debit_account(lp, base_asset, base_asset_amount)?;
			T::BalanceApi::credit_account(strategy_id, base_asset, base_asset_amount);
		}

		if quote_asset_amount > 0 {
			T::BalanceApi::try_debit_account(lp, STABLE_ASSET, quote_asset_amount)?;
			T::BalanceApi::credit_account(strategy_id, STABLE_ASSET, quote_asset_amount);
		}

		Self::deposit_event(Event::<T>::FundsAddedToStrategy {
			strategy_id: strategy_id.clone(),
			base_asset,
			base_asset_amount,
			quote_asset_amount,
		});

		Ok(())
	}
}
