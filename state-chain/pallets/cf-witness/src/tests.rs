use crate::{mock::dummy::pallet as pallet_dummy, mock::*, Calls, Error, VoteMask};
use frame_support::{assert_noop, assert_ok};

#[test]
fn call_on_threshold() {
	new_test_ext().execute_with(|| {
		let answer = 42;
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value(
			answer,
		)));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call.clone()));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again, we should reach the threshold and dispatch the call.
		assert_ok!(Witnesser::witness(Origin::signed(BOBSON), call.clone()));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(answer));

		// Vote again, should count the vote but the call should not be dispatched again.
		assert_ok!(Witnesser::witness(
			Origin::signed(CHARLEMAGNE),
			call.clone()
		));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(answer));

		// Check the deposited event to get the vote count.
		let call_hash = frame_support::Hashable::blake2_256(&*call);
		let stored_vec = Calls::<Test>::get(0, call_hash).unwrap_or(vec![]);
		let votes = VoteMask::from_slice(stored_vec.as_slice()).unwrap();
		assert_eq!(votes.count_ones(), 3);
	});
}

#[test]
fn cannot_double_witness() {
	new_test_ext().execute_with(|| {
		let answer = 42;
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value(
			answer,
		)));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call.clone()));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again with the same account, should error.
		assert_noop!(
			Witnesser::witness(Origin::signed(ALISSA), call.clone()),
			Error::<Test>::DuplicateWitness
		);
	});
}

#[test]
fn only_validators_can_witness() {
	new_test_ext().execute_with(|| {
		let answer = 42;
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value(
			answer,
		)));

		// Validators can witness
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call.clone()));
		assert_ok!(Witnesser::witness(Origin::signed(BOBSON), call.clone()));
		assert_ok!(Witnesser::witness(
			Origin::signed(CHARLEMAGNE),
			call.clone()
		));

		// Other accounts can't witness
		assert_noop!(
			Witnesser::witness(Origin::signed(DEIRDRE), call.clone()),
			Error::<Test>::UnauthorizedWitness
		);
	});
}
