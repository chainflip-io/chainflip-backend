#![cfg(test)]

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok, traits::HandleLifetime};
use frame_system::Provider;

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
				vanity.clone().into_bytes()
			));
			assert_eq!(
				sp_std::str::from_utf8(VanityNames::<Test>::get().get(account_id).unwrap()).unwrap(),
				vanity
			);
		}
		assert_eq!(VanityNames::<Test>::get().len(), ACCOUNT_IDS.len());

		// Test invalid vanity names
		assert_noop!(
			AccountRolesPallet::set_vanity_name(RuntimeOrigin::signed(1), [0xfe, 0xff].to_vec()),
			Error::<Test>::InvalidCharactersInName
		);
		assert_noop!(
			AccountRolesPallet::set_vanity_name(
				RuntimeOrigin::signed(1),
				"Validator Name too longggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg".as_bytes().to_vec()
			),
			Error::<Test>::NameTooLong
		);

		// Test removal of a vanity name
		AccountRolesPallet::on_killed_account(&ACCOUNT_IDS[0]);
		assert_eq!(VanityNames::<Test>::get().len(), ACCOUNT_IDS.len()-1, "Vanity name should of been removed when account was killed");
	});
}
