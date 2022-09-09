#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use cf_primitives::AccountRole;
use cf_traits::Chainflip;
use frame_support::{
	error::BadOrigin,
	pallet_prelude::DispatchResult,
	traits::{EnsureOrigin, IsType, OnKilledAccount, OnNewAccount},
};
use frame_system::{ensure_signed, pallet_prelude::OriginFor, RawOrigin};
pub use pallet::*;
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
	pub(crate) type AccountRoles<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, T::AccountId, AccountRole>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		AccountRoleRegistered { account_id: T::AccountId, role: AccountRole },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		UnknownAccount,
		AccountNotInitialised,
		/// Accounts can only be upgraded from the initial [AccountRole::Undefined] state.
		AccountRoleAlreadyRegistered,
	}
}

impl<T: Config> Pallet<T> {
	pub fn register_as_relayer(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::Relayer)
	}

	pub fn register_as_liquidity_provider(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::LiquidityProvider)
	}

	pub fn register_as_validator(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::Validator)
	}

	/// Register the account type for some account id.
	///
	/// Fails if an account type has already been registered for this account id.
	fn register_account_role(
		account_id: &T::AccountId,
		account_role: AccountRole,
	) -> DispatchResult {
		AccountRoles::<T>::try_mutate(account_id, |old_account_role| {
			match old_account_role.replace(account_role) {
				Some(AccountRole::Undefined) => {
					Self::deposit_event(Event::AccountRoleRegistered {
						account_id: account_id.clone(),
						role: account_role,
					});
					Ok(())
				},
				Some(_) => Err(Error::<T>::AccountRoleAlreadyRegistered),
				None => Err(Error::<T>::UnknownAccount),
			}
		})
		.map_err(Into::into)
	}
}

impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(who: &T::AccountId) {
		AccountRoles::<T>::remove(who);
	}
}

impl<T: Config> OnNewAccount<T::AccountId> for Pallet<T> {
	fn on_new_account(who: &T::AccountId) {
		AccountRoles::<T>::insert(who, AccountRole::default());
	}
}

macro_rules! define_ensure_origin {
	( $fn_name:ident, $struct_name:ident, $account_variant:pat ) => {
		/// Implements EnsureOrigin, enforcing the correct [AccountType].
		pub struct $struct_name<T>(PhantomData<T>);

		impl<T: Config> EnsureOrigin<OriginFor<T>> for $struct_name<T> {
			type Success = T::AccountId;

			fn try_origin(o: OriginFor<T>) -> Result<Self::Success, OriginFor<T>> {
				match o.clone().into() {
					Ok(RawOrigin::Signed(account_id)) =>
						match AccountRoles::<T>::get(&account_id) {
							Some($account_variant) => Ok(account_id),
							_ => Err(o),
						},
					Ok(o) => Err(o.into()),
					Err(o) => Err(o),
				}
			}
		}

		/// Ensure that the origin is signed and that the signer operates the correct [AccountType].
		pub fn $fn_name<T: Config>(o: OriginFor<T>) -> Result<T::AccountId, BadOrigin> {
			ensure_signed(o).and_then(|account_id| match AccountRoles::<T>::get(&account_id) {
				Some($account_variant) => Ok(account_id),
				_ => Err(BadOrigin),
			})
		}
	};
}

define_ensure_origin!(ensure_relayer, EnsureRelayer, AccountRole::Relayer);
define_ensure_origin!(ensure_validator, EnsureValidator, AccountRole::Validator { .. });
define_ensure_origin!(
	ensure_liquidity_provider,
	EnsureLiquidityProvider,
	AccountRole::LiquidityProvider
);
