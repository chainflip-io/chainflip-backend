use crate::{mock::*, *};
use cf_traits::mocks::bid_info::MockBidInfo;
use frame_support::{assert_noop, assert_ok, traits::HandleLifetime};
use frame_system::Provider;

const ALICE: u64 = 1;
const BOB: u64 = 2;
const CHARLIE: u64 = 3;

#[test]
fn test_ensure_stake_of_validator() {
	new_test_ext().execute_with(|| {
		AccountRoles::<Test>::insert(ALICE, AccountRole::None);
		assert_ok!(Pallet::<Test>::register_account_role(
			RuntimeOrigin::signed(ALICE),
			AccountRole::Validator
		));
	});
}

#[test]
fn test_expect_validator_register_fails() {
	new_test_ext().execute_with(|| {
		MockBidInfo::set_min_bid(35);
		AccountRoles::<Test>::insert(ALICE, AccountRole::None);
		assert_noop!(
			Pallet::<Test>::register_account_role(
				RuntimeOrigin::signed(ALICE),
				AccountRole::Validator
			),
			crate::Error::<Test>::NotEnoughStake
		);
	});
}

#[test]
fn test_ensure_origin_struct() {
	new_test_ext().execute_with(|| {
		// Root and none should be invalid.
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::root()).unwrap_err();
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::none()).unwrap_err();
		EnsureValidator::<Test>::ensure_origin(RuntimeOriginFor::<Test>::root()).unwrap_err();
		EnsureValidator::<Test>::ensure_origin(RuntimeOriginFor::<Test>::none()).unwrap_err();
		EnsureLiquidityProvider::<Test>::ensure_origin(RuntimeOriginFor::<Test>::root())
			.unwrap_err();
		EnsureLiquidityProvider::<Test>::ensure_origin(RuntimeOriginFor::<Test>::none())
			.unwrap_err();

		// Validation should fail for non-existent accounts.
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(ALICE)).unwrap_err();
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(BOB)).unwrap_err();
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(CHARLIE))
			.unwrap_err();

		// Create the accounts.
		<Provider<Test> as HandleLifetime<u64>>::created(&ALICE).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&BOB).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&CHARLIE).unwrap();

		// Validation should fail for uninitalised accounts.
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(ALICE)).unwrap_err();
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(BOB)).unwrap_err();
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(CHARLIE))
			.unwrap_err();

		// Upgrade the accounts.
		Pallet::<Test>::register_as_relayer(&ALICE).unwrap();
		Pallet::<Test>::register_as_validator(&BOB).unwrap();
		Pallet::<Test>::register_as_liquidity_provider(&CHARLIE).unwrap();

		// Each account should validate as the correct account type and fail otherwise.
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(ALICE)).unwrap();
		EnsureValidator::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(ALICE))
			.unwrap_err();
		EnsureLiquidityProvider::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(ALICE))
			.unwrap_err();
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(BOB)).unwrap_err();
		EnsureValidator::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(BOB)).unwrap();
		EnsureLiquidityProvider::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(BOB))
			.unwrap_err();
		EnsureRelayer::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(CHARLIE))
			.unwrap_err();
		EnsureValidator::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(CHARLIE))
			.unwrap_err();
		EnsureLiquidityProvider::<Test>::ensure_origin(RuntimeOriginFor::<Test>::signed(CHARLIE))
			.unwrap();
	});
}

#[test]
fn test_ensure_origin_fn() {
	new_test_ext().execute_with(|| {
		// Root and none should be invalid.
		ensure_relayer::<Test>(RuntimeOriginFor::<Test>::root()).unwrap_err();
		ensure_relayer::<Test>(RuntimeOriginFor::<Test>::none()).unwrap_err();
		ensure_validator::<Test>(RuntimeOriginFor::<Test>::root()).unwrap_err();
		ensure_validator::<Test>(RuntimeOriginFor::<Test>::none()).unwrap_err();
		ensure_liquidity_provider::<Test>(RuntimeOriginFor::<Test>::root()).unwrap_err();
		ensure_liquidity_provider::<Test>(RuntimeOriginFor::<Test>::none()).unwrap_err();

		// Validation should fail for non-existent accounts.
		ensure_relayer::<Test>(RuntimeOriginFor::<Test>::signed(ALICE)).unwrap_err();
		ensure_relayer::<Test>(RuntimeOriginFor::<Test>::signed(BOB)).unwrap_err();
		ensure_relayer::<Test>(RuntimeOriginFor::<Test>::signed(CHARLIE)).unwrap_err();

		// Create the accounts.
		<Provider<Test> as HandleLifetime<u64>>::created(&ALICE).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&BOB).unwrap();
		<Provider<Test> as HandleLifetime<u64>>::created(&CHARLIE).unwrap();

		// Validation should fail for uninitalised accounts.
		ensure_relayer::<Test>(RuntimeOriginFor::<Test>::signed(ALICE)).unwrap_err();
		ensure_relayer::<Test>(RuntimeOriginFor::<Test>::signed(BOB)).unwrap_err();
		ensure_relayer::<Test>(RuntimeOriginFor::<Test>::signed(CHARLIE)).unwrap_err();

		// Upgrade the accounts.
		Pallet::<Test>::register_as_relayer(&ALICE).unwrap();
		Pallet::<Test>::register_as_validator(&BOB).unwrap();
		Pallet::<Test>::register_as_liquidity_provider(&CHARLIE).unwrap();

		// Each account should validate as the correct account type and fail otherwise.
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_relayer(
			RuntimeOriginFor::<Test>::signed(ALICE),
		)
		.unwrap();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_validator(
			RuntimeOriginFor::<Test>::signed(ALICE),
		)
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_liquidity_provider(RuntimeOriginFor::<
			Test,
		>::signed(ALICE))
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_relayer(
			RuntimeOriginFor::<Test>::signed(BOB),
		)
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_validator(
			RuntimeOriginFor::<Test>::signed(BOB),
		)
		.unwrap();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_liquidity_provider(RuntimeOriginFor::<
			Test,
		>::signed(BOB))
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_relayer(
			RuntimeOriginFor::<Test>::signed(CHARLIE),
		)
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_validator(
			RuntimeOriginFor::<Test>::signed(CHARLIE),
		)
		.unwrap_err();
		<Pallet<Test> as AccountRoleRegistry<Test>>::ensure_liquidity_provider(RuntimeOriginFor::<
			Test,
		>::signed(CHARLIE))
		.unwrap();
	});
}
