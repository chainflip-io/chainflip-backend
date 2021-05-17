use crate::{mock::dummy::pallet as pallet_dummy, mock::*, Votes, Error, VoteMask, Config, Pallet};
use frame_support::{assert_noop, assert_ok, dispatch::DispatchResultWithPostInfo};

fn witness_call<T: Config>(who: T::AccountId, call: <T as Config>::Call) -> DispatchResultWithPostInfo {
	<Pallet<T> as cf_traits::Witnesser>::witness(who.into(), call)
}

#[test]
fn call_on_threshold() {
	new_test_ext().execute_with(|| {
		let answer = 42;
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value(
			answer,
		)));
		let call_hash = Witnesser::call_hash(call.as_ref());

		// Register the call.
		assert_ok!(Witnesser::register(Origin::none(), call));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call_hash));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again, we should reach the threshold and dispatch the call.
		assert_ok!(Witnesser::witness(Origin::signed(BOBSON), call_hash));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(answer));

		// Vote again, should count the vote but the call should not be dispatched again.
		assert_ok!(Witnesser::witness(
			Origin::signed(CHARLEMAGNE),
			call_hash
		));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(answer));

		// Check the vote count.
		let stored_vec = Votes::<Test>::get(0, call_hash).unwrap_or(vec![]);
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
		let call_hash = Witnesser::call_hash(call.as_ref());

		// Register the call.
		assert_ok!(Witnesser::register(Origin::none(), call));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call_hash));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again with the same account, should error.
		assert_noop!(
			Witnesser::witness(Origin::signed(ALISSA), call_hash),
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
		let call_hash = Witnesser::call_hash(call.as_ref());

		// Register the call.
		assert_ok!(Witnesser::register(Origin::none(), call));

		// Validators can witness
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call_hash));
		assert_ok!(Witnesser::witness(Origin::signed(BOBSON), call_hash));
		assert_ok!(Witnesser::witness(
			Origin::signed(CHARLEMAGNE),
			call_hash
		));

		// Other accounts can't witness
		assert_noop!(
			Witnesser::witness(Origin::signed(DEIRDRE), call_hash),
			Error::<Test>::UnauthorizedWitness
		);
	});
}
