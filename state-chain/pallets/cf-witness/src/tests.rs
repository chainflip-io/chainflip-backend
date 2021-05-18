use crate::{mock::dummy::pallet as pallet_dummy, mock::*, Votes, Error, VoteMask, Config, Pallet};
use frame_support::{assert_noop, assert_ok, dispatch::DispatchResultWithPostInfo};

fn assert_event_sequence<T: frame_system::Config>(expected: Vec<T::Event>) 
{
	let events = frame_system::Pallet::<T>::events()
		.into_iter()
		.rev()
		.take(expected.len())
		.rev()
		.map(|e| e.event)
		.collect::<Vec<_>>();
	
	assert_eq!(events, expected)
}

fn pop_last_event() -> Event {
	frame_system::Pallet::<Test>::events().pop().expect("Expected an event").event
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

#[test]
fn witness_via_witnesser_trait() {
	new_test_ext().execute_with(|| {
		let answer = 42;
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value(
			answer,
		)));
		let call_hash = Witnesser::call_hash(call.as_ref());

		assert_ok!(<Pallet<Test> as cf_traits::Witnesser>::witness(ALISSA.into(), *call.clone()));
		assert_ok!(<Pallet<Test> as cf_traits::Witnesser>::witness(BOBSON.into(), *call.clone()));

		let dispatch_result = if let Event::pallet_cf_witness(crate::Event::WitnessExecuted(_, dispatch_result)) = pop_last_event() {
			assert_ok!(dispatch_result);
			dispatch_result
		} else {
			panic!("Expected WitnessExecuted event!")
		};

		assert_event_sequence::<Test>(vec![
			crate::Event::CallRegistered(call_hash).into(),
			crate::Event::WitnessReceived(call_hash, ALISSA, 1).into(),
			crate::Event::WitnessReceived(call_hash, BOBSON, 2).into(),
			crate::Event::ThresholdReached(call_hash, 2).into(),
			dummy::Event::<Test>::ValueIncremented(answer).into(),
			crate::Event::WitnessExecuted(call_hash, dispatch_result).into(),
		]);
	});
}
