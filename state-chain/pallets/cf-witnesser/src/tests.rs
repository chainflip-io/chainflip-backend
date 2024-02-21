#![cfg(test)]

use crate::{
	mock::{dummy::pallet as pallet_dummy, *},
	weights::WeightInfo,
	CallHash, CallHashExecuted, Config, EpochsToCull, Error, ExtraCallData, PalletOffence,
	PalletSafeMode, VoteMask, Votes, WitnessDeadline, WitnessedCallsScheduledForDispatch,
};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::account_role_registry::MockAccountRoleRegistry, AccountRoleRegistry, EpochInfo,
	EpochTransitionHandler, SafeMode, SetSafeMode,
};
use frame_support::{assert_noop, assert_ok, traits::Hooks, weights::Weight};
use sp_std::collections::btree_set::BTreeSet;

#[test]
fn call_on_threshold() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));
		let current_epoch = MockEpochInfo::epoch_index();

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(ALISSA),
			call.clone(),
			current_epoch
		));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again, we should reach the threshold and dispatch the call.
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(BOBSON),
			call.clone(),
			current_epoch
		));

		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		// Vote again, should count the vote but the call should not be dispatched again.
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(CHARLEMAGNE),
			call.clone(),
			current_epoch
		));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		// Check the deposited event to get the vote count.
		let call_hash = CallHash(frame_support::Hashable::blake2_256(&*call));
		let stored_vec =
			Votes::<Test>::get(MockEpochInfo::epoch_index(), call_hash).unwrap_or_default();
		let votes = VoteMask::from_slice(stored_vec.as_slice());
		assert_eq!(votes.count_ones(), 3);

		assert_event_sequence!(
			Test,
			RuntimeEvent::Dummy(dummy::Event::<Test>::ValueIncrementedTo(0u32))
		);
	});
}

/// This test is very important! It supports the assumption that the CFE witnessing may occur twice.
/// and that if it does, we handle that correctly, by not executing the call twice.
#[test]
fn no_double_call_on_epoch_boundary() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));

		assert_eq!(MockEpochInfo::epoch_index(), 1);

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness_at_epoch(RuntimeOrigin::signed(ALISSA), call.clone(), 1));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);
		MockEpochInfo::next_epoch(BTreeSet::from([ALISSA, BOBSON, CHARLEMAGNE]));
		assert_eq!(MockEpochInfo::epoch_index(), 2);

		// Vote for the same call, this time in the next epoch.
		assert_ok!(Witnesser::witness_at_epoch(RuntimeOrigin::signed(ALISSA), call.clone(), 2));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again with different signer on epoch 1, we should reach the threshold and dispatch
		// the call from epoch 1.
		assert_ok!(Witnesser::witness_at_epoch(RuntimeOrigin::signed(BOBSON), call.clone(), 1));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		// Vote for the same call, this time in another epoch. Threshold for the same call should be
		// reached but call shouldn't be dispatched again.
		assert_ok!(Witnesser::witness_at_epoch(RuntimeOrigin::signed(BOBSON), call, 2));
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));

		assert_event_sequence!(
			Test,
			RuntimeEvent::Dummy(dummy::Event::<Test>::ValueIncrementedTo(0u32))
		);
	});
}

#[test]
fn cannot_double_witness() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));
		let current_epoch = MockEpochInfo::epoch_index();

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(ALISSA),
			call.clone(),
			current_epoch
		));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again with the same account, should error.
		assert_noop!(
			Witnesser::witness_at_epoch(RuntimeOrigin::signed(ALISSA), call, current_epoch),
			Error::<Test>::DuplicateWitness
		);
	});
}

#[test]
fn only_authorities_can_witness() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));
		let current_epoch = MockEpochInfo::epoch_index();

		// Validators can witness
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(ALISSA),
			call.clone(),
			current_epoch
		));
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(BOBSON),
			call.clone(),
			current_epoch
		));
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(CHARLEMAGNE),
			call.clone(),
			current_epoch
		));

		// Other accounts can't witness
		assert_noop!(
			Witnesser::witness_at_epoch(RuntimeOrigin::signed(DEIRDRE), call, current_epoch),
			Error::<Test>::UnauthorisedWitness
		);
	});
}

#[test]
fn can_continue_to_witness_for_old_epochs() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));

		// These are ALISSA, BOBSON, CHARLEMAGNE
		let mut current_authorities = MockEpochInfo::current_authorities();
		// same authorities for each epoch - we should change this though
		MockEpochInfo::next_epoch(current_authorities.clone());
		MockEpochInfo::next_epoch(current_authorities.clone());

		// remove CHARLEMAGNE and add DEIRDRE
		current_authorities.pop_last();
		current_authorities.insert(DEIRDRE);
		assert_eq!(current_authorities, BTreeSet::from([ALISSA, BOBSON, DEIRDRE]));
		MockEpochInfo::next_epoch(current_authorities);

		let current_epoch = MockEpochInfo::epoch_index();
		assert_eq!(current_epoch, 4);

		let expired_epoch = current_epoch - 3;
		MockEpochInfo::set_last_expired_epoch(expired_epoch);

		// Witness a call for one before the current epoch which has yet to expire
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(ALISSA),
			call.clone(),
			current_epoch - 1,
		));

		// Try to witness in an epoch that has expired
		assert_noop!(
			Witnesser::witness_at_epoch(RuntimeOrigin::signed(ALISSA), call.clone(), expired_epoch,),
			Error::<Test>::EpochExpired
		);

		// Try to witness in a past epoch, which has yet to expire, and that we weren't a member
		assert_noop!(
			Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(DEIRDRE),
				call.clone(),
				current_epoch - 1,
			),
			Error::<Test>::UnauthorisedWitness
		);

		// But can witness in an epoch we are in
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(DEIRDRE),
			call.clone(),
			current_epoch,
		));

		// And cannot witness in an epoch that doesn't yet exist
		assert_noop!(
			Witnesser::witness_at_epoch(RuntimeOrigin::signed(ALISSA), call, current_epoch + 1,),
			Error::<Test>::InvalidEpoch
		);
	});
}

#[test]
fn can_purge_stale_storage() {
	const BLOCK_WEIGHT: u64 = 1_000_000_000_000u64;
	let delete_weight = <Test as Config>::WeightInfo::remove_storage_items(1);
	new_test_ext()
		.execute_with(|| {
			let call1 = CallHash(frame_support::Hashable::blake2_256(&*Box::new(
				RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}),
			)));
			let call2 = CallHash(frame_support::Hashable::blake2_256(&*Box::new(
				RuntimeCall::System(frame_system::Call::<Test>::remark { remark: vec![0] }),
			)));

			for e in [2u32, 9, 10, 11] {
				Votes::<Test>::insert(e, call1, vec![0, 0, e as u8]);
				Votes::<Test>::insert(e, call2, vec![0, 0, e as u8]);
				ExtraCallData::<Test>::insert(e, call1, vec![vec![0], vec![e as u8]]);
				ExtraCallData::<Test>::insert(e, call2, vec![vec![0], vec![e as u8]]);
				CallHashExecuted::<Test>::insert(e, call1, ());
				CallHashExecuted::<Test>::insert(e, call2, ());
			}
		})
		// Commit Overlay changeset into the backend DB, to fully test clear_prefix logic.
		// See: /state-chain/TROUBLESHOOTING.md
		// Section: ## Substrate storage: Separation of front overlay and backend. Feat
		// clear_prefix()
		.commit_all()
		.execute_with(|| {
			let call1 = CallHash(frame_support::Hashable::blake2_256(&*Box::new(
				RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}),
			)));
			let call2 = CallHash(frame_support::Hashable::blake2_256(&*Box::new(
				RuntimeCall::System(frame_system::Call::<Test>::remark { remark: vec![0] }),
			)));

			Witnesser::on_expired_epoch(2);
			Witnesser::on_expired_epoch(3);
			Witnesser::on_expired_epoch(4);
			assert_eq!(EpochsToCull::<Test>::get(), vec![2, 3, 4]);

			// Nothing to clean up in epoch 4
			Witnesser::on_idle(1, Weight::from_parts(BLOCK_WEIGHT, 0));
			assert_eq!(EpochsToCull::<Test>::get(), vec![2, 3]);
			for e in [2u32, 9, 10, 11] {
				assert_eq!(Votes::<Test>::get(e, call1), Some(vec![0, 0, e as u8]));
				assert_eq!(Votes::<Test>::get(e, call2), Some(vec![0, 0, e as u8]));
				assert_eq!(
					ExtraCallData::<Test>::get(e, call1),
					Some(vec![vec![0], vec![e as u8]])
				);
				assert_eq!(
					ExtraCallData::<Test>::get(e, call2),
					Some(vec![vec![0], vec![e as u8]])
				);
				assert_eq!(CallHashExecuted::<Test>::get(e, call1), Some(()));
				assert_eq!(CallHashExecuted::<Test>::get(e, call2), Some(()));
			}

			Witnesser::on_idle(2, Weight::from_parts(BLOCK_WEIGHT, 0));

			// Partially clean data from epoch 2
			Witnesser::on_idle(3, delete_weight * 4);

			assert_eq!(Votes::<Test>::get(2u32, call1), None);
			assert_eq!(Votes::<Test>::get(2u32, call2), None);
			assert_eq!(ExtraCallData::<Test>::get(2u32, call1), None);
			assert_eq!(ExtraCallData::<Test>::get(2u32, call2), None);
			assert_eq!(CallHashExecuted::<Test>::get(2u32, call1), Some(()));
			assert_eq!(CallHashExecuted::<Test>::get(2u32, call2), Some(()));

			assert_eq!(EpochsToCull::<Test>::get(), vec![2]);
		})
		.commit_all()
		.execute_with(|| {
			let call1 = CallHash(frame_support::Hashable::blake2_256(&*Box::new(
				RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}),
			)));
			let call2 = CallHash(frame_support::Hashable::blake2_256(&*Box::new(
				RuntimeCall::System(frame_system::Call::<Test>::remark { remark: vec![0] }),
			)));

			// Clean the remaining storage
			Witnesser::on_idle(4, Weight::from_parts(BLOCK_WEIGHT, 0));

			// Epoch 2's stale data should be fully cleaned.
			assert_eq!(CallHashExecuted::<Test>::get(2u32, call1), None);
			assert_eq!(CallHashExecuted::<Test>::get(2u32, call2), None);
			assert!(EpochsToCull::<Test>::get().is_empty());

			// Future epoch items are unaffected.
			for e in [9u32, 10, 11] {
				assert_eq!(Votes::<Test>::get(e, call1), Some(vec![0, 0, e as u8]));
				assert_eq!(Votes::<Test>::get(e, call2), Some(vec![0, 0, e as u8]));
				assert_eq!(
					ExtraCallData::<Test>::get(e, call1),
					Some(vec![vec![0], vec![e as u8]])
				);
				assert_eq!(
					ExtraCallData::<Test>::get(e, call2),
					Some(vec![vec![0], vec![e as u8]])
				);
				assert_eq!(CallHashExecuted::<Test>::get(e, call1), Some(()));
				assert_eq!(CallHashExecuted::<Test>::get(e, call2), Some(()));
			}

			// Remove storage items for epoch 9 and 10.
			Witnesser::on_expired_epoch(9);
			Witnesser::on_expired_epoch(10);
			assert_eq!(EpochsToCull::<Test>::get(), vec![9, 10]);
			Witnesser::on_idle(4, Weight::from_parts(BLOCK_WEIGHT, 0));
			Witnesser::on_idle(5, Weight::from_parts(BLOCK_WEIGHT, 0));
			assert!(EpochsToCull::<Test>::get().is_empty());

			for e in [9u32, 10] {
				assert_eq!(Votes::<Test>::get(e, call1), None);
				assert_eq!(Votes::<Test>::get(e, call2), None);
				assert_eq!(ExtraCallData::<Test>::get(e, call1), None);
				assert_eq!(ExtraCallData::<Test>::get(e, call2), None);
				assert_eq!(CallHashExecuted::<Test>::get(e, call1), None);
				assert_eq!(CallHashExecuted::<Test>::get(e, call2), None);
			}

			// Epoch 11's storage items are unaffected.
			assert_eq!(Votes::<Test>::get(11u32, call1), Some(vec![0, 0, 11]));
			assert_eq!(Votes::<Test>::get(11u32, call2), Some(vec![0, 0, 11]));
			assert_eq!(ExtraCallData::<Test>::get(11u32, call1), Some(vec![vec![0], vec![11]]));
			assert_eq!(ExtraCallData::<Test>::get(11u32, call2), Some(vec![vec![0], vec![11]]));
			assert_eq!(CallHashExecuted::<Test>::get(11u32, call1), Some(()));
			assert_eq!(CallHashExecuted::<Test>::get(11u32, call2), Some(()));
		});
}

#[test]
fn test_safe_mode() {
	new_test_ext().execute_with(|| {
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			witnesser: PalletSafeMode::CODE_RED,
		});

		let call = Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));
		let current_epoch = MockEpochInfo::epoch_index();

		// Only one vote, nothing should happen yet.
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(ALISSA),
			call.clone(),
			current_epoch
		));
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// Vote again, we should reach the threshold but not dispatch the call.
		assert_ok!(Witnesser::witness_at_epoch(RuntimeOrigin::signed(BOBSON), call, current_epoch));

		// the call should not be dispatched
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		//the call should be stored for dispatching later when safe mode is deactivated.
		assert!(!WitnessedCallsScheduledForDispatch::<Test>::get().is_empty());

		Witnesser::on_idle(1, Weight::zero().set_ref_time(1_000_000_000_000u64));

		// the call is still not dispatched and we do nothing in the on_initialize since we are
		// still in safe mode
		assert!(!WitnessedCallsScheduledForDispatch::<Test>::get().is_empty());

		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			witnesser: PalletSafeMode::CODE_GREEN,
		});

		// the call should now be able to dispatch since we now deactivated the safe mode but wont
		// because there is not enough idle weight available.
		Witnesser::on_idle(2, Weight::zero().set_ref_time(0u64));

		assert!(!WitnessedCallsScheduledForDispatch::<Test>::get().is_empty());

		// The call should now dispatch since we have enough weight now.
		Witnesser::on_idle(3, Weight::zero().set_ref_time(1_000_000_000_000u64));

		assert!(WitnessedCallsScheduledForDispatch::<Test>::get().is_empty());

		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));
	});
}

#[test]
fn safe_mode_code_amber_can_filter_calls() {
	new_test_ext().execute_with(|| {
		// Block calls via SafeMode::CodeAmber
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			witnesser: PalletSafeMode::CodeAmber(MockCallFilter {}),
		});
		AllowCall::set(false);

		// Sign the call so its ready to be dispatched
		let call = Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));
		let current_epoch = MockEpochInfo::epoch_index();
		for s in [ALISSA, BOBSON] {
			assert_ok!(Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(s),
				call.clone(),
				current_epoch
			));
		}
		assert_eq!(WitnessedCallsScheduledForDispatch::<Test>::decode_len(), Some(1));

		// Call is not dispatched because its blocked by the CallDispatchFilter
		Witnesser::on_idle(1, Weight::zero().set_ref_time(1_000_000_000_000u64));
		assert!(!WitnessedCallsScheduledForDispatch::<Test>::get().is_empty());

		// Allow the call to pass the filter
		AllowCall::set(true);

		// Call should be dispatched now.
		Witnesser::on_idle(2, Weight::zero().set_ref_time(1_000_000_000_000u64));
		assert!(WitnessedCallsScheduledForDispatch::<Test>::get().is_empty());
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));
	});
}

#[test]
fn safe_mode_recovery_ignores_duplicates() {
	new_test_ext().execute_with(|| {
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			witnesser: PalletSafeMode::CODE_RED,
		});

		let call = Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));

		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(ALISSA),
			call.clone(),
			MockEpochInfo::epoch_index()
		));
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(BOBSON),
			call.clone(),
			MockEpochInfo::epoch_index()
		));
		MockEpochInfo::next_epoch(MockEpochInfo::current_authorities());
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(ALISSA),
			call.clone(),
			MockEpochInfo::epoch_index()
		));
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(BOBSON),
			call.clone(),
			MockEpochInfo::epoch_index()
		));

		// The call should not be dispatched
		assert_eq!(pallet_dummy::Something::<Test>::get(), None);

		// The same call is stored twice.
		assert_eq!(WitnessedCallsScheduledForDispatch::<Test>::get().len(), 2);

		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			witnesser: PalletSafeMode::CODE_GREEN,
		});
		Witnesser::on_idle(1, Weight::zero().set_ref_time(1_000_000_000_000u64));

		assert!(WitnessedCallsScheduledForDispatch::<Test>::get().is_empty());

		// The call was only applied once.
		assert_eq!(pallet_dummy::Something::<Test>::get(), Some(0u32));
	});
}

fn setup_witness_authorities(
	authority_ids: impl Iterator<Item = u64>,
) -> (Box<RuntimeCall>, CallHash) {
	// Setup authorities and variables.
	let authorities = authority_ids
		.map(|v| {
			let _ =
				<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&v);
			v
		})
		.collect::<BTreeSet<_>>();
	MockEpochInfo::next_epoch(authorities.clone());
	let mut call: Box<RuntimeCall> =
		Box::new(RuntimeCall::Dummy(pallet_dummy::Call::<Test>::increment_value {}));
	let (_, call_hash) = Witnesser::split_calldata(&mut call);
	(call, call_hash)
}

#[test]
fn count_votes_works() {
	new_test_ext().execute_with(|| {
		// Setup authorities and variables.
		let (call, call_hash) = setup_witness_authorities(0u64..100u64);
		let epoch = MockEpochInfo::epoch_index();

		// Prepare expected votes vec.
		let mut votes = (0u64..100u64).map(|v| (v, false)).collect::<Vec<_>>();

		// Verify the count_votes function can correctly split the votes.
		for v in 0u64..100u64 {
			// Update expected values
			votes[v as usize].1 = true;

			// Insert a new witness
			assert_ok!(Witnesser::witness_at_epoch(RuntimeOrigin::signed(v), call.clone(), epoch));

			assert_eq!(Witnesser::count_votes(epoch, call_hash), Some(votes.clone()));
		}
	});
}

#[test]
fn can_punish_failed_witnesser() {
	let mut target = 0u64;
	let success_threshold = cf_utilities::success_threshold_from_share_count(100u32) as u64;
	new_test_ext()
		.execute_with(|| {
			// Setup authorities and variables.
			let (call, call_hash) = setup_witness_authorities(0u64..100u64);
			let epoch = MockEpochInfo::epoch_index();

			// Upon hook execution, a deadline is set for witnessing.
			target = System::block_number() + GracePeriod::get();

			// Witness just enough to succeed
			for v in 0u64..success_threshold {
				// Insert a new witness
				assert_ok!(Witnesser::witness_at_epoch(
					RuntimeOrigin::signed(v),
					call.clone(),
					epoch
				));
			}

			assert!(CallHashExecuted::<Test>::contains_key(epoch, call_hash));
			assert_eq!(WitnessDeadline::<Test>::get(target), vec![(epoch, call_hash)]);

			// Before the deadline is set, no one has been reported.
			OffenceReporter::assert_reported(PalletOffence::FailedToWitnessInTime, vec![]);
			call_hash
		})
		.then_execute_at_block(target, |_| {})
		.then_execute_with(|_| {
			// After deadline has passed, all nodes that are late are reported.
			OffenceReporter::assert_reported(
				PalletOffence::FailedToWitnessInTime,
				success_threshold..100u64,
			);

			// storage is cleaned up.
			assert_eq!(WitnessDeadline::<Test>::decode_len(target), None);
		});
}

#[test]
fn can_punish_failed_witnesser_after_forced_witness() {
	let mut target = 0u64;
	let witnessed_nodes = 10u64;
	new_test_ext()
		.execute_with(|| {
			// Setup authorities and variables.
			let (call, call_hash) = setup_witness_authorities(0u64..100u64);
			let epoch = MockEpochInfo::epoch_index();

			// Upon hook execution, a deadline is set for witnessing.
			target = System::block_number() + GracePeriod::get();

			// Have a few nodes witness
			for v in 0u64..witnessed_nodes {
				// Insert a new witness
				assert_ok!(Witnesser::witness_at_epoch(
					RuntimeOrigin::signed(v),
					call.clone(),
					epoch
				));
			}

			// Force witness to pass the votes
			assert_ok!(Witnesser::force_witness(RuntimeOrigin::root(), call, epoch,));

			assert!(CallHashExecuted::<Test>::contains_key(epoch, call_hash));
			assert_eq!(WitnessDeadline::<Test>::get(target), vec![(epoch, call_hash)]);

			// Before the deadline is set, no one has been reported.
			OffenceReporter::assert_reported(PalletOffence::FailedToWitnessInTime, vec![]);
			call_hash
		})
		.then_execute_at_block(target, |_| {})
		.then_execute_with(|_| {
			// After deadline has passed, all nodes that are late are reported.
			OffenceReporter::assert_reported(
				PalletOffence::FailedToWitnessInTime,
				witnessed_nodes..100u64,
			);

			// storage is cleaned up.
			assert_eq!(WitnessDeadline::<Test>::decode_len(target), None);
		});
}

#[test]
fn can_punish_failed_witnesser_in_previous_epochs() {
	let mut target = 0u64;
	let success_threshold = cf_utilities::success_threshold_from_share_count(100u32) as u64;
	new_test_ext()
		.execute_with(|| {
			// Setup authorities and variables.
			let (call, call_hash) = setup_witness_authorities(0u64..100u64);
			let epoch = MockEpochInfo::epoch_index();

			// Upon hook execution, a deadline is set for witnessing.
			target = System::block_number() + GracePeriod::get();

			// Have some nodes to witness
			for v in 0u64..(success_threshold / 2) {
				assert_ok!(Witnesser::witness_at_epoch(
					RuntimeOrigin::signed(v),
					call.clone(),
					epoch
				));
			}

			// Rotate to the next epoch with new authorities
			let _ = setup_witness_authorities(100u64..200u64);
			// Set the current set of authority as the past authorities in the Mock.
			MockEpochInfo::set_past_authorities(BTreeSet::from_iter(0u64..100u64));

			// Some of remaining authorities can witness to pass the
			for v in (success_threshold / 2)..success_threshold {
				assert_ok!(Witnesser::witness_at_epoch(
					RuntimeOrigin::signed(v),
					call.clone(),
					epoch
				));
			}

			assert!(CallHashExecuted::<Test>::contains_key(epoch, call_hash));
			assert_eq!(WitnessDeadline::<Test>::get(target), vec![(epoch, call_hash)]);

			// Before the deadline is set, no one has been reported.
			OffenceReporter::assert_reported(PalletOffence::FailedToWitnessInTime, vec![]);
			call_hash
		})
		.then_execute_at_block(target, |_| {})
		.then_execute_with(|_| {
			// Nodes from previous epoch is reported.
			OffenceReporter::assert_reported(
				PalletOffence::FailedToWitnessInTime,
				success_threshold..100u64,
			);

			// storage is cleaned up.
			assert_eq!(WitnessDeadline::<Test>::decode_len(target), None);
		});
}
