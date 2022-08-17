use crate::{
	mock::{dummy::pallet as pallet_dummy, *},
	CallHash, CallHashExecuted, Error, ExtraCallData, VoteMask, Votes,
};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{mocks::epoch_info::MockEpochInfo, EpochInfo, EpochTransitionHandler};
use frame_support::{assert_noop, assert_ok};

#[test]
fn call_on_threshold() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value {}));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call.clone()));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again, we should reach the threshold and dispatch the call.
		assert_ok!(Witnesser::witness(Origin::signed(BOBSON), call.clone()));

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

		assert_event_sequence!(Test, Event::Dummy(dummy::Event::<Test>::ValueIncrementedTo(0u32)));
	});
}

#[test]
fn no_double_call_on_epoch_boundary() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value {}));

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness_at_epoch(Origin::signed(ALISSA), call.clone(), 1));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);
		MockEpochInfo::next_epoch([ALISSA, BOBSON, CHARLEMAGNE].to_vec());
		// Vote for the same call, this time in another epoch.
		assert_ok!(Witnesser::witness_at_epoch(Origin::signed(ALISSA), call.clone(), 2));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again, we should reach the threshold and dispatch the call.
		assert_ok!(Witnesser::witness_at_epoch(Origin::signed(BOBSON), call.clone(), 1));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		// Vote for the same call, this time in another epoch. Threshold for the same call should be
		// reached but call shouldn't be dispatched again.
		assert_ok!(Witnesser::witness_at_epoch(Origin::signed(BOBSON), call, 2));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		assert_event_sequence!(Test, Event::Dummy(dummy::Event::<Test>::ValueIncrementedTo(0u32)));
	});
}

#[test]
fn cannot_double_witness() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value {}));

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
fn only_authorities_can_witness() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value {}));

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
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::increment_value {}));

		// These are ALISSA, BOBSON, CHARLEMAGNE
		let mut current_authorities = MockEpochInfo::current_authorities();
		// same authorities for each epoch - we should change this though
		MockEpochInfo::next_epoch(current_authorities.clone());
		MockEpochInfo::next_epoch(current_authorities.clone());

		// remove CHARLEMAGNE and add DEIRDRE
		current_authorities.pop();
		current_authorities.push(DEIRDRE);
		assert_eq!(current_authorities, vec![ALISSA, BOBSON, DEIRDRE]);
		MockEpochInfo::next_epoch(current_authorities);

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

#[test]
fn can_purge_stale_storage() {
	new_test_ext().execute_with(|| {
		let call = CallHash(frame_support::Hashable::blake2_256(&*Box::new(Call::Dummy(
			pallet_dummy::Call::<Test>::increment_value {},
		))));

		for e in [2u32, 9, 10, 11] {
			Votes::<Test>::insert(e, &call, vec![0, 0, e as u8]);
			ExtraCallData::<Test>::insert(e, &call, vec![vec![0], vec![e as u8]]);
			CallHashExecuted::<Test>::insert(e, &call, ());
		}

		// Purge storage in epoch 2.
		Witnesser::on_expired_epoch(2);

		// Storage items for epoch 2 should be removed.
		assert_eq!(Votes::<Test>::get(2u32, &call), None);
		assert_eq!(ExtraCallData::<Test>::get(2u32, call), None);
		assert_eq!(CallHashExecuted::<Test>::get(2u32, call), None);
		// Future epoch items are unaffected.
		for e in [9u32, 10, 11] {
			Votes::<Test>::insert(e, &call, vec![0, 0, e as u8]);
			ExtraCallData::<Test>::insert(e, &call, vec![vec![0], vec![e as u8]]);
			CallHashExecuted::<Test>::insert(e, &call, ());
		}

		// Remove storage items for epoch 9 and 10.
		Witnesser::on_expired_epoch(9);
		Witnesser::on_expired_epoch(10);

		for e in [9u32, 10] {
			assert_eq!(Votes::<Test>::get(e, &call), None);
			assert_eq!(ExtraCallData::<Test>::get(e, call), None);
			assert_eq!(CallHashExecuted::<Test>::get(e, call), None);
		}

		// Epoch 11's storage items are unaffected.
		assert_eq!(Votes::<Test>::get(11u32, &call), Some(vec![0, 0, 11]));
		assert_eq!(ExtraCallData::<Test>::get(11u32, call), Some(vec![vec![0], vec![11]]));
		assert_eq!(CallHashExecuted::<Test>::get(11u32, call), Some(()));
	});
}
