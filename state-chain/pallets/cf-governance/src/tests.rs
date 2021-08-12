use crate::mock::*;
use crate::*;
// use crate::{Call as GovernanceCall, Error as BalancesError};
use crate::mock::Governance;
use frame_support::{assert_noop, assert_ok};

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		let genesis_members = Members::<Test>::get();
		assert!(genesis_members.contains(&ALICE));
		assert!(genesis_members.contains(&BOB));
		assert!(genesis_members.contains(&CHARLES));
		assert!(genesis_members.contains(&EVE));
		assert!(genesis_members.contains(&PETER));
		assert!(genesis_members.contains(&MAX));
	});
}

#[test]
fn check_governance_restriction() {
	new_test_ext().execute_with(|| {
		// let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value(
		// 	answer,
		// )));
		assert_noop!(
			Governance::new_membership_set(mock::Origin::signed(ALICE), vec![ALICE, BOB, EVE]),
			frame_support::error::BadOrigin
		);
	});
}

#[test]
fn it_can_approve_a_proposal() {
	new_test_ext().execute_with(|| {});
}
