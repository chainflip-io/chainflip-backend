#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;
mod mock;
mod tests;

pub mod weights;
use weights::WeightInfo;

use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_support::{
	error::BadOrigin,
	pallet_prelude::DispatchResult,
	traits::{EnsureOrigin, IsType, OnKilledAccount, OnNewAccount},
};
use frame_system::{ensure_signed, pallet_prelude::OriginFor, RawOrigin};
pub use pallet::*;
use sp_std::{marker::PhantomData, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	// TODO: Remove once swapping is enabled and stabilised.
	// Acts to flag the swapping features. If there are no Broker accounts or
	// LP accounts, then the swapping features are disabled.
	#[pallet::storage]
	#[pallet::getter(fn swapping_enabled)]
	pub type SwappingEnabled<T: Config> = StorageValue<_, bool, ValueQuery>;

	// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
	// !!!!!!!!!!!!!!!!!!!! IMPORTANT: Care must be taken when changing this !!!!!!!!!!!!!!!!!!!!
	// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
	// !!! This is because this is used before the version compatibility checks in the engine !!!
	// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
	#[pallet::storage]
	pub type AccountRoles<T: Config> = StorageMap<_, Identity, T::AccountId, AccountRole>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AccountRoleRegistered { account_id: T::AccountId, role: AccountRole },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account has never been created.
		UnknownAccount,
		/// The account already has a registered role.
		AccountRoleAlreadyRegistered,
		/// Initially when swapping features are deployed to the chain, they will be disabled.
		SwappingDisabled,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub initial_account_roles: Vec<(T::AccountId, AccountRole)>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { initial_account_roles: Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let mut should_enable_swapping = false;
			for (account, role) in &self.initial_account_roles {
				AccountRoles::<T>::insert(account, role);
				if *role == AccountRole::LiquidityProvider || *role == AccountRole::Broker {
					should_enable_swapping = true;
				}
			}
			if should_enable_swapping {
				SwappingEnabled::<T>::put(true);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// TODO: Remove this function after the feature is deployed and stabilised.
		// Once the swapping features are enabled, they can't be disabled.
		// If they have been enabled, it's possible accounts have already registered as Brokers or
		// LPs. Thus, disabling this flag is not an indicator of whether the public can swap.
		// Governance can bypass this by calling `gov_register_account_role`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::enable_swapping())]
		pub fn enable_swapping(origin: OriginFor<T>) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			SwappingEnabled::<T>::put(true);
			Ok(())
		}

		// TODO: Remove this function after swapping is deployed and stabilised.
		/// Bypass the Swapping Enabled check. This allows governance to enable swapping
		/// features for some controlled accounts.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::gov_register_account_role())]
		pub fn gov_register_account_role(
			origin: OriginFor<T>,
			account: T::AccountId,
			role: AccountRole,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			Self::register_account_role_unprotected(&account, role)?;
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	// WARN: This is not protected by the Swapping feature flag.
	// In most cases the correct function to use is `register_account_role`.
	fn register_account_role_unprotected(
		account_id: &T::AccountId,
		account_role: AccountRole,
	) -> DispatchResult {
		AccountRoles::<T>::try_mutate(account_id, |old_account_role| {
			match old_account_role.replace(account_role) {
				Some(AccountRole::Unregistered) => {
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

impl<T: Config> AccountRoleRegistry<T> for Pallet<T> {
	/// Register the account role for some account id.
	///
	/// Fails if an account type has already been registered for this account id.
	/// Or if Swapping is not yet enabled.
	fn register_account_role(
		account_id: &T::AccountId,
		account_role: AccountRole,
	) -> DispatchResult {
		match account_role {
			AccountRole::Broker | AccountRole::LiquidityProvider
				if !SwappingEnabled::<T>::get() =>
				Err(Error::<T>::SwappingDisabled.into()),
			_ => Self::register_account_role_unprotected(account_id, account_role),
		}
	}

	fn has_account_role(id: &T::AccountId, role: AccountRole) -> bool {
		AccountRoles::<T>::get(id).unwrap_or_default() == role
	}

	fn ensure_account_role(
		origin: T::RuntimeOrigin,
		role: AccountRole,
	) -> Result<T::AccountId, BadOrigin> {
		match role {
			AccountRole::Unregistered => Err(BadOrigin),
			AccountRole::Validator => ensure_validator::<T>(origin),
			AccountRole::LiquidityProvider => ensure_liquidity_provider::<T>(origin),
			AccountRole::Broker => ensure_broker::<T>(origin),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn register_account(account_id: &T::AccountId, role: AccountRole) {
		AccountRoles::<T>::insert(account_id, role);
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn get_account_role(account_id: &T::AccountId) -> AccountRole {
		AccountRoles::<T>::get(account_id).unwrap_or_default()
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

			#[cfg(feature = "runtime-benchmarks")]
			fn try_successful_origin() -> Result<<T as frame_system::Config>::RuntimeOrigin, ()> {
				// Can't return a default account id with the correct role.
				Err(())
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

define_ensure_origin!(ensure_broker, EnsureBroker, AccountRole::Broker);
define_ensure_origin!(ensure_validator, EnsureValidator, AccountRole::Validator);
define_ensure_origin!(
	ensure_liquidity_provider,
	EnsureLiquidityProvider,
	AccountRole::LiquidityProvider
);
