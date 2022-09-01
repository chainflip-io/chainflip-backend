#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use cf_traits::{
	account_data::{AccountType, ValidatorAccountData, ValidatorAccountState},
	Chainflip,
};
use frame_support::traits::{OnKilledAccount, OnNewAccount};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_runtime::DispatchError;
use sp_std::marker::PhantomData;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	pub type AccountTypes<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, T::AccountId, AccountType>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		AccountTypeRegistered { account_id: T::AccountId, account_type: AccountType },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		UnknownAccount,
		InvalidAccountType,
		AccountNotInitialised,
		/// Accounts can only be upgraded from the initial [AccountType::Undefined] state.
		AccountTypeAlreadyRegistered,
	}
}

impl<T: Config> Pallet<T> {
	pub fn register_as_relayer(account_id: &T::AccountId) -> Result<(), Error<T>> {
		Self::register_account_type(account_id, AccountType::Relayer)
	}

	pub fn register_as_lp(account_id: &T::AccountId) -> Result<(), Error<T>> {
		Self::register_account_type(account_id, AccountType::LiquidityProvider)
	}

	pub fn register_as_validator(account_id: &T::AccountId) -> Result<(), Error<T>> {
		Self::register_account_type(
			account_id,
			AccountType::Validator(ValidatorAccountData {
				state: Default::default(),
				is_active_bidder: false,
			}),
		)
	}

	/// Register the account type for some account id.
	///
	/// Fails if an account type has already been registered for this account id.
	fn register_account_type(
		account_id: &T::AccountId,
		account_type: AccountType,
	) -> Result<(), Error<T>> {
		AccountTypes::<T>::try_mutate(account_id, |old_account_type| {
			match old_account_type.replace(account_type) {
				Some(AccountType::Undefined) => Ok(()),
				Some(_) => Err(Error::AccountTypeAlreadyRegistered),
				None => Err(Error::UnknownAccount),
			}
		})
	}

	/// Try to apply a mutation to the account data.
	///
	/// Fails if the account has not been initialised. If the provided closure returns an `Err`,
	/// does not mutate.
	fn try_mutate_validator_state<
		R,
		E: Into<DispatchError>,
		F: FnOnce(&mut ValidatorAccountData) -> Result<R, E>,
	>(
		account_id: &T::AccountId,
		f: F,
	) -> Result<R, DispatchError> {
		AccountTypes::<T>::try_mutate(account_id, |maybe_account_data| {
			match maybe_account_data.as_mut() {
				Some(AccountType::Validator(ref mut validator_account_data)) =>
					f(validator_account_data).map_err(Into::into),
				_ => Err(Error::<T>::InvalidAccountType.into()),
			}
		})
	}
}

impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(who: &T::AccountId) {
		AccountTypes::<T>::remove(who);
	}
}

impl<T: Config> OnNewAccount<T::AccountId> for Pallet<T> {
	fn on_new_account(who: &T::AccountId) {
		AccountTypes::<T>::insert(who, AccountType::default());
	}
}
