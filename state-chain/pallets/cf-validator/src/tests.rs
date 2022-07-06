use crate::{mock::*, Error, *};
use cf_test_utilities::last_event;
use cf_traits::{
	mocks::{
		reputation_resetter::MockReputationResetter, system_state_info::MockSystemStateInfo,
		vault_rotation::MockVaultRotator,
	},
	AuctionOutcome, BackupNodes, SystemStateInfo, VaultRotator,
};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;

const ALICE: u64 = 100;
const BOB: u64 = 101;
const GENESIS_EPOCH: u32 = 1;

fn assert_epoch_number(n: EpochIndex) {
	assert_eq!(
		<ValidatorPallet as EpochInfo>::epoch_index(),
		n,
		"we should be in epoch {n:?}. Rotation status is {:?}, VaultRotator says {:?} / {:?}",
		CurrentRotationPhase::<Test>::get(),
		MockVaultRotator::get_vault_rotation_outcome(),
		<Test as crate::Config>::VaultRotator::get_vault_rotation_outcome()
	);
}

macro_rules! assert_default_auction_outcome {
	() => {
		assert_epoch_number(GENESIS_EPOCH + 1);
		assert_eq!(Bond::<Test>::get(), BOND, "bond should be updated");
		assert_eq!(<ValidatorPallet as EpochInfo>::current_authorities(), AUCTION_WINNERS.to_vec());
		assert_eq!(
			ValidatorPallet::backup_nodes(),
			AUCTION_LOSERS[..ValidatorPallet::backup_set_target_size(
				AUCTION_WINNERS.len(),
				BackupNodePercentage::<Test>::get()
			)]
				.to_vec(),
			"backup nodes should be updated and should not include the unqualified node"
		);
	};
}

fn simple_rotation_status(
	auction_winners: Vec<u64>,
	bond: Option<u128>,
) -> RuntimeRotationStatus<Test> {
	RuntimeRotationStatus::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
		winners: auction_winners,
		bond: bond.unwrap_or(100),
		losers: AUCTION_LOSERS.zip(LOSING_BIDS).map(Into::into).to_vec(),
	})
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
		<ValidatorPallet as EmergencyRotation>::request_emergency_rotation();
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::VaultsRotating(..)
		));

		// Once we've passed the Idle phase, requesting an emergency rotation should have no
		// effect on the rotation status.
		for status in [
			RotationPhase::<Test>::VaultsRotating(Default::default()),
			RotationPhase::<Test>::VaultsRotated(Default::default()),
			RotationPhase::<Test>::SessionRotating(Default::default()),
		] {
			CurrentRotationPhase::<Test>::put(&status);
			ValidatorPallet::request_emergency_rotation();
			assert_eq!(CurrentRotationPhase::<Test>::get(), status,);
		}
	});
}

#[test]
fn should_retry_rotation_until_success_with_failing_auctions() {
	new_test_ext().execute_with(|| {
		// store this for later:
		let auction_outcome = MockAuctioneer::resolve_auction().unwrap();
		MockAuctioneer::set_next_auction_outcome(Err("auction failed"));
		run_to_block(EPOCH_DURATION);
		// Move forward a few blocks, the auction will be failing
		move_forward_blocks(100);
		assert_epoch_number(GENESIS_EPOCH);
		assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);

		// The auction now runs
		MockAuctioneer::set_next_auction_outcome(Ok(auction_outcome));

		move_forward_blocks(1);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::VaultsRotating(..)
		));

		while CurrentRotationPhase::<Test>::get() != RotationPhase::<Test>::Idle {
			move_forward_blocks(1);
		}
		assert_epoch_number(GENESIS_EPOCH + 1);
	});
}

#[test]
fn should_retry_rotation_until_success_with_failing_vault_rotations() {
	new_test_ext().execute_with(|| {
		MockVaultRotator::set_error_on_start(true);
		ValidatorPallet::start_authority_rotation();

		// Move forward a few blocks, vault rotations fail because the vault rotator can't start.
		// We keep trying to resolve the auction.
		move_forward_blocks(10);
		assert_epoch_number(GENESIS_EPOCH);
		assert_eq!(ValidatorPallet::current_rotation_phase(), RotationPhase::Idle);

		// Allow vault rotations to progress.
		// The keygen ceremony will fail.
		MockVaultRotator::set_error_on_start(false);
		MockVaultRotator::failing(vec![]);

		for i in 0..10 {
			move_forward_blocks(1);
			assert!(
				matches!(
					ValidatorPallet::current_rotation_phase(),
					RotationPhase::VaultsRotating(..)
				),
				"at iteration {i:?}, got {:?}",
				ValidatorPallet::current_rotation_phase()
			);
		}

		assert_epoch_number(GENESIS_EPOCH);

		// Allow keygen to succeed.
		MockVaultRotator::succeeding();

		// Four blocks - one for keygen, one for each session rotation, then one more because the
		// vaults on_initialise runs *after* the validator pallet's on_initialise.
		// TODO: Address this as part of issue [#1702](https://github.com/chainflip-io/chainflip-backend/issues/1702)
		move_forward_blocks(5);
		assert_default_auction_outcome!();
	});
}

#[test]
fn should_be_unable_to_force_rotation_during_a_rotation() {
	new_test_ext().execute_with(|| {
		ValidatorPallet::start_authority_rotation();
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
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::VaultsRotating(..)
		));
		assert_noop!(
			ValidatorPallet::force_rotation(Origin::root()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn auction_winners_should_be_the_new_authorities_on_new_epoch() {
	new_test_ext().execute_with(|| {
		assert_eq!(
			CurrentAuthorities::<Test>::get(),
			GENESIS_VALIDATORS,
			"the current authorities should be the genesis authorities"
		);
		// Run to the epoch boundary.
		run_to_block(EPOCH_DURATION);
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_authorities(),
			GENESIS_VALIDATORS,
			"we should still be validating with the genesis authorities"
		);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::VaultsRotating(..)
		));
		while <ValidatorPallet as EpochInfo>::epoch_index() == GENESIS_EPOCH {
			move_forward_blocks(1);
		}
		assert_default_auction_outcome!();
	});
}

#[test]
fn genesis() {
	new_test_ext().execute_with(|| {
		assert_eq!(
			CurrentAuthorities::<Test>::get(),
			GENESIS_VALIDATORS,
			"We should have a set of validators at genesis"
		);
		assert_eq!(Bond::<Test>::get(), GENESIS_BOND, "We should have a minimum bid at genesis");
		assert_epoch_number(GENESIS_EPOCH);
		assert_invariants!();
	});
}

#[test]
fn send_cfe_version() {
	new_test_ext().execute_with(|| {
		// We initially submit version
		let authority = GENESIS_VALIDATORS[0];

		let version = SemVer { major: 4, ..Default::default() };
		assert_ok!(ValidatorPallet::cfe_version(Origin::signed(authority), version.clone(),));

		assert_eq!(
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::CFEVersionUpdated(
				authority,
				SemVer::default(),
				version.clone()
			)),
			"should emit event on updated version"
		);

		assert_eq!(
			version,
			ValidatorPallet::node_cfe_version(authority),
			"version should be stored"
		);

		// We submit a new version
		let new_version = SemVer { major: 5, ..Default::default() };
		assert_ok!(ValidatorPallet::cfe_version(Origin::signed(authority), new_version.clone()));

		assert_eq!(
			last_event::<Test>(),
			mock::Event::ValidatorPallet(crate::Event::CFEVersionUpdated(
				authority,
				version,
				new_version.clone()
			)),
			"should emit event on updated version"
		);

		assert_eq!(
			new_version,
			ValidatorPallet::node_cfe_version(authority),
			"new version should be stored"
		);

		// When we submit the same version we should see no `CFEVersionUpdated` event
		frame_system::Pallet::<Test>::reset_events();
		assert_ok!(ValidatorPallet::cfe_version(Origin::signed(authority), new_version.clone()));

		assert_eq!(
			0,
			frame_system::Pallet::<Test>::events().len(),
			"We should have no events of an update"
		);

		assert_eq!(
			new_version,
			ValidatorPallet::node_cfe_version(authority),
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
			ValidatorPallet::node_peer_id(&ALICE),
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

		// New authority mapping works
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
			ValidatorPallet::node_peer_id(&BOB),
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
			ValidatorPallet::node_peer_id(&BOB),
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
			ValidatorPallet::node_peer_id(&BOB),
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
		// Expect the bond to be zero if there is no epoch the node is active in
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
		// Use a large offset to ensure the modulo math selects the correct validators.
		let offset: u64 = GENESIS_VALIDATORS.len() as u64 * 123456;
		let (expected_authority_index, authored_authority_index) = (1usize, 3usize);
		MockMissedAuthorshipSlots::set(
			expected_authority_index as u64 + offset,
			authored_authority_index as u64 + offset,
		);
		move_forward_blocks(1);
		MockOffenceReporter::assert_reported(
			PalletOffence::MissedAuthorshipSlot,
			ValidatorPallet::current_authorities()
				.get(expected_authority_index..authored_authority_index)
				.unwrap()
				.to_vec(),
		)
	})
}

#[test]
fn no_auction_during_maintenance() {
	new_test_ext().execute_with(|| {
		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		// Assert that we are in maintenance mode
		assert!(MockSystemStateInfo::is_maintenance_mode());
		// Try to start a rotation.
		ValidatorPallet::start_authority_rotation();
		ValidatorPallet::force_rotation(RawOrigin::Root.into()).unwrap();
		<ValidatorPallet as EmergencyRotation>::request_emergency_rotation();

		assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);

		// Deactivate maintenance mode
		MockSystemStateInfo::set_maintenance(false);
		assert!(!MockSystemStateInfo::is_maintenance_mode());

		// Try to start a rotation.
		ValidatorPallet::start_authority_rotation();
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::VaultsRotating(..)
		));
	});
}

#[test]
fn test_reputation_reset() {
	new_test_ext().execute_with_unchecked_invariants(|| {
		// Simulate an epoch rotation and give the validators some reputation.
		CurrentRotationPhase::<Test>::put(RotationPhase::<Test>::SessionRotating(
			simple_rotation_status(vec![1, 2, 3], None),
		));
		<ValidatorPallet as pallet_session::SessionManager<_>>::start_session(0);

		for id in &ValidatorPallet::current_authorities() {
			MockReputationResetter::<Test>::set_reputation(id, 100);
		}

		let first_epoch = ValidatorPallet::current_epoch();

		// Simulate another epoch rotation and give the validators some reputation.
		CurrentRotationPhase::<Test>::put(RotationPhase::<Test>::SessionRotating(
			simple_rotation_status(vec![4, 5, 6], None),
		));
		<ValidatorPallet as pallet_session::SessionManager<_>>::start_session(0);

		for id in &ValidatorPallet::current_authorities() {
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
	});
}

#[test]
fn backup_set_size_calculation() {
	assert_eq!(Pallet::<Test>::backup_set_target_size(150, 20u8), 30);
	assert_eq!(Pallet::<Test>::backup_set_target_size(150, 33u8), 49);
	assert_eq!(Pallet::<Test>::backup_set_target_size(150, 34u8), 51);
	assert_eq!(Pallet::<Test>::backup_set_target_size(150, 0u8), 0);
	// Saturates at 100%.
	assert_eq!(Pallet::<Test>::backup_set_target_size(150, 200u8), 150);
}

#[cfg(test)]
mod bond_expiry {
	use super::*;

	#[test]
	fn increasing_bond() {
		new_test_ext().execute_with_unchecked_invariants(|| {
			let initial_epoch = ValidatorPallet::current_epoch();
			ValidatorPallet::transition_to_next_epoch(simple_rotation_status(
				vec![1, 2],
				Some(100),
			));
			assert_eq!(ValidatorPallet::bond(), 100);

			ValidatorPallet::transition_to_next_epoch(simple_rotation_status(
				vec![2, 3],
				Some(101),
			));
			assert_eq!(ValidatorPallet::bond(), 101);

			assert_eq!(EpochHistory::<Test>::active_epochs_for_authority(&1), [initial_epoch + 1]);
			assert_eq!(EpochHistory::<Test>::active_bond(&1), 100);
			assert_eq!(
				EpochHistory::<Test>::active_epochs_for_authority(&2),
				[initial_epoch + 1, initial_epoch + 2]
			);
			assert_eq!(EpochHistory::<Test>::active_bond(&2), 101);
			assert_eq!(EpochHistory::<Test>::active_epochs_for_authority(&3), [initial_epoch + 2]);
			assert_eq!(EpochHistory::<Test>::active_bond(&3), 101);
		});
	}

	#[test]
	fn decreasing_bond() {
		new_test_ext().execute_with_unchecked_invariants(|| {
			let initial_epoch = ValidatorPallet::current_epoch();
			ValidatorPallet::transition_to_next_epoch(simple_rotation_status(
				vec![1, 2],
				Some(100),
			));
			assert_eq!(ValidatorPallet::bond(), 100);

			ValidatorPallet::transition_to_next_epoch(simple_rotation_status(vec![2, 3], Some(99)));
			assert_eq!(ValidatorPallet::bond(), 99);

			assert_eq!(EpochHistory::<Test>::active_epochs_for_authority(&1), [initial_epoch + 1]);
			assert_eq!(EpochHistory::<Test>::active_bond(&1), 100);
			assert_eq!(
				EpochHistory::<Test>::active_epochs_for_authority(&2),
				[initial_epoch + 1, initial_epoch + 2]
			);
			assert_eq!(EpochHistory::<Test>::active_bond(&2), 100);
			assert_eq!(EpochHistory::<Test>::active_epochs_for_authority(&3), [initial_epoch + 2]);
			assert_eq!(EpochHistory::<Test>::active_bond(&3), 99);
		});
	}
}
