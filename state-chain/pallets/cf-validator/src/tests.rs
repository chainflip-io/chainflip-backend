mod test {
	use crate::*;
	use crate::{Error, mock::*};
	use sp_runtime::traits::{BadOrigin, Zero};
	use frame_support::{assert_ok, assert_noop};

	// Constants
	const ALICE: u64 = 100;
	const INVALID_EPOCH: EpochIndex = EpochIndex(0);
	const FIRST_EPOCH: EpochIndex = EpochIndex(1);
	const SECOND_EPOCH: EpochIndex = EpochIndex(2);
	const THIRD_EPOCH: EpochIndex = EpochIndex(3);

	fn events() -> Vec<mock::Event> {
		let evt = System::events().into_iter().map(|evt| evt.event).collect::<Vec<_>>();
		System::reset_events();
		evt
	}

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
	}

	fn confirm_and_complete_auction(block_number: &mut u64, idx: EpochIndex) {
		assert_ok!(ValidatorManager::confirm_auction(Origin::signed(ALICE), idx));
		*block_number += 1;
		run_to_block(*block_number);

		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::AuctionConfirmed(idx)),
				mock::Event::pallet_cf_validator(crate::Event::NewEpoch(idx)),
				// An epoch is 2 sessions so easy math
				mock::Event::pallet_session(pallet_session::Event::NewSession(idx.0 * 2)),
			]
		);

		// Confirm we have set the epoch index after moving on
		assert_eq!(ValidatorManager::epoch_index(), idx);
		// We should fail in confirming the second time
		assert_noop!(ValidatorManager::confirm_auction(Origin::signed(ALICE), idx), Error::<Test>::InvalidAuction);
	}

	fn get_auction_epoch_idx(event: mock::Event) -> EpochIndex {
		if let mock::Event::pallet_cf_validator(event) = event {
			if let crate::Event::AuctionStarted(idx) = event.into() {
				return idx;
			}
		}
		panic!("Expected AuctionStarted event");
	}

	#[test]
	fn estimation_on_next_session() {
		new_test_ext().execute_with(|| {
			assert_eq!(ValidatorManager::estimate_next_session_rotation(3), None);
		});
	}

	#[test]
	fn changing_validator_size() {
		new_test_ext().execute_with(|| {
			// Assert our minimum is set to 2
			assert_eq!(<Test as Config>::MinValidatorSetSize::get(), 2);
			// Check we are throwing up an error when we send anything less than the minimum of 2
			assert_noop!(ValidatorManager::set_validator_target_size(Origin::root(), 0), Error::<Test>::InvalidValidatorSetSize);
			assert_noop!(ValidatorManager::set_validator_target_size(Origin::root(), 1), Error::<Test>::InvalidValidatorSetSize);
			// This should now work
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 2));
			// Confirm we have an event with the change of 0 to 2
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 2)),
			);

			// We throw up an error if we try to set it to the current
			assert_noop!(ValidatorManager::set_validator_target_size(Origin::root(), 2), Error::<Test>::InvalidValidatorSetSize);
		});
	}

	#[test]
	fn validate_candidates() {
		new_test_ext().execute_with(|| {
			let min_size = 2;
			// Assert our minimum is set to 2
			assert_eq!(<Test as Config>::MinValidatorSetSize::get(), min_size);
			// Remove those with zero amount
			let candidates = vec![(1, 2), (2, 0), (3, 100)];
			assert_eq!(ValidatorManager::validate_auction(candidates), Ok(vec![(1, 2), (3, 100)]));
			// Candidate with id of '100' is not in the session key set
			let candidates = vec![(1, 2), (2, 2), (BAD_VALIDATOR_ID, 100)];
			assert_eq!(ValidatorManager::validate_auction(candidates), Ok(vec![(1, 2), (2, 2)]));
			// After this we end up with less than 'min_size' we should end up empty handed
			let candidates = vec![(1, 2), (2, 0), (BAD_VALIDATOR_ID, 100)];
			assert_eq!(ValidatorManager::validate_auction(candidates), Err(AuctionError::MinValidatorSize));
		});
	}

	#[test]
	fn run_auction() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 100));
			// A group of validators, we should see the order from high to low and the bond being 10
			// as it's the lowest
			let candidates = vec![(1, 1000), (3, 10), (2, 100)];
			assert_eq!(ValidatorManager::run_auction(candidates), Ok((vec![1, 2, 3], 10)));
		});
	}

	#[test]
	fn confirm_auction() {
		new_test_ext().execute_with(|| {
			// Propose a set that will fail the confirmation, at the moment the rule is 0 bond which
			// isn't possible as this is a rule in `validate_auction`.  We would hope to have more
			// conditions here to confirm auction such as whether the validator is online or possibly
			// a blacklist/whitelist
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 100));
			let candidates = vec![(1, 1000), (3, 0), (2, 100)];
			let candidates = ValidatorManager::run_auction(candidates);
			assert_eq!(candidates, Ok((vec![1, 2, 3], 0)));
			assert_eq!(ValidatorManager::complete_auction(candidates.unwrap()).unwrap_err(), AuctionError::BondIsZero);
		});
	}

	#[test]
	fn changing_epoch() {
		new_test_ext().execute_with(|| {
			// Confirm we have a minimum epoch of 1 block
			assert_eq!(<Test as Config>::MinEpoch::get(), 1);
			// Throw up an error if we supply anything less than this
			assert_noop!(ValidatorManager::set_blocks_for_epoch(Origin::root(), 0), Error::<Test>::InvalidEpoch);
			// This should work as 2 > 1
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), 2));
			// Confirm we have an event for the change from 0 to 2
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(0, 2)),
			);
			// We throw up an error if we try to set it to the current
			assert_noop!(ValidatorManager::set_blocks_for_epoch(Origin::root(), 2), Error::<Test>::InvalidEpoch);
		});
	}

	#[test]
	fn sessions_do_end() {
		new_test_ext().execute_with(|| {
			// As our epoch is 0 at genesis we should return false always
			assert!(!ValidatorManager::should_end_session(1));
			assert!(!ValidatorManager::should_end_session(2));
			// Set epoch to 2 blocks
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), 2));
			// Confirm we have the event for the change from 0 to 2
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(0, 2)),
			);
			// We should now be able to end a session on block 2
			assert!(ValidatorManager::should_end_session(2));
			// This isn't the case for block 1
			assert!(!ValidatorManager::should_end_session(1));
		});
	}

	#[test]
	fn have_optional_validators_on_genesis() {
		new_test_ext().execute_with(|| {
			// Add two validators at genesis
			// Confirm we have them from block 1 in the validator set
		});
	}

	#[test]
	fn you_have_to_be_priviledged() {
		new_test_ext().execute_with(|| {
			// Run through the sudo extrinsics to be sure they are what they are
			assert_noop!(ValidatorManager::set_blocks_for_epoch(Origin::signed(ALICE), Zero::zero()), BadOrigin);
			assert_noop!(ValidatorManager::set_validator_target_size(Origin::signed(ALICE), Zero::zero()), BadOrigin);
			assert_noop!(ValidatorManager::force_auction(Origin::signed(ALICE)), BadOrigin);
		});
	}

	#[test]
	fn bring_forward_session() {
		new_test_ext().execute_with(|| {
			// We are after 4 validators, the mock is set up for 4
			let mut set_size = 10;
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), set_size));
			// Set block length of epoch to 2
			let mut epoch = 2;
			let mut block_number = epoch;
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch));
			assert_eq!(mock::current_validators().len(), 0);
			// Move an epoch forward
			run_to_block(block_number);

			let mut ev: Vec<mock::Event> = events();
			// Pop off session event
			ev.pop();
			let auction_idx = get_auction_epoch_idx(ev.pop().expect("event expected"));
			assert_eq!(auction_idx, FIRST_EPOCH);

			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(0, 2))));
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, set_size))));

			// We have no current validators nor outgoing in first rotation as there were none in genesis
			assert_eq!(mock::current_validators().len(), 0);
			assert_eq!(mock::outgoing_validators().len(), 0);

			// Move an epoch forward, as we are in an auction phase we shouldn't move forward and
			// hence no events
			block_number += epoch;
			run_to_block(block_number);
			assert_eq!(events(), []);

			// Validator set hasn't changed.
			assert_eq!(mock::current_validators(), mock::outgoing_validators());

			// Confirm auction, call extrinsic `confirm_auction`
			// Just to see if it fails we will try this first
			assert_noop!(ValidatorManager::confirm_auction(Origin::signed(ALICE), INVALID_EPOCH), Error::<Test>::InvalidAuction);
			confirm_and_complete_auction(&mut block_number, auction_idx);

			let mut current = mock::current_validators();
			let mut outgoing = mock::outgoing_validators();

			assert_eq!(current, ValidatorManager::current_validators());
			// Until we are in an auction phase we wouldn't have any candidates
			assert!(ValidatorManager::next_validators().is_empty());

			// We should have our validators except the first as theu have 0 staked,
			// as we had none before we would see none in 'outgoing'
			assert_eq!(current.len(), 3);
			assert_eq!(outgoing.len(), 0);
			// On each auction are candidates are increasing stake so we should see 'bond' increase
			let mut bond = 0;
			// Repeat a few epochs
			for epoch_idx in 2..10 {
				block_number += epoch;
				// Move another session forward
				run_to_block(block_number);
				let mut ev: Vec<mock::Event> = events();
				// Pop off session event
				ev.pop();
				let auction_idx = get_auction_epoch_idx(ev.pop().expect("event expected"));
				assert_eq!(auction_idx, EpochIndex(epoch_idx));

				// We should see current set of validators not changing even though we have a new session idx
				assert_eq!(current, mock::current_validators());
				// and this would be reflected in outgoing and current
				assert_eq!(mock::outgoing_validators(), mock::current_validators());

				// We are in the auction phase and would expect to see our candidates in `next_validators()`
				assert_eq!(current, ValidatorManager::current_validators());
				assert_ne!(current, ValidatorManager::next_validators());

				confirm_and_complete_auction(&mut block_number, auction_idx);
				// Reduce the size for the next auction
				set_size -= 1;
				assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), set_size));

				// Increase the size of the epoch
				epoch += 1;
				assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch));

				// Confirm the bond is increasing
				assert!(bond < ValidatorManager::bond());
				bond = ValidatorManager::bond();

				// Should be new set of validators
				assert_ne!(current, mock::current_validators());
				current = mock::current_validators();
				assert_ne!(outgoing, mock::outgoing_validators());
				outgoing = mock::outgoing_validators();
			}
		});
	}

	#[test]
	fn force_auction() {
		new_test_ext().execute_with(|| {
			// We are after 4 validators, the mock is set up for 4
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 3));
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 3)),
			);
			// Set the epoch at 10
			let epoch = 10;
			let block_number = 2;
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch));
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(0, 10)),
			);
			// Clear the event queue
			System::reset_events();
			// Run forward 2 blocks
			run_to_block(block_number);
			// No rotation, no candidates
			assert_eq!(mock::outgoing_validators().len(), 0);
			// Force rotation for next block
			assert_ok!(ValidatorManager::force_auction(Origin::root()));
			run_to_block(block_number + 1);

			let mut ev: Vec<mock::Event> = events();
			// Pop off session event
			ev.pop();
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(FIRST_EPOCH))));
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::ForceAuctionRequested())));
		});
	}

	#[test]
	fn force_auction_in_auction() {
		new_test_ext().execute_with(|| {
			// We are after 4 validators, the mock is set up for 4
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 3));
			// Check we get rotation
			let epoch = 2;
			let block_number = epoch;
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch));
			assert_eq!(
				events(),
				[
					mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 3)),
					mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(0, epoch)),
				]
			);

			run_to_block(block_number);
			let mut ev: Vec<mock::Event> = events();
			// Pop off session event
			ev.pop();
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(FIRST_EPOCH))));
			// Force rotation, failing as we are in an auction
			assert_noop!(ValidatorManager::force_auction(Origin::root()), Error::<Test>::FailedForceAuction);
		});
	}

	#[test]
	fn push_back_session() {
		new_test_ext().execute_with(|| {
			// We are after 4 validators, the mock is set up for 4
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 3));
			// Check we get rotation
			let epoch = 2;
			let mut block_number = epoch;
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch));
			assert_eq!(
				events(),
				[
					mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 3)),
					mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(0, epoch)),
				]
			);

			run_to_block(block_number);
			let mut ev: Vec<mock::Event> = events();
			// Pop off session event
			ev.pop();
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(FIRST_EPOCH))));

			// Confirm auction and complete auction
			confirm_and_complete_auction(&mut block_number, FIRST_EPOCH);

			// Push back rotation by an epoch so we should see no rotation now for the last epoch
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch * 2));
			block_number += epoch;
			run_to_block(block_number);
			assert_eq!(events(), [
				mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(epoch, epoch * 2)),
			]);
			// Clear the event queue
			System::reset_events();
			// Move forward and now it should rotate
			block_number += epoch;
			run_to_block(block_number);
			let mut ev: Vec<mock::Event> = events();
			// Pop off session event
			ev.pop();
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(SECOND_EPOCH))));
		});
	}

	#[test]
	fn limit_validator_set_size() {
		new_test_ext().execute_with(|| {
			// We are after 4 validators, the mock is set up for 4, the first one will be dropped
			// during validation in the auction phase
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 4));
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 4)),
			);
			// Run a rotation
			let epoch = 2;
			let mut block_number = epoch;
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch));
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(0, epoch)),
			);
			// Clear the event queue
			System::reset_events();
			run_to_block(block_number);
			let mut ev: Vec<mock::Event> = events();
			// Pop off session event
			ev.pop();
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(FIRST_EPOCH))));
			confirm_and_complete_auction(&mut block_number, FIRST_EPOCH);

			// Reduce size of validator set, we should see next set of candidates reduced from 4 to 2
			assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 2));
			block_number += epoch;
			run_to_block(block_number);

			let mut ev: Vec<mock::Event> = events();
			// Pop off session event
			ev.pop();
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(SECOND_EPOCH))));
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(4, 2))));

			confirm_and_complete_auction(&mut block_number, SECOND_EPOCH);

			// We should expect 3 outgoing as we dropped one in the auction due to stake of 0
			assert_eq!(mock::current_validators().len(), 2);
			assert_eq!(mock::outgoing_validators().len(), 3);

			// One more to see the rotation maintain the new set size of 2
			block_number += epoch;
			run_to_block(block_number);
			let mut ev: Vec<mock::Event> = events();
			// Pop off session event
			ev.pop();
			assert_eq!(ev.pop(), Some(mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(THIRD_EPOCH))));

			confirm_and_complete_auction(&mut block_number, THIRD_EPOCH);

			assert_eq!(mock::current_validators().len(), 2);
			assert_eq!(mock::outgoing_validators().len(), 2);
		});
	}
}
