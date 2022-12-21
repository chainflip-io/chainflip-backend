#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
use weights::WeightInfo;

use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, BidInfo, Chainflip, QualifyNode};
use frame_support::{
	error::BadOrigin,
	pallet_prelude::DispatchResult,
	sp_runtime::traits::CheckedDiv,
	traits::{EnsureOrigin, IsType, OnKilledAccount, OnNewAccount},
};
use frame_system::{ensure_signed, pallet_prelude::OriginFor, RawOrigin};
pub use pallet::*;
use sp_std::marker::PhantomData;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::StakingInfo;
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The Flip token implementation.
		type StakeInfo: StakingInfo<AccountId = Self::AccountId, Balance = Self::Amount>;

		/// Infos about bids.
		type BidInfo: BidInfo<Balance = Self::Amount>;

		/// Weights.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub type AccountRoles<T: Config> = StorageMap<_, Identity, T::AccountId, AccountRole>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AccountRoleRegistered { account_id: T::AccountId, role: AccountRole },
	}

	#[pallet::error]
	pub enum Error<T> {
		UnknownAccount,
		AccountNotInitialised,
		/// Accounts can only be upgraded from the initial [AccountRole::Undefined] state.
		AccountRoleAlreadyRegistered,
		NotEnoughStake,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub initial_account_roles: Vec<(T::AccountId, AccountRole)>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { initial_account_roles: Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			for (account, role) in &self.initial_account_roles {
				AccountRoles::<T>::insert(account, role);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(T::WeightInfo::register_account_role())]
		pub fn register_account_role(origin: OriginFor<T>, role: AccountRole) -> DispatchResult {
			let who: T::AccountId = ensure_signed(origin)?;
			if role == AccountRole::Validator {
				ensure!(
					T::StakeInfo::total_stake_of(&who) >=
						T::BidInfo::get_min_backup_bid()
							.checked_div(&T::Amount::from(2_u32))
							.expect("Division by 2 can't fail."),
					Error::<T>::NotEnoughStake
				);
			}
			<Self as AccountRoleRegistry<T>>::register_account_role(&who, role)?;
			Ok(())
		}
	}
}

impl<T: Config> AccountRoleRegistry<T> for Pallet<T> {
	/// Register the account role for some account id.
	///
	/// Fails if an account type has already been registered for this account id.
	fn register_account_role(
		account_id: &T::AccountId,
		account_role: AccountRole,
	) -> DispatchResult {
		AccountRoles::<T>::try_mutate(account_id, |old_account_role| {
			match old_account_role.replace(account_role) {
				Some(AccountRole::None) => {
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

	fn ensure_account_role(
		origin: T::Origin,
		role: AccountRole,
	) -> Result<T::AccountId, BadOrigin> {
		match role {
			AccountRole::None => Err(BadOrigin),
			AccountRole::Validator => ensure_validator::<T>(origin),
			AccountRole::LiquidityProvider => ensure_liquidity_provider::<T>(origin),
			AccountRole::Relayer => ensure_relayer::<T>(origin),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn register_account(account_id: T::AccountId, role: AccountRole) {
		AccountRoles::<T>::insert(account_id, role);
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

impl<T: Config> QualifyNode for Pallet<T> {
	type ValidatorId = T::AccountId;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		if let Some(role) = AccountRoles::<T>::get(validator_id) {
			AccountRole::Validator == role
		} else {
			false
		}
	}
}

macro_rules! define_ensure_origin {
	( $fn_name:ident, $struct_name:ident, $account_variant:pat ) => {
		/// Implements EnsureOrigin, enforcing the correct [AccountRole].
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

		/// Ensure that the origin is signed and that the signer operates the correct [AccountRole].
		pub fn $fn_name<T: Config>(o: OriginFor<T>) -> Result<T::AccountId, BadOrigin> {
			ensure_signed(o).and_then(|account_id| match AccountRoles::<T>::get(&account_id) {
				Some($account_variant) => Ok(account_id),
				_ => Err(BadOrigin),
			})
		}
	};
}

define_ensure_origin!(ensure_relayer, EnsureRelayer, AccountRole::Relayer);
define_ensure_origin!(ensure_validator, EnsureValidator, AccountRole::Validator);
define_ensure_origin!(
	ensure_liquidity_provider,
	EnsureLiquidityProvider,
	AccountRole::LiquidityProvider
);
