#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use cf_traits::Chainflip;
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::marker::PhantomData;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use types::AccountType;

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
		InvalidAccountType,
		AccountNotInitialised,
		/// Accounts can only be upgraded from the initial [AccountType::Undefined] state.
		InvalidAccountTypeUpgrade,
	}
}
