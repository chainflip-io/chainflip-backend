use frame_support::{assert_noop, assert_ok};

use crate::{mock::*, pallet, Members, Pallet};

use crate as pallet_cf_governance;

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
		assert_noop!(
			Governance::new_membership_set(Origin::signed(ALICE), vec![ALICE, BOB, EVE]),
			frame_support::error::BadOrigin
		);
	});
}

#[test]
fn it_can_propose_a_governance_extrinsic() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Governance(
			pallet_cf_governance::Call::<Test>::new_membership_set(vec![ALICE, BOB, EVE]),
		));
		assert_ok!(Governance::propose_governance_extrinsic(
			Origin::signed(ALICE),
			call
		));
	});
}

#[test]
fn it_can_approve_a_proposal() {
	new_test_ext().execute_with(|| {});
}
