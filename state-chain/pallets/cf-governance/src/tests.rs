use crate::{mock::*, pallet, Members, OnGoingProposals, Pallet};
use frame_support::{assert_noop, assert_ok, traits::OnInitialize};

use crate as pallet_cf_governance;

fn next_block() {
	System::set_block_number(System::block_number() + 1);
	<Governance as OnInitialize<u64>>::on_initialize(System::block_number());
}

#[test]
fn it_can_use_genesis_config() {
	new_test_ext().execute_with(|| {
		let genesis_members = Members::<Test>::get();
		assert!(genesis_members.contains(&ALICE));
		assert!(genesis_members.contains(&BOB));
		assert!(genesis_members.contains(&CHARLES));
	});
}

#[test]
fn it_can_detect_governance_restriction() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Governance::new_membership_set(Origin::signed(ALICE), vec![EVE, PETER, MAX]),
			frame_support::error::BadOrigin
		);
	});
}

#[test]
fn it_can_propose_a_governance_extrinsic_and_execute_it() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Governance(
			pallet_cf_governance::Call::<Test>::new_membership_set(vec![EVE, PETER, MAX]),
		));
		assert_ok!(Governance::propose_governance_extrinsic(
			Origin::signed(ALICE),
			call
		));
		assert_eq!(OnGoingProposals::<Test>::get().len(), 1);
		next_block();
		assert_ok!(Governance::approve(Origin::signed(BOB), 0));
		next_block();
		assert_ok!(Governance::approve(Origin::signed(CHARLES), 0));
		next_block();
		let genesis_members = Members::<Test>::get();
		assert!(genesis_members.contains(&EVE));
		assert!(genesis_members.contains(&PETER));
		assert!(genesis_members.contains(&MAX));
		assert_eq!(OnGoingProposals::<Test>::get().len(), 0);
	});
}

#[test]
fn it_can_detect_expiry_date() {
	new_test_ext().execute_with(|| {});
}
