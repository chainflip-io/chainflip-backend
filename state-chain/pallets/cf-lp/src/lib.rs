#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::DispatchResult;

use cf_chains::AnyChain;
use cf_primitives::{
	liquidity::{PositionId, TradingPosition},
	Asset, AssetAmount, ForeignChain, ForeignChainAddress, IntentId,
};
use cf_traits::{
	liquidity::LpProvisioningApi, AccountRoleRegistry, Chainflip, EgressApi, IngressApi,
	LiquidityPoolApi, SystemStateInfo,
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

#[derive(Debug, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct UserTradingPosition<AccountId, Amount> {
	pub account_id: AccountId,
	pub position: TradingPosition<Amount>,
	pub asset: Asset,
}

impl<AccountId, Amount> UserTradingPosition<AccountId, Amount> {
	pub fn new(account_id: AccountId, position: TradingPosition<Amount>, asset: Asset) -> Self {
		Self { account_id, position, asset }
	}
}

#[frame_support::pallet]
pub mod pallet {
	use cf_primitives::EgressId;

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// API for handling asset ingress.
		type IngressHandler: IngressApi<
			AnyChain,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

		/// API to interface with exchange Pools
		type LiquidityPoolApi: LiquidityPoolApi;

		/// For governance checks.
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::error]
	pub enum Error<T> {
		// The user does not have enough fund.
		InsufficientBalance,
		// The TradingPosition provided is invalid for the liquidity pool.
		InvalidTradingPosition,
		// The caller is not authorized to modify the trading position.
		UnauthorisedToModify,
		// The Asset cannot be egressed to the destination chain.
		InvalidEgressAddress,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
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
		TradingPositionOpened {
			account_id: T::AccountId,
			position_id: PositionId,
			asset: Asset,
			position: TradingPosition<AssetAmount>,
		},
		TradingPositionUpdated {
			account_id: T::AccountId,
			position_id: PositionId,
			asset: Asset,
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
		WithdrawalEgressScheduled {
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
		},
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	/// Storage for user's free balances/ DoubleMap: (AccountId, Asset) => Balance
	pub type FreeBalances<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::AccountId, Identity, Asset, AssetAmount>;

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
		#[pallet::weight(0)]
		pub fn request_deposit_address(origin: OriginFor<T>, asset: Asset) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			let (intent_id, ingress_address) =
				T::IngressHandler::register_liquidity_ingress_intent(account_id, asset)?;

			Self::deposit_event(Event::DepositAddressReady { intent_id, ingress_address });

			Ok(())
		}

		#[pallet::weight(0)]
		pub fn withdraw_liquidity(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			egress_address: ForeignChainAddress,
		) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			// Check validity of Chain and Asset
			ensure!(
				ForeignChain::from(egress_address) == ForeignChain::from(asset),
				Error::<T>::InvalidEgressAddress
			);

			// Debit the asset from the account.
			Pallet::<T>::try_debit(&account_id, asset, amount)?;

			let egress_id = T::EgressHandler::schedule_egress(asset, amount, egress_address);

			Self::deposit_event(Event::<T>::WithdrawalEgressScheduled {
				egress_id,
				asset,
				amount,
				egress_address,
			});

			Ok(())
		}

		#[pallet::weight(0)]
		pub fn register_lp_account(who: OriginFor<T>) -> DispatchResult {
			let account_id = ensure_signed(who)?;

			T::AccountRoleRegistry::register_as_liquidity_provider(&account_id)?;

			Ok(())
		}

		#[pallet::weight(0)]
		pub fn open_position(
			origin: OriginFor<T>,
			asset: Asset,
			position: TradingPosition<AssetAmount>,
		) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let maybe_liquidity =
				T::LiquidityPoolApi::get_liquidity_amount_by_position(&asset, &position);
			ensure!(maybe_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
			let (liquidity_0, liquidity_1) = maybe_liquidity.unwrap();

			// Debit the user's asset from their account.
			Pallet::<T>::try_debit(&account_id, asset, liquidity_0)?;
			Pallet::<T>::try_debit(&account_id, T::LiquidityPoolApi::STABLE_ASSET, liquidity_1)?;

			T::LiquidityPoolApi::deploy(&asset, position);

			let position_id = NextTradingPositionId::<T>::get();
			NextTradingPositionId::<T>::put(position_id.saturating_add(1u64));

			// Insert the position into
			TradingPositions::<T>::insert(
				position_id,
				UserTradingPosition::<T::AccountId, AssetAmount>::new(
					account_id.clone(),
					position,
					asset,
				),
			);

			Self::deposit_event(Event::<T>::TradingPositionOpened {
				account_id,
				position_id,
				asset,
				position,
			});

			Ok(())
		}

		#[pallet::weight(0)]
		pub fn update_position(
			origin: OriginFor<T>,
			asset: Asset,
			id: PositionId,
			new_position: TradingPosition<AssetAmount>,
		) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			TradingPositions::<T>::try_mutate(id, |maybe_position| -> DispatchResult {
				ensure!(maybe_position.is_some(), Error::<T>::InvalidTradingPosition);
				let mut current_position = maybe_position.as_mut().unwrap();
				ensure!(current_position.asset == asset, Error::<T>::InvalidTradingPosition);
				ensure!(
					current_position.account_id == account_id,
					Error::<T>::UnauthorisedToModify
				);

				let (old_liquidity_0, old_liquidity_1) =
					T::LiquidityPoolApi::retract(&asset, current_position.position);

				// Refund the debited assets for the previous position
				Pallet::<T>::credit(&account_id, asset, old_liquidity_0)?;
				Pallet::<T>::credit(
					&account_id,
					T::LiquidityPoolApi::STABLE_ASSET,
					old_liquidity_1,
				)?;

				// Debit the user's account for the new position.
				let maybe_new_liquidity =
					T::LiquidityPoolApi::get_liquidity_amount_by_position(&asset, &new_position);
				ensure!(maybe_new_liquidity.is_some(), Error::<T>::InvalidTradingPosition);
				let (new_liquidity_0, new_liquidity_1) = maybe_new_liquidity.unwrap();

				Pallet::<T>::try_debit(&account_id, asset, new_liquidity_0)?;
				Pallet::<T>::try_debit(
					&account_id,
					T::LiquidityPoolApi::STABLE_ASSET,
					new_liquidity_1,
				)?;

				// Update the pool's liquidity amount
				T::LiquidityPoolApi::deploy(&asset, new_position);

				// Update the Position storage
				current_position.position = new_position;

				Self::deposit_event(Event::<T>::TradingPositionUpdated {
					account_id,
					position_id: id,
					asset,
					new_position,
				});

				Ok(())
			})
		}

		#[pallet::weight(0)]
		pub fn close_position(who: OriginFor<T>, position_id: PositionId) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(who)?;

			// Remove the position.
			let maybe_position = TradingPositions::<T>::take(position_id);

			// Ensure the position exists and belongs to the user.
			ensure!(maybe_position.is_some(), Error::<T>::InvalidTradingPosition);
			let current_position = maybe_position.unwrap();
			ensure!(current_position.account_id == account_id, Error::<T>::UnauthorisedToModify);

			// Update the pool's liquidity amount
			let (asset_0_credit, asset_1_credit) =
				T::LiquidityPoolApi::retract(&current_position.asset, current_position.position);

			// Refund the debited assets for the previous position
			Pallet::<T>::credit(&account_id, current_position.asset, asset_0_credit)?;
			Pallet::<T>::credit(&account_id, T::LiquidityPoolApi::STABLE_ASSET, asset_1_credit)?;

			Self::deposit_event(Event::<T>::TradingPositionClosed { account_id, position_id });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn try_debit(account_id: &T::AccountId, asset: Asset, amount: AssetAmount) -> DispatchResult {
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

	fn credit(account_id: &T::AccountId, asset: Asset, amount: AssetAmount) -> DispatchResult {
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

	fn provision_account(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult {
		Pallet::<T>::credit(account_id, asset, amount).map_err(Into::into)
	}
}
