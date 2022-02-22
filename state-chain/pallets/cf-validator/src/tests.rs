use crate::{mock::*, Error, *};
use cf_traits::{AuctionError, IsOutgoing};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::{BadOrigin, Zero};

const ALICE: u64 = 100;
const BOB: u64 = 101;
const GENESIS_EPOCH: u32 = 1;

fn last_event() -> mock::Event {
	frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
}

fn initialise_validator(epoch: u64) {
	assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
	assert_eq!(
		<ValidatorPallet as EpochInfo>::epoch_index(),
		GENESIS_EPOCH,
		"we should be in the genesis epoch({})",
		GENESIS_EPOCH
	);
}

fn assert_next_epoch() {
	assert_eq!(
		<ValidatorPallet as EpochInfo>::epoch_index(),
		GENESIS_EPOCH + 1,
		"we should be in epoch {}",
		GENESIS_EPOCH + 1
	);
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
fn changing_epoch_block_size() {
	new_test_ext().execute_with(|| {
		assert_eq!(<Test as Config>::MinEpoch::get(), 1, "the minimum epoch interval should be 1");
		// Throw up an error if we supply anything less than this
		assert_noop!(
			ValidatorPallet::set_blocks_for_epoch(Origin::root(), 0),
			Error::<Test>::InvalidEpoch
		);
		// This should work as 2 > 1
		assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), 2));
		assert_eq!(
			last_event(),
			mock::Event::ValidatorPallet(crate::Event::EpochDurationChanged(0, 2)),
			"an event of the duration change from 0 to 2"
		);
		// We throw up an error if we try to set it to the current
		assert_noop!(
			ValidatorPallet::set_blocks_for_epoch(Origin::root(), 2),
			Error::<Test>::InvalidEpoch
		);
	});
}

#[test]
fn should_request_emergency_rotation() {
	new_test_ext().execute_with(|| {
		let epoch = 10;
		initialise_validator(epoch);
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
	});
}

#[test]
fn should_retry_rotation_until_success() {
	new_test_ext().execute_with(|| {
		let epoch = 10;
		initialise_validator(epoch);
		MockAuctioneer::set_run_behaviour(Err(AuctionError::NotEnoughBidders));
		run_to_block(epoch);
		// Move forward a few blocks, the auction will be failing
		move_forward_blocks(100);
		assert_eq!(
			<ValidatorPallet as EpochInfo>::epoch_index(),
			GENESIS_EPOCH,
			"we should still be in the first epoch"
		);
		// The auction now runs
		MockAuctioneer::set_run_behaviour(Ok(Default::default()));

		move_forward_blocks(BLOCKS_TO_SESSION_ROTATION);
		assert_next_epoch();
	});
}

#[test]
fn should_be_unable_to_force_rotation_during_a_rotation() {
	new_test_ext().execute_with(|| {
		let epoch = 10;
		initialise_validator(epoch);
		MockAuctioneer::set_run_behaviour(Ok(Default::default()));
		run_to_block(epoch);
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
		initialise_validator(100);
		let new_validators = vec![1, 2];
		MockAuctioneer::set_run_behaviour(Ok(AuctionResult {
			winners: new_validators.clone(),
			minimum_active_bid: Zero::zero(),
		}));

		// Force an auction at the next block
		assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
		move_forward_blocks(BLOCKS_TO_SESSION_ROTATION);
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_validators(),
			new_validators,
			"a new set of validators should be now validating"
		);
		assert_next_epoch();
	});
}

#[test]
fn should_have_outgoers_after_rotation() {
	new_test_ext().execute_with(|| {
		let epoch = 10;
		initialise_validator(epoch);
		MockAuctioneer::set_run_behaviour(Ok(Default::default()));
		run_to_block(epoch);
		move_forward_blocks(BLOCKS_TO_SESSION_ROTATION);
		assert_next_epoch();
		let outgoing_validators = outgoing_validators();
		assert_eq!(
			outgoing_validators, DUMMY_GENESIS_VALIDATORS,
			"outgoers should be the genesis validators"
		);
		for outgoer in &outgoing_validators {
			assert!(MockIsOutgoing::is_outgoing(outgoer));
		}
	});
}

#[test]
fn should_rotate_at_epoch() {
	// We expect from our `DummyAuction` that we will have our bidders which are then
	// ran through an auction and that the winners of this auction become the validating set
	new_test_ext().execute_with(|| {
		let epoch = 10;
		initialise_validator(epoch);

		let bond = 10;
		let new_validators = vec![1, 2];

		MockAuctioneer::set_run_behaviour(Ok(AuctionResult {
			winners: new_validators.clone(),
			minimum_active_bid: bond,
		}));

		assert_eq!(
			mock::current_validators(),
			DUMMY_GENESIS_VALIDATORS,
			"the current validators should be the genesis validators"
		);
		// Run to the epoch
		run_to_block(epoch);
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_validators(),
			DUMMY_GENESIS_VALIDATORS,
			"we should still be validating with the genesis validators"
		);
		move_forward_blocks(BLOCKS_TO_SESSION_ROTATION);
		assert_next_epoch();
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_validators(),
			new_validators,
			"the new validators are now validating"
		);
		assert_eq!(min_bid(), bond, "bond should be updated");
	});
}

#[test]
fn genesis() {
	new_test_ext().execute_with(|| {
		// We should have a set of validators on genesis with a minimum bid set
		assert_eq!(
			current_validators(),
			DUMMY_GENESIS_VALIDATORS,
			"We should have a set of validators at genesis"
		);
		assert_eq!(
			min_bid(),
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
			last_event(),
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
			last_event(),
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
			last_event(),
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
			last_event(),
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
			last_event(),
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
	});
}
