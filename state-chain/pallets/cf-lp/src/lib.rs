#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;

use cf_traits::{
	liquidity::{
		AmmPoolApi, Asset, EgressHandler, ForeignAsset, LpAccountHandler, LpPositionManagement,
		LpProvisioningApi, LpWithdrawalApi, PoolId, PositionId, TradingPosition,
	},
	AccountType, AccountTypeRegistry, FlipBalance,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::transactional;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

// #[cfg(feature = "runtime-benchmarks")]
// mod benchmarking;

// #[cfg(test)]
// mod mock;
// #[cfg(test)]
// mod tests;

pub mod liquidity_pool;
use liquidity_pool::*;
// pub mod weights;
// pub use weights::WeightInfo;

#[derive(Debug, Encode, Decode, MaxEncodedLen, Serialize, Deserialize, TypeInfo)]
pub struct UserTradingPosition<AccountId> {
	pub who: AccountId,
	pub position: TradingPosition<FlipBalance>,
	pub pool_id: PoolId,
}

impl<AccountId> UserTradingPosition<AccountId> {
	pub fn new(who: AccountId, position: TradingPosition<FlipBalance>, pool_id: PoolId) -> Self {
		Self { who, position, pool_id }
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Address used to deposit withdrawn foreign assets.
		type EgressAddress: Clone;

		/// Registry for account types
		type AccountTypeRegistry: AccountTypeRegistry<AccountId = Self::AccountId>;

		/// API used to withdraw foreign assets off the chain.
		type EgressHandler: EgressHandler<Amount = FlipBalance, EgressAddress = Self::EgressAddress>;

		/// For governance checks.
		type EnsureGovernance: EnsureOrigin<Self::Origin>;
	}

	#[pallet::error]
	pub enum Error<T> {
		// The user does not have enough fund.
		InsufficientBalance,
		// The give account Id already exists.
		AccountAlreadyExist,
		// The account is not registered as `Liquidity Provider`.
		AccountNotLiquidProvider,
		// The liquidity pool is not available for trade.
		InvalidLiquidityPool,
		// The TradingPosition provided is invalid for the liquidity pool.
		InvalidTradingPosition,
		// The caller is not authorized to modify the trading position.
		UnauthorisedToModify,
		// The liquidity pool already exists.
		LiquidityPoolAlreadyExists,
		// The liquidity pool does not exist.
		LiquidityPoolDoesNotExist,
		// The liquidity pool is currently disabled.
		LiquidityPoolDisabled,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AccountRegistered {
			who: T::AccountId,
			asset: Asset,
		},
		AccountDebited {
			who: T::AccountId,
			asset: Asset,
			amount_debited: FlipBalance,
		},
		AccountCredited {
			who: T::AccountId,
			asset: Asset,
			amount_credited: FlipBalance,
		},
		LiquidityPoolAdded {
			asset0: Asset,
			asset1: Asset,
		},
		LiquidityPoolStatusSet {
			asset0: Asset,
			asset1: Asset,
			enabled: bool,
		},
		TradingPositionOpened {
			who: T::AccountId,
			position_id: PositionId,
			pool_id: PoolId,
			position: TradingPosition<FlipBalance>,
		},
		TradingPositionUpdated {
			who: T::AccountId,
			position_id: PositionId,
			pool_id: PoolId,
			new_position: TradingPosition<FlipBalance>,
		},
		TradingPositionClosed {
			who: T::AccountId,
			position_id: PositionId,
		},
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	/// Storage for user's free balances/ DoubleMap: (AccountId, Asset) => Balance
	pub type FreeBalances<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::AccountId, Identity, Asset, FlipBalance>;

	#[pallet::storage]
	/// Stores liquidity pools that are allowed: PoolId => LiquidityPool
	pub type LiquidityPools<T: Config> =
		StorageMap<_, Twox64Concat, PoolId, LiquidityPool<FlipBalance>>;

	#[pallet::storage]
	/// A map of Amm Position ID to the TradingPosition and owner. Map: PositionId =>
	/// UserTradingPosition
	pub type TradingPositions<T: Config> =
		StorageMap<_, Twox64Concat, PositionId, UserTradingPosition<T::AccountId>>;

	#[pallet::storage]
	/// Stores the Position ID for the next Amm position.
	pub type NextTradingPositionId<T: Config> = StorageValue<_, PositionId, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Add a new Liquidity Pool and allow trading. Requires Governance.
		///
		/// ## Events
		///
		/// - [Event::LiquidityPoolAdded]
		///
		/// ## Errors
		///
		/// - [Error::LiquidityPoolAlreadyExists]
		#[pallet::weight(0)]
		pub fn add_liquidity_pool(
			origin: OriginFor<T>,
			asset0: Asset,
			asset1: Asset,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			let pool_id = (asset0, asset1);

			ensure!(
				LiquidityPools::<T>::get(pool_id).is_none(),
				Error::<T>::LiquidityPoolAlreadyExists
			);

			LiquidityPools::<T>::insert(pool_id, LiquidityPool::<FlipBalance>::new(asset0, asset1));

			Self::deposit_event(Event::LiquidityPoolAdded { asset0, asset1 });

			Ok(())
		}

		/// Enable or disables an existing trading pool. Requires Governance.
		///
		/// ## Events
		///
		/// - [Event::LiquidityPoolEnabled]
		/// - [Event::LiquidityPoolDisabled]
		///
		/// ## Errors
		///
		/// - [Error::LiquidityPoolDoesNotExist]
		#[pallet::weight(0)]
		pub fn set_liquidity_pool_status(
			origin: OriginFor<T>,
			asset0: Asset,
			asset1: Asset,
			enabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			let pool_id = (asset0, asset1);

			ensure!(
				LiquidityPools::<T>::get(pool_id).is_some(),
				Error::<T>::LiquidityPoolDoesNotExist
			);

			LiquidityPools::<T>::mutate(pool_id, |maybe_pool| {
				let mut pool = maybe_pool.unwrap();
				pool.enabled = enabled;
				*maybe_pool = Some(pool);
			});

			Self::deposit_event(Event::LiquidityPoolStatusSet { asset0, asset1, enabled });

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {}

impl<T: Config> LpAccountHandler for Pallet<T> {
	type AccountId = T::AccountId;
	type Amount = FlipBalance;
	fn register_lp_account(who: &Self::AccountId) -> DispatchResult {
		T::AccountTypeRegistry::register_account(who, AccountType::LiquidityProvider)
	}

	fn try_debit(who: &Self::AccountId, asset: Asset, amount: Self::Amount) -> DispatchResult {
		let mut balance = FreeBalances::<T>::get(who, asset).unwrap_or_default();
		ensure!(balance >= amount, Error::<T>::InsufficientBalance);
		balance = balance.saturating_sub(amount);
		FreeBalances::<T>::insert(who, asset, balance);
		Self::deposit_event(Event::AccountDebited {
			who: who.clone(),
			asset,
			amount_debited: amount,
		});
		Ok(())
	}

	fn credit(who: &Self::AccountId, asset: Asset, amount: Self::Amount) -> DispatchResult {
		ensure!(
			T::AccountTypeRegistry::account_type(who) == Some(AccountType::LiquidityProvider),
			Error::<T>::AccountNotLiquidProvider
		);
		FreeBalances::<T>::mutate(who, asset, |maybe_balance| {
			let mut balance = maybe_balance.unwrap_or_default();
			balance = balance.saturating_add(amount);
			*maybe_balance = Some(balance);
		});

		Self::deposit_event(Event::AccountCredited {
			who: who.clone(),
			asset,
			amount_credited: amount,
		});
		Ok(())
	}
}

impl<T: Config> LpProvisioningApi for Pallet<T> {
	type AccountId = T::AccountId;
	type Amount = FlipBalance;

	fn provision_account(who: &Self::AccountId, asset: Asset, amount: Self::Amount) {
		let _res = Pallet::<T>::credit(who, asset, amount);
	}
}

impl<T: Config> LpWithdrawalApi for Pallet<T> {
	type AccountId = T::AccountId;
	type Amount = FlipBalance;
	type EgressAddress = T::EgressAddress;

	fn withdraw_liquidity(
		who: &Self::AccountId,
		amount: Self::Amount,
		foreign_asset: &ForeignAsset,
		egress_address: &Self::EgressAddress,
	) -> DispatchResult {
		ensure!(
			T::AccountTypeRegistry::account_type(who) == Some(AccountType::LiquidityProvider),
			Error::<T>::AccountNotLiquidProvider
		);

		// Debit the asset from the account.
		Pallet::<T>::try_debit(who, foreign_asset.asset, amount)?;

		// Send the assets off-chain.
		T::EgressHandler::add_to_egress_batch(foreign_asset, amount, egress_address)
	}
}

impl<T: Config> LpPositionManagement for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = FlipBalance;

	#[transactional]
	fn open_position(
		who: &Self::AccountId,
		pool_id: PoolId,
		position: TradingPosition<Self::Balance>,
	) -> DispatchResult {
		// Ensure account is Liquidity Provider type.
		ensure!(
			T::AccountTypeRegistry::account_type(who) == Some(AccountType::LiquidityProvider),
			Error::<T>::AccountNotLiquidProvider
		);

		// Ensure the liquidity pool is enabled.
		ensure!(LiquidityPools::<T>::get(pool_id).is_some(), Error::<T>::InvalidLiquidityPool);
		let mut pool = LiquidityPools::<T>::get(pool_id).unwrap();
		ensure!(pool.enabled, Error::<T>::LiquidityPoolDisabled);

		let maybe_liquidity = pool.get_liquidity_requirement(&position);
		ensure!(maybe_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
		let (liquidity_0, liquidity_1) = maybe_liquidity.unwrap();

		// Debit the user's asset from their account.
		Pallet::<T>::try_debit(who, pool_id.0, liquidity_0)?;
		Pallet::<T>::try_debit(who, pool_id.1, liquidity_1)?;

		// Update the pool's liquidity amount
		pool.liquidity_0 = pool.liquidity_0.saturating_add(liquidity_0);
		pool.liquidity_1 = pool.liquidity_1.saturating_add(liquidity_1);
		LiquidityPools::<T>::insert(pool_id, pool);

		let position_id = NextTradingPositionId::<T>::get();
		NextTradingPositionId::<T>::put(position_id.saturating_add(1u64));

		// Insert the position into
		TradingPositions::<T>::insert(
			position_id,
			UserTradingPosition::<T::AccountId>::new(who.clone(), position, pool_id),
		);

		Self::deposit_event(Event::<T>::TradingPositionOpened {
			who: who.clone(),
			position_id,
			pool_id,
			position,
		});
		Ok(())
	}

	#[transactional]
	fn update_position(
		who: &Self::AccountId,
		pool_id: PoolId,
		id: PositionId,
		new_position: TradingPosition<Self::Balance>,
	) -> DispatchResult {
		// Ensure account is Liquidity Provider type.
		ensure!(
			T::AccountTypeRegistry::account_type(who) == Some(AccountType::LiquidityProvider),
			Error::<T>::AccountNotLiquidProvider
		);

		TradingPositions::<T>::try_mutate(id, |maybe_position| {
			match maybe_position.as_mut() {
				None => Err(Error::<T>::InvalidTradingPosition.into()),
				Some(current_position) => {
					ensure!(
						current_position.pool_id == pool_id,
						Error::<T>::InvalidTradingPosition
					);
					ensure!(current_position.who == *who, Error::<T>::UnauthorisedToModify);

					let maybe_pool = LiquidityPools::<T>::get(pool_id);
					ensure!(maybe_pool.is_some(), Error::<T>::InvalidLiquidityPool);
					let mut pool = maybe_pool.unwrap();
					ensure!(pool.enabled, Error::<T>::LiquidityPoolDisabled);

					let maybe_liquidity =
						pool.get_liquidity_requirement(&current_position.position);
					ensure!(maybe_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
					let (old_liquidity_0, old_liquidity_1) = maybe_liquidity.unwrap();

					// Refund the debited assets for the previous position
					Pallet::<T>::credit(who, pool_id.0, old_liquidity_0)?;
					Pallet::<T>::credit(who, pool_id.1, old_liquidity_1)?;
					// Update the Position storage
					current_position.position = new_position;

					// Debit the user's account for the new position.
					let maybe_new_liquidity = pool.get_liquidity_requirement(&new_position);
					ensure!(maybe_new_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
					let (new_liquidity_0, new_liquidity_1) = maybe_new_liquidity.unwrap();

					Pallet::<T>::try_debit(who, pool_id.0, new_liquidity_0)?;
					Pallet::<T>::try_debit(who, pool_id.1, new_liquidity_1)?;

					// Update the pool's liquidity amount
					pool.liquidity_0 = pool
						.liquidity_0
						.saturating_add(new_liquidity_0)
						.saturating_sub(old_liquidity_0);
					pool.liquidity_1 = pool
						.liquidity_1
						.saturating_add(new_liquidity_1)
						.saturating_sub(old_liquidity_1);
					LiquidityPools::<T>::insert(pool_id, pool);

					Self::deposit_event(Event::<T>::TradingPositionUpdated {
						who: who.clone(),
						position_id: id,
						pool_id,
						new_position,
					});
					Ok(())
				},
			}
		})
	}

	#[transactional]
	fn close_position(who: &Self::AccountId, id: PositionId) -> DispatchResult {
		// Remove the position.
		let maybe_position = TradingPositions::<T>::take(id);

		// Ensure the position exists and belongs to the user.
		ensure!(maybe_position.is_some(), Error::<T>::InvalidTradingPosition);
		let current_position = maybe_position.unwrap();
		ensure!(current_position.who == *who, Error::<T>::UnauthorisedToModify);

		let maybe_pool = LiquidityPools::<T>::get(current_position.pool_id);
		ensure!(maybe_pool.is_some(), Error::<T>::InvalidLiquidityPool);
		let mut pool = maybe_pool.unwrap();
		ensure!(pool.enabled, Error::<T>::LiquidityPoolDisabled);

		let maybe_liquidity = pool.get_liquidity_requirement(&current_position.position);
		ensure!(maybe_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
		let (liquidity_0, liquidity_1) = maybe_liquidity.unwrap();

		// Refund the debited assets for the previous position
		Pallet::<T>::credit(who, current_position.pool_id.0, liquidity_0)?;
		Pallet::<T>::credit(who, current_position.pool_id.1, liquidity_1)?;

		// Update the pool's liquidity amount
		pool.liquidity_0 = pool.liquidity_0.saturating_sub(liquidity_0);
		pool.liquidity_1 = pool.liquidity_1.saturating_sub(liquidity_1);
		LiquidityPools::<T>::insert(current_position.pool_id, pool);

		Self::deposit_event(Event::<T>::TradingPositionClosed {
			who: who.clone(),
			position_id: id,
		});
		Ok(())
	}
}
