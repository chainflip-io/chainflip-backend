// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;
mod mock;
mod tests;

pub mod weights;
pub use weights::WeightInfo;
pub mod migrations;
use sp_std::boxed::Box;

use cf_traits::Chainflip;

#[cfg(not(feature = "runtime-benchmarks"))]
use cf_traits::FeePayment;

use sp_runtime::{codec::Decode, DispatchError};

use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, DeregistrationCheck, SpawnAccount};
use frame_support::{
	dispatch::GetDispatchInfo,
	error::BadOrigin,
	pallet_prelude::{DispatchResult, StorageVersion},
	traits::{EnsureOrigin, HandleLifetime, IsType, OnKilledAccount, OnNewAccount, OriginTrait},
	BoundedVec, Hashable,
};
use sp_core::ConstU32;

use sp_runtime::traits::TrailingZeroInput;

use sp_runtime::traits::Dispatchable;

pub type SubAccountIndex = u8;

use frame_system::{ensure_signed, pallet_prelude::OriginFor, RawOrigin};
pub use pallet::*;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, vec::Vec};

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);
pub const MAX_LENGTH_FOR_VANITY_NAME: u32 = 64;

type VanityName = BoundedVec<u8, ConstU32<MAX_LENGTH_FOR_VANITY_NAME>>;
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::DeregistrationCheck;
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config + cf_traits::Chainflip {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
		type DeregistrationCheck: DeregistrationCheck<
			AccountId = <Self as frame_system::Config>::AccountId,
		>;
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ From<frame_system::Call<Self>>
			+ From<Call<Self>>
			+ GetDispatchInfo;
		type SpawnAccount: SpawnAccount<
			AccountId = <Self as frame_system::Config>::AccountId,
			Amount = <Self as Chainflip>::Amount,
		>;
		#[cfg(feature = "runtime-benchmarks")]
		type FeePayment: cf_traits::FeePayment<
			Amount = <Self as Chainflip>::Amount,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(PALLET_VERSION)]
	pub struct Pallet<T>(PhantomData<T>);

	// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
	// !!!!!!!!!!!!!!!!!!!! IMPORTANT: Care must be taken when changing this !!!!!!!!!!!!!!!!!!!!
	// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
	// !!! This is because this is used before the version compatibility checks in the engine !!!
	// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
	#[pallet::storage]
	pub type AccountRoles<T: Config> = StorageMap<_, Identity, T::AccountId, AccountRole>;

	/// Vanity names of the validators stored as a Map with the current validator IDs as key.
	#[pallet::storage]
	#[pallet::getter(fn vanity_names)]
	pub type VanityNames<T: Config> =
		StorageValue<_, BTreeMap<T::AccountId, VanityName>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AccountRoleRegistered {
			account_id: T::AccountId,
			role: AccountRole,
		},
		AccountRoleDeregistered {
			account_id: T::AccountId,
			role: AccountRole,
		},
		/// Vanity Name for a node has been set.
		VanityNameSet {
			account_id: T::AccountId,
			name: VanityName,
		},
		SubAccountCreated {
			account_id: T::AccountId,
			sub_account_id: T::AccountId,
			sub_account_index: SubAccountIndex,
		},
		SubAccountCallExecuted {
			account_id: T::AccountId,
			sub_account_id: T::AccountId,
			sub_account_index: SubAccountIndex,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account has never been created.
		UnknownAccount,
		/// The account already has a registered role.
		AccountRoleAlreadyRegistered,
		/// Invalid characters in the name.
		InvalidCharactersInName,
		/// Failed to execute the call of a sub-account.
		FailedToExecuteCallOnBehalfOfSubAccount,
		/// Failed to derive the sub-account id.
		SubAccountIdDerivationFailed,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub initial_account_roles: Vec<(T::AccountId, AccountRole)>,
		pub genesis_vanity_names: BTreeMap<T::AccountId, VanityName>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				initial_account_roles: Default::default(),
				genesis_vanity_names: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (account, role) in &self.initial_account_roles {
				Pallet::<T>::register_account_role(account, *role)
					.expect("Genesis account role registration should not fail.");
			}
			VanityNames::<T>::put(&self.genesis_vanity_names);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Allow a node to set a "Vanity Name" for themselves. This is functionally
		/// useless but can be used to make the network a bit more friendly for
		/// observers. Names are required to be <= MAX_LENGTH_FOR_VANITY_NAME (64)
		/// UTF-8 bytes.
		///
		/// The dispatch origin of this function must be signed.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_vanity_name())]
		pub fn set_vanity_name(origin: OriginFor<T>, name: VanityName) -> DispatchResult {
			let account_id = ensure_signed(origin)?;
			ensure!(sp_std::str::from_utf8(&name).is_ok(), Error::<T>::InvalidCharactersInName);
			VanityNames::<T>::mutate(|vanity_names| {
				vanity_names.insert(account_id.clone(), name.clone());
			});
			Self::deposit_event(Event::VanityNameSet { account_id, name });
			Ok(())
		}

		/// Derives a sub-account by the given origin account id and a sub-account index.
		/// Stores the account id against the sub-account index and origin account id.
		///
		/// The sub account is unmutual associated with the origin/parent account and the creation
		/// fails if the sub-account already exists for the given index. It's possible to derive 256
		/// sub-accounts per parent account. The parent account can execute calls on behalf of the
		/// sub-account (see as_sub_account).
		///
		/// Events:
		/// - `SubAccountCreated` if the sub-account is created.
		///
		/// Errors:
		/// - `SubAccountAlreadyExists` if the sub-account already exists for the given index.
		/// - `SubAccountIdDerivationFailed` if the sub-account id derivation fails.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::derive_sub_account())]
		pub fn derive_sub_account(
			origin: OriginFor<T>,
			sub_account_index: SubAccountIndex,
			amount: Option<T::Amount>,
		) -> DispatchResult {
			let account_id = ensure_signed(origin)?;
			let sub_account_id =
				Self::derive_sub_account_id(account_id.clone(), sub_account_index)?;
			T::SpawnAccount::spawn_sub_account(account_id.clone(), sub_account_id.clone(), amount)?;
			Self::deposit_event(Event::SubAccountCreated {
				account_id: account_id.clone(),
				sub_account_id,
				sub_account_index,
			});
			Ok(())
		}

		/// Executes a call on behalf of a sub-account. The account is getting identified by the
		/// passed sub-account index.
		///
		/// The call is executed with the sub-account's account id as the dispatch origin.
		///
		/// Events:
		/// - `SubAccountCallExecuted` if the call is executed.
		///
		/// Errors:
		/// - `UnknownAccount` if the sub-account is not found.
		/// - `FailedToExecuteCallOnBehalfOfSubAccount` if the call fails.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::as_sub_account().saturating_add(call.get_dispatch_info().weight))]
		pub fn as_sub_account(
			origin: OriginFor<T>,
			sub_account_index: SubAccountIndex,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			let mut origin = origin;
			let account_id = ensure_signed(origin.clone())?;
			let sub_account_id =
				Self::derive_sub_account_id(account_id.clone(), sub_account_index)?;
			ensure!(
				T::SpawnAccount::does_account_exist(&sub_account_id),
				Error::<T>::UnknownAccount
			);
			origin.set_caller_from(frame_system::RawOrigin::Signed(sub_account_id.clone()));
			match call.clone().dispatch(origin) {
				Ok(_) => {
					Self::deposit_event(Event::SubAccountCallExecuted {
						account_id: account_id.clone(),
						sub_account_id: sub_account_id.clone(),
						sub_account_index,
					});
					Ok(())
				},
				Err(e) => Err(e.error),
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn derive_sub_account_id(
		parent_account_id: T::AccountId,
		sub_account_index: SubAccountIndex,
	) -> Result<T::AccountId, DispatchError> {
		Ok(Decode::decode(&mut TrailingZeroInput::new(
			(*b"chainflip/subaccount", parent_account_id.clone(), sub_account_index)
				.blake2_256()
				.as_ref(),
		))
		.map_err(|_| Error::<T>::SubAccountIdDerivationFailed)?)
	}
}

impl<T: Config> AccountRoleRegistry<T> for Pallet<T> {
	/// Register the account role for some account id.
	///
	/// Fails if an account role has already been registered for this account id, or if the account
	/// doesn't exist.
	#[frame_support::transactional]
	fn register_account_role(
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
		})?;
		frame_system::Consumer::<T>::created(account_id)?;
		Ok(())
	}

	/// Deregister the account role for some account id.
	///
	/// This is required in order to be able to redeem all funds. Callers should ensure that any
	/// state associated with the account is cleaned up before calling this function. For example:
	/// LPs should withdraw all liquidity.
	#[frame_support::transactional]
	fn deregister_account_role(
		account_id: &T::AccountId,
		account_role: AccountRole,
	) -> DispatchResult {
		T::DeregistrationCheck::check(account_id).map_err(Into::into)?;
		AccountRoles::<T>::try_mutate(account_id, |role| {
			role.replace(AccountRole::Unregistered)
				.filter(|r| *r == account_role)
				.ok_or(Error::<T>::UnknownAccount)
		})?;
		<frame_system::Pallet<T>>::dec_consumers(account_id);

		Self::deposit_event(Event::AccountRoleDeregistered {
			account_id: account_id.clone(),
			role: account_role,
		});

		Ok(())
	}

	fn account_role(id: &T::AccountId) -> AccountRole {
		AccountRoles::<T>::get(id).unwrap_or_default()
	}

	fn has_account_role(id: &T::AccountId, role: AccountRole) -> bool {
		Self::account_role(id) == role
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
			AccountRole::Operator => ensure_operator::<T>(origin),
		}
	}
}

impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(who: &T::AccountId) {
		AccountRoles::<T>::remove(who);
		let _ = VanityNames::<T>::try_mutate(|vanity_names| vanity_names.remove(who).ok_or(()));
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

define_ensure_origin!(ensure_operator, EnsureOperator, AccountRole::Operator);
