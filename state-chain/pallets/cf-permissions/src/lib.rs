#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
use frame_support::pallet_prelude::*;
use cf_traits::{Permissions, PermissionError, PermissionVerifier};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use cf_traits::PermissionVerifier;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The scope type
		type Scope: Default + Parameter + Member + MaybeSerializeDeserialize;
		/// The verifier
		type Verifier: PermissionVerifier<AccountId=Self::AccountId, Scope=Self::Scope>;
	}

	#[pallet::storage]
	#[pallet::getter(fn scopes)]
	pub(super) type Scopes<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		AccountId<T>,
		T::Scope,
		ValueQuery>;

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub(crate) scopes: Vec<(T::AccountId, T::Scope)>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				scopes: vec![],
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			for (account, scope) in &self.scopes {
				<Scopes<T>>::insert(account, scope);
			}
		}
	}
}

impl<T: Config> Permissions for Pallet<T> {
	type AccountId = T::AccountId;
	type Scope = T::Scope;
	type Verifier = T::Verifier;

	fn scope(account: Self::AccountId) -> Result<Self::Scope, PermissionError> {
		match Scopes::<T>::try_get(account) {
			Ok(scope) => Ok(scope),
			Err(_) => Err(PermissionError::AccountNotFound),
		}
	}

	fn set_scope(account: Self::AccountId, scope: Self::Scope) -> Result<(), PermissionError> {
		if Self::Verifier::verify_scope(&account, &scope) {
			Scopes::<T>::insert(account, scope);
			Ok(())
		} else {
			Err(PermissionError::FailedToSetScope)
		}
	}

	fn revoke(account: Self::AccountId) -> Result<(), PermissionError> {
		if Scopes::<T>::contains_key(&account) {
			Scopes::<T>::remove(account);
			Ok(())
		} else {
			Err(PermissionError::AccountNotFound)
		}
	}
}
