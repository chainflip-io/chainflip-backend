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

use super::{MockPallet, MockPalletStorage};
use crate::{AccountRoleRegistry, DeregistrationHooks, VanityName};
use cf_primitives::AccountRole;
use frame_support::{pallet_prelude::DispatchError, sp_runtime::DispatchResult};
use frame_system::{ensure_signed, Config};
use sp_std::marker::PhantomData;

pub struct MockAccountRoleRegistry;

impl MockPallet for MockAccountRoleRegistry {
	const PREFIX: &'static [u8] = b"MockAccountRoleRegistry";
}

const ACCOUNT_ROLES: &[u8] = b"AccountRoles";
const VANITY_NAMES: &[u8] = b"VanityNames";
pub const ALREADY_REGISTERED_ERROR: DispatchError =
	DispatchError::Other("Account already registered");

impl<T: Config> AccountRoleRegistry<T> for MockAccountRoleRegistry {
	fn register_account_role(
		account_id: &<T as frame_system::Config>::AccountId,
		role: AccountRole,
	) -> DispatchResult {
		if <Self as MockPalletStorage>::get_storage::<_, AccountRole>(ACCOUNT_ROLES, account_id)
			.is_some()
		{
			return Err(ALREADY_REGISTERED_ERROR)
		}
		<Self as MockPalletStorage>::put_storage(ACCOUNT_ROLES, account_id, role);
		Ok(())
	}

	fn deregister_account_role(
		account_id: &<T as Config>::AccountId,
		role: AccountRole,
	) -> DispatchResult {
		match <Self as MockPalletStorage>::take_storage::<_, AccountRole>(ACCOUNT_ROLES, account_id)
		{
			Some(r) if r == role => Ok(()),
			Some(_) => Err("Account role mismatch".into()),
			_ => Err("Account not registered".into()),
		}
	}

	fn set_vanity_name(who: &<T as Config>::AccountId, name: VanityName) -> DispatchResult {
		<Self as MockPalletStorage>::put_storage(VANITY_NAMES, who, name);
		Ok(())
	}

	fn account_role(who: &<T as Config>::AccountId) -> AccountRole {
		<Self as MockPalletStorage>::get_storage::<_, AccountRole>(ACCOUNT_ROLES, who)
			.unwrap_or(AccountRole::Unregistered)
	}

	fn has_account_role(who: &<T as Config>::AccountId, role: AccountRole) -> bool {
		<Self as MockPalletStorage>::get_storage::<_, AccountRole>(ACCOUNT_ROLES, who)
			.unwrap_or(AccountRole::Unregistered) ==
			role
	}

	fn ensure_account_role(
		origin: <T as frame_system::Config>::RuntimeOrigin,
		role: AccountRole,
	) -> Result<<T as frame_system::Config>::AccountId, frame_support::sp_runtime::traits::BadOrigin>
	{
		match ensure_signed(origin) {
			Ok(account_id) => {
				let account_role = <Self as MockPalletStorage>::get_storage::<_, AccountRole>(
					ACCOUNT_ROLES,
					account_id.clone(),
				)
				.unwrap_or(AccountRole::Unregistered);
				if account_role == role {
					Ok(account_id)
				} else {
					Err(frame_support::sp_runtime::traits::BadOrigin)
				}
			},
			Err(_) => Err(frame_support::sp_runtime::traits::BadOrigin),
		}
	}
}

/// Like [MockAccountRoleRegistry], but runs the given [DeregistrationHooks] on deregistration, in
/// the same order as the real registry. Shares storage with [MockAccountRoleRegistry].
pub struct MockAccountRoleRegistryWithHooks<H>(PhantomData<H>);

impl<T: Config, H: DeregistrationHooks<AccountId = T::AccountId>> AccountRoleRegistry<T>
	for MockAccountRoleRegistryWithHooks<H>
{
	fn register_account_role(account_id: &T::AccountId, role: AccountRole) -> DispatchResult {
		<MockAccountRoleRegistry as AccountRoleRegistry<T>>::register_account_role(account_id, role)
	}

	fn deregister_account_role(account_id: &T::AccountId, role: AccountRole) -> DispatchResult {
		H::check(account_id).map_err(Into::into)?;
		<MockAccountRoleRegistry as AccountRoleRegistry<T>>::deregister_account_role(
			account_id, role,
		)?;
		H::on_deregistered(account_id, role);
		Ok(())
	}

	fn set_vanity_name(who: &T::AccountId, name: VanityName) -> DispatchResult {
		<MockAccountRoleRegistry as AccountRoleRegistry<T>>::set_vanity_name(who, name)
	}

	fn account_role(who: &T::AccountId) -> AccountRole {
		<MockAccountRoleRegistry as AccountRoleRegistry<T>>::account_role(who)
	}

	fn has_account_role(who: &T::AccountId, role: AccountRole) -> bool {
		<MockAccountRoleRegistry as AccountRoleRegistry<T>>::has_account_role(who, role)
	}

	fn ensure_account_role(
		origin: T::RuntimeOrigin,
		role: AccountRole,
	) -> Result<T::AccountId, frame_support::sp_runtime::traits::BadOrigin> {
		<MockAccountRoleRegistry as AccountRoleRegistry<T>>::ensure_account_role(origin, role)
	}
}
