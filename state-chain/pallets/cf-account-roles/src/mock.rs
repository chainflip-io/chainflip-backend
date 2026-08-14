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

#![cfg(test)]

use crate::{self as pallet_cf_account_roles, Config};
use cf_primitives::AccountRole;
#[cfg(feature = "runtime-benchmarks")]
use cf_traits::mocks::fee_payment::MockFeePayment;
use cf_traits::{
	impl_mock_chainflip,
	mocks::deregistration_hooks::MockDeregistrationHooks as SharedMockDeregistrationHooks,
	AccountRoleRegistry, DeregistrationHooks, SpawnAccount,
};
use codec::Encode;
use frame_support::{derive_impl, StorageHasher};
use sp_runtime::DispatchError;
use std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		MockAccountRoles: pallet_cf_account_roles,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type OnNewAccount = MockAccountRoles;
	type OnKilledAccount = MockAccountRoles;
}

impl_mock_chainflip!(Test);

pub struct MockSpawnAccount;

impl SpawnAccount for MockSpawnAccount {
	type AccountId = u64;
	type Amount = u128;
	type Index = u8;

	fn spawn_sub_account(
		parent_account_id: &Self::AccountId,
		sub_account_id: Self::Index,
		_amount: Self::Amount,
	) -> Result<Self::AccountId, DispatchError> {
		use frame_support::traits::HandleLifetime;
		let sub_account_id = Self::derive_sub_account_id(parent_account_id, sub_account_id)?;
		frame_system::Provider::<Test>::created(&sub_account_id)
			.expect("Cannot fail (see implementation).");
		Ok(sub_account_id)
	}
	fn derive_sub_account_id(
		parent_account_id: &Self::AccountId,
		sub_account_index: Self::Index,
	) -> Result<Self::AccountId, DispatchError> {
		use codec::Decode;
		let bytes =
			(*parent_account_id, sub_account_index).using_encoded(frame_support::Twox128::hash);
		Ok(u64::decode(&mut &bytes[..]).expect("u64::decode should not fail; the input is valid"))
	}
}

thread_local! {
	// Records what `Pallet::<Test>::account_role` returns at the moment `on_deregistered` is
	// called, so tests can assert the role hasn't already been wiped from storage by then.
	pub static ACCOUNT_ROLE_AT_ON_DEREGISTERED: RefCell<Option<AccountRole>> = RefCell::new(None);
}

pub struct TestDeregistrationHooks;

impl DeregistrationHooks for TestDeregistrationHooks {
	type AccountId = u64;
	type Error = &'static str;

	fn check(account_id: &Self::AccountId) -> Result<(), Self::Error> {
		SharedMockDeregistrationHooks::<u64>::check(account_id)
	}

	fn on_deregistered(account_id: &Self::AccountId) {
		ACCOUNT_ROLE_AT_ON_DEREGISTERED.with(|role| {
			*role.borrow_mut() = Some(crate::Pallet::<Test>::account_role(account_id));
		});
		SharedMockDeregistrationHooks::<u64>::on_deregistered(account_id);
	}
}

impl Config for Test {
	type EnsureGovernance = frame_system::EnsureRoot<<Self as frame_system::Config>::AccountId>;
	type DeregistrationHooks = TestDeregistrationHooks;
	type RuntimeCall = RuntimeCall;
	type SpawnAccount = MockSpawnAccount;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = MockFeePayment<Self>;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers!(Test);
