use crate::{
	mock::{dummy::pallet as pallet_dummy, *},
	CallHash, Error, VoteMask, Votes,
};
use cf_traits::{mocks::epoch_info::MockEpochInfo, EpochInfo};
use frame_support::{assert_noop, assert_ok};

fn assert_event_sequence<T: frame_system::Config>(expected: Vec<T::Event>) {
	let events = frame_system::Pallet::<T>::events()
		.into_iter()
		.rev()
		.take(expected.len())
		.rev()
		.map(|e| e.event)
		.collect::<Vec<_>>();

	assert_eq!(events, expected)
}

fn get_last_event() -> Event {
	frame_system::Pallet::<Test>::events().pop().expect("Expected an event").event
}

#[test]
fn call_on_threshold() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value()));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call.clone()));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again, we should reach the threshold and dispatch the call.
		assert_ok!(Witnesser::witness(Origin::signed(BOBSON), call.clone()));
		let dispatch_result =
			if let Event::Witnesser(crate::Event::WitnessExecuted(_, dispatch_result)) =
				get_last_event()
			{
				assert_ok!(dispatch_result);
				dispatch_result
			} else {
				panic!("Expected WitnessExecuted event!")
			};

		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		// Vote again, should count the vote but the call should not be dispatched again.
		assert_ok!(Witnesser::witness(Origin::signed(CHARLEMAGNE), call.clone()));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		// Check the deposited event to get the vote count.
		let call_hash = CallHash(frame_support::Hashable::blake2_256(&*call));
		let stored_vec =
			Votes::<Test>::get(MockEpochInfo::epoch_index(), call_hash).unwrap_or_default();
		let votes = VoteMask::from_slice(stored_vec.as_slice()).unwrap();
		assert_eq!(votes.count_ones(), 3);

		assert_event_sequence::<Test>(vec![
			crate::Event::WitnessReceived(call_hash, ALISSA, 1).into(),
			crate::Event::WitnessReceived(call_hash, BOBSON, 2).into(),
			crate::Event::ThresholdReached(call_hash, 2).into(),
			dummy::Event::<Test>::ValueIncremented(0u32).into(),
			crate::Event::WitnessExecuted(call_hash, dispatch_result).into(),
			crate::Event::WitnessReceived(call_hash, CHARLEMAGNE, 3).into(),
		]);
	});
}

#[test]
fn no_double_call_on_epoch_boundary() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value()));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness_at_epoch(Origin::signed(ALISSA), call.clone(), 1));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);
		MockEpochInfo::next_epoch([ALISSA, BOBSON, CHARLEMAGNE].to_vec());
		// Vote for the same call, this time in another epoch.
		assert_ok!(Witnesser::witness_at_epoch(Origin::signed(ALISSA), call.clone(), 2));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again, we should reach the threshold and dispatch the call.
		assert_ok!(Witnesser::witness_at_epoch(Origin::signed(BOBSON), call.clone(), 1));
		let dispatch_result =
			if let Event::Witnesser(crate::Event::WitnessExecuted(_, dispatch_result)) =
				get_last_event()
			{
				assert_ok!(dispatch_result);
				dispatch_result
			} else {
				panic!("Expected WitnessExecuted event!")
			};
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		// Vote for the same call, this time in another epoch. Threshold for the same call should be
		// reached but call shouldn't be dispatched again.
		assert_ok!(Witnesser::witness_at_epoch(Origin::signed(BOBSON), call.clone(), 2));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		let call_hash = CallHash(frame_support::Hashable::blake2_256(&*call));
		assert_event_sequence::<Test>(vec![
			crate::Event::WitnessReceived(call_hash, ALISSA, 1).into(),
			crate::Event::WitnessReceived(call_hash, ALISSA, 1).into(),
			crate::Event::WitnessReceived(call_hash, BOBSON, 2).into(),
			crate::Event::ThresholdReached(call_hash, 2).into(),
			dummy::Event::<Test>::ValueIncremented(0u32).into(),
			crate::Event::WitnessExecuted(call_hash, dispatch_result).into(),
			crate::Event::WitnessReceived(call_hash, BOBSON, 2).into(),
		]);
	});
}

#[test]
fn cannot_double_witness() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value()));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call.clone()));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again with the same account, should error.
		assert_noop!(
			Witnesser::witness(Origin::signed(ALISSA), call),
			Error::<Test>::DuplicateWitness
		);
	});
}

#[test]
fn only_validators_can_witness() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value()));

		// Validators can witness
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call.clone()));
		assert_ok!(Witnesser::witness(Origin::signed(BOBSON), call.clone()));
		assert_ok!(Witnesser::witness(Origin::signed(CHARLEMAGNE), call.clone()));

		// Other accounts can't witness
		assert_noop!(
			Witnesser::witness(Origin::signed(DEIRDRE), call),
			Error::<Test>::UnauthorisedWitness
		);
	});
}

#[test]
fn can_continue_to_witness_for_old_epochs() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value()));

		// These are ALISSA, BOBSON, CHARLEMAGNE
		let mut current_validators = MockEpochInfo::current_validators();
		// same validators for each epoch - we should change this though
		MockEpochInfo::next_epoch(current_validators.clone());
		MockEpochInfo::next_epoch(current_validators.clone());

		// remove CHARLEMAGNE and add DEIRDRE
		current_validators.pop();
		current_validators.push(DEIRDRE);
		assert_eq!(current_validators, vec![ALISSA, BOBSON, DEIRDRE]);
		MockEpochInfo::next_epoch(current_validators);

		let current_epoch = MockEpochInfo::epoch_index();
		assert_eq!(current_epoch, 4);

		let expired_epoch = current_epoch - 3;
		MockEpochInfo::set_last_expired_epoch(expired_epoch);

		// Witness a call for one before the current epoch which has yet to expire
		assert_ok!(Witnesser::witness_at_epoch(
			Origin::signed(ALISSA),
			call.clone(),
			current_epoch - 1,
		));

		// Try to witness in an epoch that has expired
		assert_noop!(
			Witnesser::witness_at_epoch(Origin::signed(ALISSA), call.clone(), expired_epoch,),
			Error::<Test>::EpochExpired
		);

		// Try to witness in a past epoch, which has yet to expire, and that we weren't a member
		assert_noop!(
			Witnesser::witness_at_epoch(Origin::signed(DEIRDRE), call.clone(), current_epoch - 1,),
			Error::<Test>::UnauthorisedWitness
		);

		// But can witness in an epoch we are in
		assert_ok!(Witnesser::witness_at_epoch(
			Origin::signed(DEIRDRE),
			call.clone(),
			current_epoch,
		));

		// And cannot witness in an epoch that doesn't yet exist
		assert_noop!(
			Witnesser::witness_at_epoch(Origin::signed(ALISSA), call, current_epoch + 1,),
			Error::<Test>::InvalidEpoch
		);
	});
}
