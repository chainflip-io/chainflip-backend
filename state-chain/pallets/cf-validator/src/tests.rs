use crate::{mock::*, Error, *};
use cf_test_utilities::last_event;
use cf_traits::{
	mocks::{
		reputation_resetter::MockReputationResetter, system_state_info::MockSystemStateInfo,
		vault_rotation::MockVaultRotator,
	},
	SystemStateInfo, VaultRotator,
};
use frame_support::{assert_noop, assert_ok};

const ALICE: u64 = 100;
const BOB: u64 = 101;
const GENESIS_EPOCH: u32 = 1;

fn assert_next_epoch() {
	assert_eq!(
		<ValidatorPallet as EpochInfo>::epoch_index(),
		2,
		"we should be in epoch 2. Rotation status is {:?}, VaultRotator says {:?} / {:?}",
		RotationPhase::<Test>::get(),
		MockVaultRotator::get_vault_rotation_outcome(),
		<Test as crate::Config>::VaultRotator::get_vault_rotation_outcome()
	);
}

#[test]
fn changing_epoch_block_size() {
	new_test_ext().execute_with(|| {
		let min_duration = <Test as Config>::MinEpoch::get();
		assert_eq!(min_duration, 1);
		// Throw up an error if we supply anything less than this
		assert_noop!(
			ValidatorPallet::set_blocks_for_epoch(Origin::root(), min_duration - 1),
			Error::<Test>::InvalidEpoch
		);
		assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), min_duration));
		assert_eq!(
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::EpochDurationChanged(
				EPOCH_DURATION,
				min_duration
			)),
		);
		// We throw up an error if we try to set it to the current
		assert_noop!(
			ValidatorPallet::set_blocks_for_epoch(Origin::root(), min_duration),
			Error::<Test>::InvalidEpoch
		);
	});
}

#[test]
fn should_request_emergency_rotation() {
	new_test_ext().execute_with(|| {
		ValidatorPallet::request_emergency_rotation();
		let mut events = frame_system::Pallet::<Test>::events();
		assert_eq!(
			events.pop().expect("an event").event,
			mock::Event::ValidatorPallet(crate::Event::RotationStatusUpdated(
				RotationStatus::RunAuction
			)),
			"should emit event of a force rotation being requested"
		);
		assert_eq!(
			events.pop().expect("an event").event,
			mock::Event::ValidatorPallet(crate::Event::EmergencyRotationRequested()),
			"should emit event of the request for an emergency rotation"
		);
		assert!(
			ValidatorPallet::emergency_rotation_in_progress(),
			"we should be in an emergency rotation"
		);
		ValidatorPallet::emergency_rotation_completed();
		assert!(
			!ValidatorPallet::emergency_rotation_in_progress(),
			"we should not be in an emergency rotation"
		);

		// Once we've passed the Idle phase, requesting an emergency rotation should have no
		// effect on the rotation status.
		for status in [
			RotationStatusOf::<Test>::RunAuction,
			RotationStatusOf::<Test>::AwaitingVaults(Default::default()),
			RotationStatusOf::<Test>::VaultsRotated(Default::default()),
			RotationStatusOf::<Test>::SessionRotating(Default::default()),
		] {
			RotationPhase::<Test>::put(&status);
			ValidatorPallet::request_emergency_rotation();
			assert_eq!(RotationPhase::<Test>::get(), status,);
			ValidatorPallet::emergency_rotation_completed();
		}
	});
}

#[test]
fn should_retry_rotation_until_success_with_failing_auctions() {
	new_test_ext().execute_with(|| {
		MockAuctioneer::set_run_behaviour(Err("auction failed"));
		run_to_block(EPOCH_DURATION);
		// Move forward a few blocks, the auction will be failing
		move_forward_blocks(100);
		assert_eq!(
			<ValidatorPallet as EpochInfo>::epoch_index(),
			GENESIS_EPOCH,
			"we should still be in the first epoch"
		);
		// The auction now runs
		MockAuctioneer::set_run_behaviour(Ok(Default::default()));

		move_forward_blocks(1);
		assert!(matches!(
			RotationPhase::<Test>::get(),
			RotationStatusOf::<Test>::AwaitingVaults(..)
		))
	});
}

#[test]
fn should_retry_rotation_until_success_with_failing_vault_rotations() {
	new_test_ext().execute_with(|| {
		MockVaultRotator::set_error_on_start(true);
		RotationPhase::<Test>::set(RotationStatusOf::<Test>::RunAuction);
		// Move forward a few blocks, vault rotations fail because the vault rotator can't start.
		// We keep trying to resolve the auction.
		move_forward_blocks(10);
		assert_eq!(
			<ValidatorPallet as EpochInfo>::epoch_index(),
			GENESIS_EPOCH,
			"we should still be in the first epoch"
		);
		assert_eq!(ValidatorPallet::rotation_phase(), RotationStatus::RunAuction);

		// Allow vault rotations to progress.
		// The keygen ceremony will fail.
		MockVaultRotator::set_error_on_start(false);
		MockVaultRotator::failing();

		for i in 0..10 {
			move_forward_blocks(1);
			assert!(matches!(
				ValidatorPallet::rotation_phase(),
				RotationStatus::AwaitingVaults(..)
			));
			move_forward_blocks(1);
			assert_eq!(
				ValidatorPallet::rotation_phase(),
				RotationStatus::RunAuction,
				"Status is {:?} at iteration {:?}",
				ValidatorPallet::rotation_phase(),
				i
			);
		}

		assert_eq!(
			<ValidatorPallet as EpochInfo>::epoch_index(),
			GENESIS_EPOCH,
			"we should still be in the first epoch"
		);

		// Allow keygen to succeed.
		MockVaultRotator::succeeding();

		// Four blocks - one for auction, one for keygen, one for each session rotation.
		move_forward_blocks(4);
		assert_next_epoch();
	});
}

#[test]
fn should_be_unable_to_force_rotation_during_a_rotation() {
	new_test_ext().execute_with(|| {
		MockAuctioneer::set_run_behaviour(Ok(Default::default()));
		run_to_block(EPOCH_DURATION);
		assert_eq!(ValidatorPallet::rotation_phase(), RotationStatus::RunAuction);
		assert_noop!(
			ValidatorPallet::force_rotation(Origin::root()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn should_rotate_when_forced() {
	new_test_ext().execute_with(|| {
		assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
		assert!(matches!(RotationPhase::<Test>::get(), RotationStatusOf::<Test>::RunAuction));
	});
}

#[test]
fn auction_winners_should_be_the_new_authorities_on_new_epoch() {
	new_test_ext().execute_with(|| {
		let new_bond = 10;
		let new_authorities = vec![1, 2];

		MockAuctioneer::set_run_behaviour(Ok(AuctionResult {
			winners: new_authorities.clone(),
			minimum_active_bid: new_bond,
		}));

		assert_eq!(
			Authorities::<Test>::get(),
			DUMMY_GENESIS_VALIDATORS,
			"the current authorities should be the genesis authorities"
		);
		// Run to the epoch boundary.
		run_to_block(EPOCH_DURATION);
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_authorities(),
			DUMMY_GENESIS_VALIDATORS,
			"we should still be validating with the genesis authorities"
		);
		assert!(matches!(RotationPhase::<Test>::get(), RotationStatusOf::<Test>::RunAuction));
		move_forward_blocks(1);
		assert!(matches!(
			RotationPhase::<Test>::get(),
			RotationStatusOf::<Test>::AwaitingVaults(..)
		));
		move_forward_blocks(3); // Three blocks - one for keygen, one for each session rotation.
		assert_next_epoch();
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_authorities(),
			new_authorities,
			"the new authorities are now validating"
		);
		assert_eq!(Bond::<Test>::get(), new_bond, "bond should be updated");

		let auction_winners = AUCTION_WINNERS
			.with(|cell| (*cell.borrow()).clone())
			.expect("no value for auction winners is provided!");

		// Expect new_authorities to be auction winners as well
		assert_eq!(new_authorities, auction_winners);
	});
}

#[test]
fn genesis() {
	new_test_ext().execute_with(|| {
		// We should have a set of validators on genesis with a minimum bid set
		assert_eq!(
			Authorities::<Test>::get(),
			DUMMY_GENESIS_VALIDATORS,
			"We should have a set of validators at genesis"
		);
		assert_eq!(
			Bond::<Test>::get(),
			MINIMUM_ACTIVE_BID_AT_GENESIS,
			"We should have a minimum bid at genesis"
		);
		assert_eq!(
			<ValidatorPallet as EpochInfo>::epoch_index(),
			GENESIS_EPOCH,
			"we should be in the genesis epoch({})",
			GENESIS_EPOCH
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
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::CFEVersionUpdated(
				validator,
				SemVer::default(),
				version.clone()
			)),
			"should emit event on updated version"
		);

		assert_eq!(
			version,
			ValidatorPallet::validator_cfe_version(validator),
			"version should be stored"
		);

		// We submit a new version
		let new_version = SemVer { major: 5, ..Default::default() };
		assert_ok!(ValidatorPallet::cfe_version(Origin::signed(validator), new_version.clone()));

		assert_eq!(
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::CFEVersionUpdated(
				validator,
				version,
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
		assert_ok!(ValidatorPallet::cfe_version(Origin::signed(validator), new_version.clone()));

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

#[test]
fn register_peer_id() {
	new_test_ext().execute_with(|| {
		use sp_core::{Encode, Pair};

		let alice_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("alice", None);
		let alice_peer_public_key = alice_peer_keypair.public();

		// Don't allow invalid signatures
		assert_noop!(
			ValidatorPallet::register_peer_id(
				Origin::signed(ALICE),
				alice_peer_public_key,
				0,
				0,
				alice_peer_keypair.sign(&BOB.encode()[..]),
			),
			Error::<Test>::InvalidAccountPeerMappingSignature
		);

		// Non-overlaping peer ids and valid signatures
		assert_ok!(ValidatorPallet::register_peer_id(
			Origin::signed(ALICE),
			alice_peer_public_key,
			40044,
			10,
			alice_peer_keypair.sign(&ALICE.encode()[..]),
		));
		assert_eq!(
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::PeerIdRegistered(
				ALICE,
				alice_peer_public_key,
				40044,
				10
			)),
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&alice_peer_public_key), Some(()));
		assert_eq!(
			ValidatorPallet::validator_peer_id(&ALICE),
			Some((ALICE, alice_peer_public_key, 40044, 10))
		);

		// New mappings to overlapping peer id are disallowed
		assert_noop!(
			ValidatorPallet::register_peer_id(
				Origin::signed(BOB),
				alice_peer_public_key,
				0,
				0,
				alice_peer_keypair.sign(&BOB.encode()[..]),
			),
			Error::<Test>::AccountPeerMappingOverlap
		);

		// New validator mapping works
		let bob_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("bob", None);
		let bob_peer_public_key = bob_peer_keypair.public();
		assert_ok!(ValidatorPallet::register_peer_id(
			Origin::signed(BOB),
			bob_peer_public_key,
			40043,
			11,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		),);
		assert_eq!(
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::PeerIdRegistered(
				BOB,
				bob_peer_public_key,
				40043,
				11
			)),
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(
			ValidatorPallet::validator_peer_id(&BOB),
			Some((BOB, bob_peer_public_key, 40043, 11))
		);

		// Changing existing mapping to overlapping peer id is disallowed
		assert_noop!(
			ValidatorPallet::register_peer_id(
				Origin::signed(BOB),
				alice_peer_public_key,
				0,
				0,
				alice_peer_keypair.sign(&BOB.encode()[..]),
			),
			Error::<Test>::AccountPeerMappingOverlap
		);

		let bob_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("bob2", None);
		let bob_peer_public_key = bob_peer_keypair.public();

		// Changing to new peer id works
		assert_ok!(ValidatorPallet::register_peer_id(
			Origin::signed(BOB),
			bob_peer_public_key,
			40043,
			11,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		));
		assert_eq!(
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::PeerIdRegistered(
				BOB,
				bob_peer_public_key,
				40043,
				11
			)),
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(
			ValidatorPallet::validator_peer_id(&BOB),
			Some((BOB, bob_peer_public_key, 40043, 11))
		);

		// Updating only the ip address works
		assert_ok!(ValidatorPallet::register_peer_id(
			Origin::signed(BOB),
			bob_peer_public_key,
			40043,
			12,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		));
		assert_eq!(
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::PeerIdRegistered(
				BOB,
				bob_peer_public_key,
				40043,
				12
			)),
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(
			ValidatorPallet::validator_peer_id(&BOB),
			Some((BOB, bob_peer_public_key, 40043, 12))
		);
	});
}

#[test]
fn historical_epochs() {
	new_test_ext().execute_with(|| {
		// Activate an epoch for ALICE
		EpochHistory::<Test>::activate_epoch(&ALICE, 1);
		// Expect the the epoch to be in the storage for ALICE
		assert!(HistoricalActiveEpochs::<Test>::get(ALICE).contains(&1));
		// Activate the next epoch
		EpochHistory::<Test>::activate_epoch(&ALICE, 2);
		// Remove epoch 1 for ALICE
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 1);
		// Expect the epoch to be removed
		assert!(!HistoricalActiveEpochs::<Test>::get(ALICE).contains(&1));
		// and epoch 2 still in storage
		assert!(HistoricalActiveEpochs::<Test>::get(ALICE).contains(&2));
		// Deactivate epoch 2
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 2);
		// And expect the historical active epoch array for ALICE to be empty
		assert!(HistoricalActiveEpochs::<Test>::get(ALICE).is_empty());
	});
}

#[test]
fn highest_bond() {
	new_test_ext().execute_with(|| {
		// Epoch 1
		EpochHistory::<Test>::activate_epoch(&ALICE, 1);
		HistoricalAuthorities::<Test>::insert(1, vec![ALICE]);
		HistoricalBonds::<Test>::insert(1, 10);
		// Epoch 2
		EpochHistory::<Test>::activate_epoch(&ALICE, 2);
		HistoricalAuthorities::<Test>::insert(2, vec![ALICE]);
		HistoricalBonds::<Test>::insert(2, 30);
		// Epoch 3
		EpochHistory::<Test>::activate_epoch(&ALICE, 3);
		HistoricalAuthorities::<Test>::insert(3, vec![ALICE]);
		HistoricalBonds::<Test>::insert(3, 20);
		// Expect the bond of epoch 2
		assert_eq!(EpochHistory::<Test>::active_bond(&ALICE), 30);
		// Deactivate all epochs
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 1);
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 2);
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 3);
		// Expect the bond to be zero if there is no epoch the validator is active in
		assert_eq!(EpochHistory::<Test>::active_bond(&ALICE), 0);
	});
}

#[test]
fn test_setting_vanity_names_() {
	new_test_ext().execute_with(|| {
		let validators: &[u64] = &[123, 456, 789, 101112];
		assert_ok!(ValidatorPallet::set_vanity_name(Origin::signed(validators[0]), "Test Validator 1".as_bytes().to_vec()));
		assert_ok!(ValidatorPallet::set_vanity_name(Origin::signed(validators[2]), "Test Validator 2".as_bytes().to_vec()));
		let vanity_names = crate::VanityNames::<Test>::get();
		assert_eq!(sp_std::str::from_utf8(vanity_names.get(&validators[0]).unwrap()).unwrap(), "Test Validator 1");
		assert_eq!(sp_std::str::from_utf8(vanity_names.get(&validators[2]).unwrap()).unwrap(), "Test Validator 2");
		assert_noop!(ValidatorPallet::set_vanity_name(Origin::signed(validators[0]), [0xfe, 0xff].to_vec()), crate::Error::<Test>::InvalidCharactersInName);
		assert_noop!(ValidatorPallet::set_vanity_name(Origin::signed(validators[0]), "Validator Name too longggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg".as_bytes().to_vec()), crate::Error::<Test>::NameTooLong);
	});
}

#[test]
fn test_missing_author_punishment() {
	new_test_ext().execute_with(|| {
		RotationPhase::<Test>::set(RotationStatusOf::<Test>::VaultsRotated(AuctionResult {
			winners: vec![1, 2, 3, 4],
			..Default::default()
		}));
		move_forward_blocks(2);

		// Use a large offset to ensure the modulo math selects the correct validators.
		let offset = 4 * 123456;
		MockMissedAuthorshipSlots::set(vec![1 + offset, 2 + offset]);
		move_forward_blocks(1);
		MockOffenceReporter::assert_reported(
			PalletOffence::MissedAuthorshipSlot,
			ValidatorPallet::authorities().get(1..=2).unwrap().to_vec(),
		)
	})
}

#[test]
fn no_auction_during_maintenance() {
	new_test_ext().execute_with(|| {
		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		// Assert that we are in maintenance mode
		assert!(MockSystemStateInfo::ensure_no_maintenance().is_err());
		// Try to start an auction
		RotationPhase::<Test>::set(RotationStatusOf::<Test>::RunAuction);
		// Move a few blocks forward to trigger the auction
		move_forward_blocks(1);
		// Expect the auction to not be started - we are stll in the auction mode and not moving
		// from here
		assert_eq!(RotationPhase::<Test>::get(), RotationStatusOf::<Test>::RunAuction);
		// Deactivate maintenance mode
		MockSystemStateInfo::set_maintenance(false);
		// Expect the maintenance mode to be deactivated
		assert!(MockSystemStateInfo::ensure_no_maintenance().is_ok());
		// Move a couple of blocks forward to run the auction
		move_forward_blocks(2);
		// Expect the auction to be to be completed
		assert_eq!(
			RotationPhase::<Test>::get(),
			RotationStatusOf::<Test>::VaultsRotated(AuctionResult {
				winners: vec![],
				minimum_active_bid: 0
			})
		);
	});
}

#[test]
fn test_reputation_reset() {
	new_test_ext().execute_with_unchecked_invariants(|| {
		// Simulate an epoch rotation and give the validators some reputation.
		RotationPhase::<Test>::put(RotationStatusOf::<Test>::SessionRotating(AuctionResult {
			winners: vec![1, 2, 3],
			..Default::default()
		}));
		<ValidatorPallet as pallet_session::SessionManager<_>>::start_session(0);

		for id in &ValidatorPallet::current_validators() {
			MockReputationResetter::<Test>::set_reputation(id, 100);
		}

		let first_epoch = ValidatorPallet::current_epoch();

		// Simulate another epoch rotation and give the validators some reputation.
		RotationPhase::<Test>::put(RotationStatusOf::<Test>::SessionRotating(AuctionResult {
			winners: vec![4, 5, 6],
			..Default::default()
		}));
		<ValidatorPallet as pallet_session::SessionManager<_>>::start_session(0);

		for id in &ValidatorPallet::current_validators() {
			MockReputationResetter::<Test>::set_reputation(id, 100);
		}

		for id in &[1, 2, 3, 4, 5, 6] {
			assert_eq!(MockReputationResetter::<Test>::get_reputation(id), 100);
		}

		ValidatorPallet::expire_epoch(first_epoch);

		for id in &[1, 2, 3] {
			assert_eq!(MockReputationResetter::<Test>::get_reputation(id), 0);
		}
		for id in &[4, 5, 6] {
			assert_eq!(MockReputationResetter::<Test>::get_reputation(id), 100);
		}
	})
}
