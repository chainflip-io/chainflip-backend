#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::DispatchResult;

use cf_primitives::{
	liquidity::{PoolId, PositionId, TradingPosition},
	Asset, AssetAmount, ForeignChainAddress, ForeignChainAsset, IntentId,
};
use cf_traits::{
	liquidity::{AmmPoolApi, LpProvisioningApi},
	AccountRoleRegistry, Chainflip, EgressApi, IngressApi, SystemStateInfo,
};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

// #[cfg(feature = "runtime-benchmarks")]
// mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod liquidity_pool;
use liquidity_pool::*;
// pub mod weights;
// pub use weights::WeightInfo;

#[derive(Debug, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct UserTradingPosition<AccountId, Amount> {
	pub account_id: AccountId,
	pub position: TradingPosition<Amount>,
	pub pool_id: PoolId,
}

impl<AccountId, Amount> UserTradingPosition<AccountId, Amount> {
	pub fn new(account_id: AccountId, position: TradingPosition<Amount>, pool_id: PoolId) -> Self {
		Self { account_id, position, pool_id }
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// API used for requesting an ingress.
		type Ingress: IngressApi<AccountId = <Self as frame_system::Config>::AccountId>;

		/// API used to withdraw foreign assets off the chain.
		type EgressApi: EgressApi;

		/// For governance checks.
		type EnsureGovernance: EnsureOrigin<Self::Origin>;
	}

	#[pallet::error]
	pub enum Error<T> {
		// The user does not have enough fund.
		InsufficientBalance,
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
		// The Asset cannot be egressed to the destination chain.
		InvalidEgressAddress,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AccountRegistered {
			account_id: T::AccountId,
			asset: Asset,
		},
		AccountDebited {
			account_id: T::AccountId,
			asset: Asset,
			amount_debited: AssetAmount,
		},
		AccountCredited {
			account_id: T::AccountId,
			asset: Asset,
			amount_credited: AssetAmount,
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
			account_id: T::AccountId,
			position_id: PositionId,
			pool_id: PoolId,
			position: TradingPosition<AssetAmount>,
		},
		TradingPositionUpdated {
			account_id: T::AccountId,
			position_id: PositionId,
			pool_id: PoolId,
			new_position: TradingPosition<AssetAmount>,
		},
		TradingPositionClosed {
			account_id: T::AccountId,
			position_id: PositionId,
		},
		DepositAddressReady {
			intent_id: IntentId,
			ingress_address: ForeignChainAddress,
		},
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	/// Storage for user's free balances/ DoubleMap: (AccountId, Asset) => Balance
	pub type FreeBalances<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::AccountId, Identity, Asset, AssetAmount>;

	#[pallet::storage]
	/// Stores liquidity pools that are allowed: PoolId => LiquidityPool
	pub type LiquidityPools<T: Config> =
		StorageMap<_, Twox64Concat, PoolId, LiquidityPool<AssetAmount>>;

	#[pallet::storage]
	/// A map of Amm Position ID to the TradingPosition and owner. Map: PositionId =>
	/// UserTradingPosition
	pub type TradingPositions<T: Config> =
		StorageMap<_, Twox64Concat, PositionId, UserTradingPosition<T::AccountId, AssetAmount>>;

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
		) -> DispatchResultWithPostInfo {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			let pool_id = (asset0, asset1);

			ensure!(
				LiquidityPools::<T>::get(pool_id).is_none(),
				Error::<T>::LiquidityPoolAlreadyExists
			);

			LiquidityPools::<T>::insert(pool_id, LiquidityPool::<AssetAmount>::new(asset0, asset1));

			Self::deposit_event(Event::LiquidityPoolAdded { asset0, asset1 });

			Ok(().into())
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
		) -> DispatchResultWithPostInfo {
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

			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn request_deposit_address(
			origin: OriginFor<T>,
			asset: ForeignChainAsset,
		) -> DispatchResultWithPostInfo {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let (intent_id, ingress_address) =
				T::Ingress::register_liquidity_ingress_intent(account_id, asset)?;

			Self::deposit_event(Event::DepositAddressReady { intent_id, ingress_address });

			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn withdraw_liquidity(
			origin: OriginFor<T>,
			amount: AssetAmount,
			foreign_asset: ForeignChainAsset,
			egress_address: ForeignChainAddress,
		) -> DispatchResultWithPostInfo {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			ensure!(
				T::EgressApi::is_egress_valid(&foreign_asset, &egress_address,),
				Error::<T>::InvalidEgressAddress
			);

			// Debit the asset from the account.
			Pallet::<T>::try_debit(&account_id, foreign_asset.asset, amount)?;

			// Send the assets off-chain.
			T::EgressApi::schedule_egress(foreign_asset, amount, egress_address);

			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn register_lp_account(who: OriginFor<T>) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(who)?;

			T::AccountRoleRegistry::register_as_liquidity_provider(&account_id)?;

			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn open_position(
			origin: OriginFor<T>,
			pool_id: PoolId,
			position: TradingPosition<AssetAmount>,
		) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			// Ensure the liquidity pool is enabled.
			let mut pool =
				LiquidityPools::<T>::get(pool_id).ok_or(Error::<T>::InvalidLiquidityPool)?;
			ensure!(pool.enabled, Error::<T>::LiquidityPoolDisabled);

			let maybe_liquidity = pool.get_liquidity_requirement(&position);
			ensure!(maybe_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
			let (liquidity_0, liquidity_1) = maybe_liquidity.unwrap();

			// Debit the user's asset from their account.
			Pallet::<T>::try_debit(&account_id, pool_id.0, liquidity_0)?;
			Pallet::<T>::try_debit(&account_id, pool_id.1, liquidity_1)?;

			// Update the pool's liquidity amount
			pool.liquidity_0 = pool.liquidity_0.saturating_add(liquidity_0);
			pool.liquidity_1 = pool.liquidity_1.saturating_add(liquidity_1);
			LiquidityPools::<T>::insert(pool_id, pool);

			let position_id = NextTradingPositionId::<T>::get();
			NextTradingPositionId::<T>::put(position_id.saturating_add(1u64));

			// Insert the position into
			TradingPositions::<T>::insert(
				position_id,
				UserTradingPosition::<T::AccountId, AssetAmount>::new(
					account_id.clone(),
					position,
					pool_id,
				),
			);

			Self::deposit_event(Event::<T>::TradingPositionOpened {
				account_id,
				position_id,
				pool_id,
				position,
			});

			Ok(())
		}

		#[pallet::weight(0)]
		pub fn update_position(
			origin: OriginFor<T>,
			pool_id: PoolId,
			id: PositionId,
			new_position: TradingPosition<AssetAmount>,
		) -> DispatchResultWithPostInfo {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			TradingPositions::<T>::try_mutate(id, |maybe_position| {
				match maybe_position.as_mut() {
					None => Err(Error::<T>::InvalidTradingPosition),
					Some(current_position) => {
						ensure!(
							current_position.pool_id == pool_id,
							Error::<T>::InvalidTradingPosition
						);
						ensure!(
							current_position.account_id == account_id,
							Error::<T>::UnauthorisedToModify
						);

						let maybe_pool = LiquidityPools::<T>::get(pool_id);
						ensure!(maybe_pool.is_some(), Error::<T>::InvalidLiquidityPool);
						let mut pool = maybe_pool.unwrap();
						ensure!(pool.enabled, Error::<T>::LiquidityPoolDisabled);

						let maybe_liquidity =
							pool.get_liquidity_requirement(&current_position.position);
						ensure!(maybe_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
						let (old_liquidity_0, old_liquidity_1) = maybe_liquidity.unwrap();

						// Refund the debited assets for the previous position
						Pallet::<T>::credit(&account_id, pool_id.0, old_liquidity_0)?;
						Pallet::<T>::credit(&account_id, pool_id.1, old_liquidity_1)?;
						// Update the Position storage
						current_position.position = new_position;

						// Debit the user's account for the new position.
						let maybe_new_liquidity = pool.get_liquidity_requirement(&new_position);
						ensure!(maybe_new_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
						let (new_liquidity_0, new_liquidity_1) = maybe_new_liquidity.unwrap();

						Pallet::<T>::try_debit(&account_id, pool_id.0, new_liquidity_0)?;
						Pallet::<T>::try_debit(&account_id, pool_id.1, new_liquidity_1)?;

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
							account_id,
							position_id: id,
							pool_id,
							new_position,
						});
						Ok(())
					},
				}
			})?;

			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn close_position(who: OriginFor<T>, id: PositionId) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(who)?;

			// Remove the position.
			let maybe_position = TradingPositions::<T>::take(id);

			// Ensure the position exists and belongs to the user.
			ensure!(maybe_position.is_some(), Error::<T>::InvalidTradingPosition);
			let current_position = maybe_position.unwrap();
			ensure!(current_position.account_id == account_id, Error::<T>::UnauthorisedToModify);

			let maybe_pool = LiquidityPools::<T>::get(current_position.pool_id);
			ensure!(maybe_pool.is_some(), Error::<T>::InvalidLiquidityPool);
			let mut pool = maybe_pool.unwrap();
			ensure!(pool.enabled, Error::<T>::LiquidityPoolDisabled);

			let maybe_liquidity = pool.get_liquidity_requirement(&current_position.position);
			ensure!(maybe_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
			let (liquidity_0, liquidity_1) = maybe_liquidity.unwrap();

			// Refund the debited assets for the previous position
			Pallet::<T>::credit(&account_id, current_position.pool_id.0, liquidity_0)?;
			Pallet::<T>::credit(&account_id, current_position.pool_id.1, liquidity_1)?;

			// Update the pool's liquidity amount
			pool.liquidity_0 = pool.liquidity_0.saturating_sub(liquidity_0);
			pool.liquidity_1 = pool.liquidity_1.saturating_sub(liquidity_1);
			LiquidityPools::<T>::insert(current_position.pool_id, pool);

			Self::deposit_event(Event::<T>::TradingPositionClosed { account_id, position_id: id });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn try_debit(
		account_id: &T::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> Result<(), Error<T>> {
		let mut balance = FreeBalances::<T>::get(account_id, asset).unwrap_or_default();
		ensure!(balance >= amount, Error::<T>::InsufficientBalance);
		balance = balance.saturating_sub(amount);
		FreeBalances::<T>::insert(account_id, asset, balance);

		Self::deposit_event(Event::AccountDebited {
			account_id: account_id.clone(),
			asset,
			amount_debited: amount,
		});
		Ok(())
	}

	fn credit(
		account_id: &T::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> Result<(), Error<T>> {
		FreeBalances::<T>::mutate(account_id, asset, |maybe_balance| {
			let mut balance = maybe_balance.unwrap_or_default();
			balance = balance.saturating_add(amount);
			*maybe_balance = Some(balance);
		});

		Self::deposit_event(Event::AccountCredited {
			account_id: account_id.clone(),
			asset,
			amount_credited: amount,
		});
		Ok(())
	}
}

impl<T: Config> LpProvisioningApi for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;
	type Amount = AssetAmount;

	fn provision_account(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: Self::Amount,
	) -> DispatchResult {
		Pallet::<T>::credit(account_id, asset, amount).map_err(Into::into)
	}
}
