mod tests {
	use crate::{
		mock::{ALICE, *},
		Error, *,
	};
	use cf_traits::{AuctionError, IsOutgoing};
	use frame_support::{assert_noop, assert_ok};
	use sp_runtime::traits::{BadOrigin, Zero};

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
	}

	#[test]
	fn you_have_to_be_priviledged() {
		new_test_ext().execute_with(|| {
			// Run through the sudo extrinsics to be sure they are what they are
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::signed(ALICE), Zero::zero()),
				BadOrigin
			);
			assert_noop!(ValidatorPallet::force_rotation(Origin::signed(ALICE)), BadOrigin);
		});
	}

	#[test]
	fn changing_epoch() {
		new_test_ext().execute_with(|| {
			// Confirm we have a minimum epoch of 1 block
			assert_eq!(<Test as Config>::MinEpoch::get(), 1, "should be in epoch 1");
			// Throw up an error if we supply anything less than this
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 0),
				Error::<Test>::InvalidEpoch
			);
			// This should work as 2 > 1
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), 2));
			// Confirm we have an event for the change from 0 to 2
			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::EpochDurationChanged(0, 2)),
				"change of duration should be from 0 to 2"
			);
			// We throw up an error if we try to set it to the current
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 2),
				Error::<Test>::InvalidEpoch
			);
		});
	}

	#[test]
	fn should_rotate_on_epoch_and_have_new_set_of_validators_with_bond() {
		new_test_ext().execute_with(|| {
			// Set block length of epoch to 10
			let epoch = 10;
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			// Run to epoch
			let next_epoch = ValidatorPallet::current_epoch_started_at() + epoch;
			run_to_block(next_epoch);
			assert_eq!(
				ValidatorPallet::validators(),
				DUMMY_GENESIS_VALIDATORS,
				"we should still have our genesis validators"
			);
			assert_eq!(ValidatorPallet::bond(), BID_TO_BE_USED, "we should have our initial bond");
			move_forward_by_blocks(1);
			assert_eq!(
				ValidatorPallet::validators(),
				MockBidderProvider::bidders(),
				"we should have a new set of validators who bidded"
			);
			assert_eq!(ValidatorPallet::bond(), NEW_BID_TO_BE_USED, "we should have our new bond");
		})
	}

	#[test]
	fn should_rotate_on_force_epoch_and_have_new_set_of_validators_with_bond() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			move_forward_by_blocks(1);
			assert_eq!(
				ValidatorPallet::validators(),
				DUMMY_GENESIS_VALIDATORS,
				"we should still have our genesis validators"
			);
			assert_eq!(ValidatorPallet::bond(), BID_TO_BE_USED, "we should have our initial bond");
			move_forward_by_blocks(1);
			assert_eq!(
				ValidatorPallet::validators(),
				MockBidderProvider::bidders(),
				"we should have a new set of validators who bidded"
			);
			assert_eq!(ValidatorPallet::bond(), NEW_BID_TO_BE_USED, "we should have our new bond");
		})
	}

	#[test]
	fn should_not_be_able_to_set_epoch_during_auction_phase() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			move_forward_by_blocks(1);
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 10),
				Error::<Test>::AuctionInProgress
			);
		})
	}

	#[test]
	fn should_restart_when_auction_fails() {
		new_test_ext().execute_with(|| {
			let epoch = 2;
			assert_eq!(false, ValidatorPallet::force(), "the force flag should be set false");
			MockAuctioneer::set_auction_error(Some(AuctionError::Empty));
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			move_forward_by_blocks(1);
			assert_eq!(true, ValidatorPallet::force(), "the abort should force a new auction");
		})
	}

	#[test]
	fn should_set_outgoers_at_end_of_epoch() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			move_forward_by_blocks(1);

			let outgoing_validators: Vec<_> = old_validators()
				.iter()
				.filter(|old_validator| !ValidatorPallet::validators().contains(old_validator))
				.cloned()
				.collect();

			for outgoer in &outgoing_validators {
				assert!(MockIsOutgoing::is_outgoing(outgoer));
			}
		})
	}

	#[test]
	fn should_reset_after_an_emergency_rotation_has_been_requested() {
		new_test_ext().execute_with(|| {
			<ValidatorPallet as EmergencyRotation>::request_emergency_rotation();
			move_forward_by_blocks(2);
			assert_eq!(
				ValidatorPallet::validators(),
				MockBidderProvider::bidders(),
				"we should have a new set of validators who bidded"
			);
			assert_eq!(
				false,
				<ValidatorPallet as EmergencyRotation>::emergency_rotation_in_progress(),
				"Emergency rotation should be reset"
			);
		})
	}

	#[test]
	fn should_repeat_auction_after_forcing_auction_and_then_aborted_auction() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			move_forward_by_blocks(1);
			assert_eq!(MockAuctioneer::auction_index(), 1, "should see a new auction");
			// Abort the current auction
			<MockAuctioneer as Auctioneer>::abort();
			move_forward_by_blocks(1);
			assert_eq!(MockAuctioneer::auction_index(), 2, "should see a new auction");
		});
	}

	#[test]
	fn genesis() {
		new_test_ext().execute_with(|| {
			// We should have a set of validators on genesis with a minimum bid set
			assert_eq!(
				ValidatorPallet::validators().len(),
				DUMMY_GENESIS_VALIDATORS.len(),
				"We should have a set of validators at genesis"
			);
			assert_eq!(ValidatorPallet::bond(), BID_TO_BE_USED, "We should have a minimum bid set");
			assert_eq!(
				ValidatorPallet::current_epoch(),
				1,
				"the first epoch should be the first epoch"
			);
		});
	}

	#[test]
	fn send_cfe_version() {
		new_test_ext().execute_with(|| {
			// We initially submit version
			let validator = DUMMY_GENESIS_VALIDATORS[0];

			let version = SemVer { major: 4, ..Default::default() };
			assert_ok!(ValidatorPallet::cfe_version(Origin::signed(validator), version.clone(),));

			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::CFEVersionUpdated(
					validator,
					SemVer::default(),
					version.clone()
				)),
				"should emit event on updated version"
			);

			assert_eq!(
				version.clone(),
				ValidatorPallet::validator_cfe_version(validator),
				"version should be stored"
			);

			// We submit a new version
			let new_version = SemVer { major: 5, ..Default::default() };
			assert_ok!(ValidatorPallet::cfe_version(
				Origin::signed(validator),
				new_version.clone()
			));

			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::CFEVersionUpdated(
					validator,
					version.clone(),
					new_version.clone()
				)),
				"should emit event on updated version"
			);

			assert_eq!(
				new_version,
				ValidatorPallet::validator_cfe_version(validator),
				"new version should be stored"
			);

			// When we submit the same version we should see no `CFEVersionUpdated` event
			frame_system::Pallet::<Test>::reset_events();
			assert_ok!(ValidatorPallet::cfe_version(
				Origin::signed(validator),
				new_version.clone()
			));

			assert_eq!(
				0,
				frame_system::Pallet::<Test>::events().len(),
				"We should have no events of an update"
			);

			assert_eq!(
				new_version,
				ValidatorPallet::validator_cfe_version(validator),
				"we should be still on the same new version"
			);
		});
	}
}
