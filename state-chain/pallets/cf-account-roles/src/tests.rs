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

use crate::{mock::*, *};
use cf_traits::mocks::deregistration_check::MockDeregistrationCheck;
use frame_support::{assert_noop, assert_ok, traits::HandleLifetime};
use frame_system::Provider;

use crate as pallet_cf_account_roles;

use cf_test_utilities::assert_has_matching_event;

type AccountRolesPallet = Pallet<Test>;

const ALICE: u64 = 1;
const BOB: u64 = 2;
const CHARLIE: u64 = 3;

#[test]
fn test_ensure_origin_struct() {
	new_test_ext().execute_with(|| {
		// Root and none should be invalid.
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::root()).unwrap_err();
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::none()).unwrap_err();
		EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::root()).unwrap_err();
		EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::none()).unwrap_err();
		EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::root()).unwrap_err();
		EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::none()).unwrap_err();

		// Validation should fail for non-existent accounts.
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE)).unwrap_err();
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap_err();
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();

		// Create the accounts.
		<Provider<Test> as HandleLifetime<u64>>::created(&ALICE).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&BOB).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&CHARLIE).unwrap();

		// Validation should fail for uninitalised accounts.
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE)).unwrap_err();
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap_err();
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();

		// Upgrade the accounts.
		AccountRolesPallet::register_as_broker(&ALICE).unwrap();
		AccountRolesPallet::register_as_validator(&BOB).unwrap();
		AccountRolesPallet::register_as_liquidity_provider(&CHARLIE).unwrap();

		// Each account should validate as the correct account type and fail otherwise.
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE)).unwrap();
		EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE)).unwrap_err();
		EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::signed(ALICE))
			.unwrap_err();
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap_err();
		EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap();
		EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::signed(BOB)).unwrap_err();
		EnsureBroker::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();
		EnsureValidator::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();
		EnsureLiquidityProvider::<Test>::ensure_origin(OriginFor::<Test>::signed(CHARLIE)).unwrap();
	});
}

#[test]
fn test_ensure_origin_fn() {
	new_test_ext().execute_with(|| {
		// Root and none should be invalid.
		ensure_broker::<Test>(OriginFor::<Test>::root()).unwrap_err();
		ensure_broker::<Test>(OriginFor::<Test>::none()).unwrap_err();
		ensure_validator::<Test>(OriginFor::<Test>::root()).unwrap_err();
		ensure_validator::<Test>(OriginFor::<Test>::none()).unwrap_err();
		ensure_liquidity_provider::<Test>(OriginFor::<Test>::root()).unwrap_err();
		ensure_liquidity_provider::<Test>(OriginFor::<Test>::none()).unwrap_err();

		// Validation should fail for non-existent accounts.
		ensure_broker::<Test>(OriginFor::<Test>::signed(ALICE)).unwrap_err();
		ensure_broker::<Test>(OriginFor::<Test>::signed(BOB)).unwrap_err();
		ensure_broker::<Test>(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();

		// Create the accounts.
		<Provider<Test> as HandleLifetime<u64>>::created(&ALICE).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&BOB).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&CHARLIE).unwrap();

		// Validation should fail for uninitalised accounts.
		ensure_broker::<Test>(OriginFor::<Test>::signed(ALICE)).unwrap_err();
		ensure_broker::<Test>(OriginFor::<Test>::signed(BOB)).unwrap_err();
		ensure_broker::<Test>(OriginFor::<Test>::signed(CHARLIE)).unwrap_err();

		// Upgrade the accounts.
		AccountRolesPallet::register_as_broker(&ALICE).unwrap();
		AccountRolesPallet::register_as_validator(&BOB).unwrap();
		AccountRolesPallet::register_as_liquidity_provider(&CHARLIE).unwrap();

		// Each account should validate as the correct account type and fail otherwise.
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_broker(OriginFor::<Test>::signed(
			ALICE,
		))
		.unwrap();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_validator(OriginFor::<Test>::signed(
			ALICE,
		))
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_liquidity_provider(
			OriginFor::<Test>::signed(ALICE),
		)
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_broker(OriginFor::<Test>::signed(BOB))
			.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_validator(OriginFor::<Test>::signed(
			BOB,
		))
		.unwrap();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_liquidity_provider(
			OriginFor::<Test>::signed(BOB),
		)
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_broker(OriginFor::<Test>::signed(
			CHARLIE,
		))
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_validator(OriginFor::<Test>::signed(
			CHARLIE,
		))
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_liquidity_provider(
			OriginFor::<Test>::signed(CHARLIE),
		)
		.unwrap();
	});
}

#[test]
fn test_setting_vanity_names_() {
	new_test_ext().execute_with(|| {
		assert_eq!(VanityNames::<Test>::get().len(), 0, "Vanity names should be empty before test");

		// Set vanity names for 4 accounts
		const ACCOUNT_IDS: [u64; 4] = [123, 456, 789, 101112];
		for (i, account_id) in ACCOUNT_IDS.iter().enumerate() {
			let vanity = format!("Test Account {i}");
			assert_ok!(AccountRolesPallet::set_vanity_name(
				RuntimeOrigin::signed(*account_id),
				vanity.clone().into_bytes().try_into().unwrap()
			));
			assert_eq!(
				sp_std::str::from_utf8(VanityNames::<Test>::get().get(account_id).unwrap())
					.unwrap(),
				vanity
			);
		}
		assert_eq!(VanityNames::<Test>::get().len(), ACCOUNT_IDS.len());

		assert_noop!(
			AccountRolesPallet::set_vanity_name(
				RuntimeOrigin::signed(100),
				BoundedVec::try_from(vec![0xfe, 0xff]).unwrap()
			),
			Error::<Test>::InvalidCharactersInName
		);

		// Test removal of a vanity name
		AccountRolesPallet::on_killed_account(&ACCOUNT_IDS[0]);
		assert_eq!(
			VanityNames::<Test>::get().len(),
			ACCOUNT_IDS.len() - 1,
			"Vanity name should of been removed when account was killed"
		);
	});
}

#[test]
fn deregistration_checks() {
	new_test_ext().execute_with(|| {
		// Create and register some accounts.
		const ROLE: AccountRole = AccountRole::Broker;
		<Provider<Test> as HandleLifetime<u64>>::created(&ALICE).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&BOB).unwrap();
		AccountRolesPallet::register_account_role(&ALICE, ROLE).unwrap();
		AccountRolesPallet::register_account_role(&BOB, ROLE).unwrap();

		MockDeregistrationCheck::set_should_fail(&ALICE, true);

		assert!(<Pallet<Test> as AccountRoleRegistry<_>>::deregister_account_role(&ALICE, ROLE)
			.is_err());
		assert!(
			<Pallet<Test> as AccountRoleRegistry<_>>::deregister_account_role(&BOB, ROLE).is_ok()
		);
	});
}

#[test]
fn derive_sub_account() {
	new_test_ext().execute_with(|| {
		assert_ok!(AccountRolesPallet::derive_sub_account(RuntimeOrigin::signed(ALICE), 0));
		assert_has_matching_event!(
			Test,
			RuntimeEvent::MockAccountRoles(Event::SubAccountCreated {
				account_id: ALICE,
				sub_account_id,
				sub_account_index: 0,
			}) if *sub_account_id == SubAccounts::<Test>::get(ALICE, 0).unwrap()
		);
	});
}

#[test]
fn can_not_register_sub_account_twice() {
	new_test_ext().execute_with(|| {
		assert_ok!(AccountRolesPallet::derive_sub_account(RuntimeOrigin::signed(ALICE), 0));
		assert_noop!(
			AccountRolesPallet::derive_sub_account(RuntimeOrigin::signed(ALICE), 0),
			Error::<Test>::SubAccountAlreadyExists
		);
	});
}

#[test]
fn execute_as_sub_account() {
	new_test_ext().execute_with(|| {
		const SUB_ACCOUNT_INDEX: u8 = 1;
		assert_ok!(AccountRolesPallet::derive_sub_account(
			RuntimeOrigin::signed(ALICE),
			SUB_ACCOUNT_INDEX
		));
		let sub_account_id = SubAccounts::<Test>::get(ALICE, SUB_ACCOUNT_INDEX).unwrap();
		assert_ok!(AccountRolesPallet::as_sub_account(
			RuntimeOrigin::signed(ALICE),
			SUB_ACCOUNT_INDEX,
			Box::new(RuntimeCall::MockAccountRoles(
				pallet_cf_account_roles::Call::<Test>::set_vanity_name {
					name: "Test Account".to_string().into_bytes().try_into().unwrap()
				},
			))
		));
		assert_has_matching_event!(
			Test,
			RuntimeEvent::MockAccountRoles(Event::SubAccountCallExecuted {
				account_id: ALICE,
				sub_account_id,
				sub_account_index: SUB_ACCOUNT_INDEX,
				call: _,
			}) if *sub_account_id == SubAccounts::<Test>::get(ALICE, SUB_ACCOUNT_INDEX).unwrap()
		);
		assert!(VanityNames::<Test>::get().contains_key(&sub_account_id));
	});
}
