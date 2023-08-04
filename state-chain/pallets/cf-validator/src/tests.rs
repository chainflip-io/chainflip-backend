#![cfg(test)]

use crate::{mock::*, Error, *};
use cf_test_utilities::{assert_event_sequence, last_event};
use cf_traits::{
	mocks::{
		qualify_node::QualifyAll, reputation_resetter::MockReputationResetter,
		vault_rotator::MockVaultRotatorA,
	},
	AccountRoleRegistry, AuctionOutcome, SafeMode, SetSafeMode,
};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;

const ALICE: u64 = 100;
const BOB: u64 = 101;
const GENESIS_EPOCH: u32 = 1;

fn assert_epoch_index(n: EpochIndex) {
	assert_eq!(
		ValidatorPallet::epoch_index(),
		n,
		"we should be in epoch {n:?}. VaultRotator says {:?} / {:?}",
		CurrentRotationPhase::<Test>::get(),
		<Test as crate::Config>::VaultRotator::status()
	);
}

macro_rules! assert_default_rotation_outcome {
	() => {
		assert!(matches!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle));
		assert_epoch_index(GENESIS_EPOCH + 1);
		assert_eq!(Bond::<Test>::get(), EXPECTED_BOND, "bond should be updated");
		assert_eq!(ValidatorPallet::current_authorities(), BTreeSet::from(AUCTION_WINNERS));
	};
}

fn simple_rotation_state(
	auction_winners: Vec<u64>,
	bond: Option<u128>,
) -> RuntimeRotationState<Test> {
	RuntimeRotationState::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
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
			ValidatorPallet::set_blocks_for_epoch(RuntimeOrigin::root(), min_duration - 1),
			Error::<Test>::InvalidEpoch
		);
		assert_ok!(ValidatorPallet::set_blocks_for_epoch(RuntimeOrigin::root(), min_duration));
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::EpochDurationChanged(
				EPOCH_DURATION,
				min_duration
			)),
		);
		// We throw up an error if we try to set it to the current
		assert_noop!(
			ValidatorPallet::set_blocks_for_epoch(RuntimeOrigin::root(), min_duration),
			Error::<Test>::InvalidEpoch
		);
	});
}

#[test]
fn should_retry_rotation_until_success_with_failing_auctions() {
	new_test_ext().execute_with(|| {
		assert_eq!(MockBidderProvider::get_bidders().len(), 0);
		run_to_block(EPOCH_DURATION);
		// Move forward a few blocks, the auction will be failing
		move_forward_blocks(100);

		assert_epoch_index(GENESIS_EPOCH);
		assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);

		// Now that we have bidders, we should succeed the auction, and complete the rotation
		MockBidderProvider::set_winning_bids();

		move_forward_blocks(1);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeygensInProgress(..)
		));
		MockVaultRotatorA::keygen_success();
		// TODO: Needs to be clearer why this is 2 blocks and not 1
		move_forward_blocks(2);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeyHandoversInProgress(..)
		));
		MockVaultRotatorA::key_handover_success();

		move_forward_blocks(2);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::ActivatingKeys(..)
		));
		MockVaultRotatorA::keys_activated();
		// TODO: Needs to be clearer why this is 2 blocks and not 1
		move_forward_blocks(2);
		assert_default_rotation_outcome!();
	});
}

#[test]
fn should_be_unable_to_force_rotation_during_a_rotation() {
	new_test_ext().execute_with(|| {
		MockBidderProvider::set_winning_bids();
		ValidatorPallet::start_authority_rotation();
		assert_noop!(
			ValidatorPallet::force_rotation(RuntimeOrigin::root()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn should_rotate_when_forced() {
	new_test_ext().execute_with(|| {
		MockBidderProvider::set_winning_bids();
		assert_ok!(ValidatorPallet::force_rotation(RuntimeOrigin::root()));
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeygensInProgress(..)
		));
		assert_noop!(
			ValidatorPallet::force_rotation(RuntimeOrigin::root()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn auction_winners_should_be_the_new_authorities_on_new_epoch() {
	new_test_ext().execute_with(|| {
		let genesis_set = BTreeSet::from(GENESIS_AUTHORITIES);
		assert_eq!(
			CurrentAuthorities::<Test>::get(),
			genesis_set,
			"the current authorities should be the genesis authorities"
		);
		// Run to the epoch boundary.
		MockBidderProvider::set_winning_bids();
		run_to_block(EPOCH_DURATION);
		assert_eq!(
			ValidatorPallet::current_authorities(),
			genesis_set,
			"we should still be validating with the genesis authorities"
		);

		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeygensInProgress(..)
		));
		MockVaultRotatorA::keygen_success();
		// TODO: Needs to be clearer why this is 2 blocks and not 1
		move_forward_blocks(2);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeyHandoversInProgress(..)
		));
		MockVaultRotatorA::key_handover_success();

		move_forward_blocks(2);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::ActivatingKeys(..)
		));

		MockVaultRotatorA::keys_activated();
		// TODO: Needs to be clearer why this is 2 blocks and not 1
		move_forward_blocks(2);
		assert_default_rotation_outcome!();
	});
}

#[test]
fn genesis() {
	new_test_ext().execute_with(|| {
		assert_eq!(
			CurrentAuthorities::<Test>::get(),
			BTreeSet::from(GENESIS_AUTHORITIES),
			"We should have a set of validators at genesis"
		);
		assert_eq!(Bond::<Test>::get(), GENESIS_BOND, "We should have a minimum bid at genesis");
		assert_epoch_index(GENESIS_EPOCH);
		assert_invariants!();
	});
}

#[test]
fn send_cfe_version() {
	new_test_ext().execute_with(|| {
		// We initially submit version
		let authority = GENESIS_AUTHORITIES[0];

		let version = SemVer { major: 4, ..Default::default() };
		assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(authority), version,));

		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::CFEVersionUpdated {
				account_id: authority,
				old_version: SemVer::default(),
				new_version: version,
			}),
			"should emit event on updated version"
		);

		assert_eq!(
			version,
			ValidatorPallet::node_cfe_version(authority),
			"version should be stored"
		);

		// We submit a new version
		let new_version = SemVer { major: 5, ..Default::default() };
		assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(authority), new_version));

		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::CFEVersionUpdated {
				account_id: authority,
				old_version: version,
				new_version,
			}),
			"should emit event on updated version"
		);

		assert_eq!(
			new_version,
			ValidatorPallet::node_cfe_version(authority),
			"new version should be stored"
		);

		// When we submit the same version we should see no `CFEVersionUpdated` event
		frame_system::Pallet::<Test>::reset_events();
		assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(authority), new_version));

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

		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&ALICE).unwrap();
		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&BOB).unwrap();

		let alice_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("alice", None);
		let alice_peer_public_key = alice_peer_keypair.public();

		// Don't allow invalid signatures
		assert_noop!(
			ValidatorPallet::register_peer_id(
				RuntimeOrigin::signed(ALICE),
				alice_peer_public_key,
				0,
				0,
				alice_peer_keypair.sign(&BOB.encode()[..]),
			),
			Error::<Test>::InvalidAccountPeerMappingSignature
		);

		// Non-overlapping peer ids and valid signatures
		assert_ok!(ValidatorPallet::register_peer_id(
			RuntimeOrigin::signed(ALICE),
			alice_peer_public_key,
			40044,
			10,
			alice_peer_keypair.sign(&ALICE.encode()[..]),
		));
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::PeerIdRegistered(
				ALICE,
				alice_peer_public_key,
				40044,
				10
			)),
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&alice_peer_public_key), Some(()));
		assert_eq!(ValidatorPallet::node_peer_id(&ALICE), Some((alice_peer_public_key, 40044, 10)));

		// New mappings to overlapping peer id are disallowed
		assert_noop!(
			ValidatorPallet::register_peer_id(
				RuntimeOrigin::signed(BOB),
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
			RuntimeOrigin::signed(BOB),
			bob_peer_public_key,
			40043,
			11,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		),);
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::PeerIdRegistered(
				BOB,
				bob_peer_public_key,
				40043,
				11
			)),
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(ValidatorPallet::node_peer_id(&BOB), Some((bob_peer_public_key, 40043, 11)));

		// Changing existing mapping to overlapping peer id is disallowed
		assert_noop!(
			ValidatorPallet::register_peer_id(
				RuntimeOrigin::signed(BOB),
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
			RuntimeOrigin::signed(BOB),
			bob_peer_public_key,
			40043,
			11,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		));
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::PeerIdRegistered(
				BOB,
				bob_peer_public_key,
				40043,
				11
			)),
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(ValidatorPallet::node_peer_id(&BOB), Some((bob_peer_public_key, 40043, 11)));

		// Updating only the ip address works
		assert_ok!(ValidatorPallet::register_peer_id(
			RuntimeOrigin::signed(BOB),
			bob_peer_public_key,
			40043,
			12,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		));
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::PeerIdRegistered(
				BOB,
				bob_peer_public_key,
				40043,
				12
			)),
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(ValidatorPallet::node_peer_id(&BOB), Some((bob_peer_public_key, 40043, 12)));
	});
}

#[test]
fn rerun_auction_if_not_enough_participants() {
	new_test_ext().execute_with_unchecked_invariants(|| {
		// Unqualify one of the auction winners
		QualifyAll::<u64>::except([AUCTION_WINNERS[0]]);
		// Change the auction parameters to simulate a shortage in available candidates
		assert_ok!(ValidatorPallet::set_auction_parameters(
			RuntimeOrigin::root(),
			SetSizeParameters { min_size: 3, max_size: 3, max_expansion: 3 }
		));
		// Run to the epoch boundary
		MockBidderProvider::set_winning_bids();
		run_to_block(EPOCH_DURATION);
		// Assert that we still in the idle phase
		assert!(matches!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle));
		// Set some over node to requalify the first auction winner
		QualifyAll::<u64>::except([AUCTION_LOSERS[0]]);
		// Run to the next block - we expect and immediate retry
		run_to_block(EPOCH_DURATION + 1);
		// Expect a resolved auction and kickedoff key-gen
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeygensInProgress(..)
		));
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
		HistoricalAuthorities::<Test>::insert(1, BTreeSet::from([ALICE]));
		HistoricalBonds::<Test>::insert(1, 10);
		// Epoch 2
		EpochHistory::<Test>::activate_epoch(&ALICE, 2);
		HistoricalAuthorities::<Test>::insert(2, BTreeSet::from([ALICE]));
		HistoricalBonds::<Test>::insert(2, 30);
		// Epoch 3
		EpochHistory::<Test>::activate_epoch(&ALICE, 3);
		HistoricalAuthorities::<Test>::insert(3, BTreeSet::from([ALICE]));
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
		assert_ok!(ValidatorPallet::set_vanity_name(RuntimeOrigin::signed(validators[0]), "Test Validator 1".as_bytes().to_vec()));
		assert_ok!(ValidatorPallet::set_vanity_name(RuntimeOrigin::signed(validators[2]), "Test Validator 2".as_bytes().to_vec()));
		let vanity_names = crate::VanityNames::<Test>::get();
		assert_eq!(sp_std::str::from_utf8(vanity_names.get(&validators[0]).unwrap()).unwrap(), "Test Validator 1");
		assert_eq!(sp_std::str::from_utf8(vanity_names.get(&validators[2]).unwrap()).unwrap(), "Test Validator 2");
		assert_noop!(ValidatorPallet::set_vanity_name(RuntimeOrigin::signed(validators[0]), [0xfe, 0xff].to_vec()), crate::Error::<Test>::InvalidCharactersInName);
		assert_noop!(ValidatorPallet::set_vanity_name(RuntimeOrigin::signed(validators[0]), "Validator Name too longggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg".as_bytes().to_vec()), crate::Error::<Test>::NameTooLong);
	});
}

#[test]
fn test_missing_author_punishment() {
	new_test_ext().execute_with(|| {
		// Use a large offset to ensure the modulo math selects the correct validators.
		let offset: u64 = GENESIS_AUTHORITIES.len() as u64 * 123456;
		let (expected_authority_index, authored_authority_index) = (1usize, 3usize);
		MockMissedAuthorshipSlots::set(
			expected_authority_index as u64 + offset,
			authored_authority_index as u64 + offset,
		);
		move_forward_blocks(1);
		MockOffenceReporter::assert_reported(
			PalletOffence::MissedAuthorshipSlot,
			ValidatorPallet::current_authorities()
				.into_iter()
				.collect::<Vec<_>>()
				.get(expected_authority_index..authored_authority_index)
				.unwrap()
				.to_vec(),
		)
	})
}

#[test]
fn no_validator_rotation_when_disabled_by_safe_mode() {
	new_test_ext().execute_with(|| {
		// Activate Safe Mode: CODE RED
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		assert!(<MockRuntimeSafeMode as Get<PalletSafeMode>>::get() == PalletSafeMode::CODE_RED);

		// Try to start a rotation.
		ValidatorPallet::start_authority_rotation();
		assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);
		assert_noop!(
			ValidatorPallet::force_rotation(RawOrigin::Root.into()),
			Error::<Test>::RotationsDisabled
		);
		assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);

		// Change safe mode to CODE GREEN
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();
		assert!(<MockRuntimeSafeMode as Get<PalletSafeMode>>::get() == PalletSafeMode::CODE_GREEN);

		// Try to start a rotation.
		MockBidderProvider::set_winning_bids();
		ValidatorPallet::start_authority_rotation();
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeygensInProgress(..)
		));
	});
}

#[test]
fn rotating_during_rotation_is_noop() {
	new_test_ext().execute_with_unchecked_invariants(|| {
		MockBidderProvider::set_winning_bids();
		ValidatorPallet::force_rotation(RawOrigin::Root.into()).unwrap();
		// We attempt an auction when we force a rotation
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeygensInProgress(..)
		));

		// We don't attempt the auction again, because we're already in a rotation
		assert_noop!(
			ValidatorPallet::force_rotation(RawOrigin::Root.into()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn test_reputation_is_reset_on_expired_epoch() {
	new_test_ext().execute_with_unchecked_invariants(|| {
		assert!(!MockReputationResetter::<Test>::reputation_was_reset());

		ValidatorPallet::expire_epoch(ValidatorPallet::current_epoch());

		assert!(MockReputationResetter::<Test>::reputation_was_reset());
	});
}
#[cfg(test)]
mod bond_expiry {
	use super::*;

	#[test]
	fn increasing_bond() {
		new_test_ext().execute_with_unchecked_invariants(|| {
			let initial_epoch = ValidatorPallet::current_epoch();
			ValidatorPallet::transition_to_next_epoch(simple_rotation_state(vec![1, 2], Some(100)));
			assert_eq!(ValidatorPallet::bond(), 100);

			ValidatorPallet::transition_to_next_epoch(simple_rotation_state(vec![2, 3], Some(101)));
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
			ValidatorPallet::transition_to_next_epoch(simple_rotation_state(vec![1, 2], Some(100)));
			assert_eq!(ValidatorPallet::bond(), 100);

			ValidatorPallet::transition_to_next_epoch(simple_rotation_state(vec![2, 3], Some(99)));
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

#[test]
fn auction_params_must_be_valid_when_set() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			ValidatorPallet::set_auction_parameters(
				RuntimeOrigin::root(),
				SetSizeParameters::default()
			),
			Error::<Test>::InvalidAuctionParameters
		);

		assert_ok!(ValidatorPallet::set_auction_parameters(
			RuntimeOrigin::root(),
			SetSizeParameters { min_size: 3, max_size: 10, max_expansion: 10 }
		));
		// Confirm we have an event
		assert!(matches!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::AuctionParametersChanged(..)),
		));
	});
}

#[test]
fn test_validator_registration_min_balance() {
	new_test_ext().execute_with(|| {
		assert_ok!(Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ALICE),));
	});
}

#[test]
fn test_expect_validator_register_fails() {
	new_test_ext().execute_with(|| {
		Backups::<Test>::put(BTreeMap::from_iter([(ALICE, 100), (BOB, 80)]));
		assert_noop!(
			Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(3),),
			crate::Error::<Test>::NotEnoughFunds
		);
	});
}

#[test]
fn key_handover_should_repeat_until_below_authority_threshold() {
	fn failed_handover_with_offenders(offenders: impl IntoIterator<Item = u64>) {
		CurrentAuthorities::<Test>::set((0..10).collect());
		CurrentRotationPhase::<Test>::put(RotationPhase::KeygensInProgress(
			RuntimeRotationState::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
				winners: (4..14).collect(),
				losers: Default::default(),
				bond: Default::default(),
			}),
		));
		MockVaultRotatorA::keygen_success();
		System::reset_events();
		Pallet::<Test>::on_initialize(1);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::KeyHandoversInProgress(..)
		));
		MockVaultRotatorA::failed(offenders);
		System::reset_events();
		Pallet::<Test>::on_initialize(2);
	}

	new_test_ext().execute_with_unchecked_invariants(|| {
		// Still enough current authorities available, we should try again.
		failed_handover_with_offenders(0..3);
		assert!(
			matches!(
				CurrentRotationPhase::<Test>::get(),
				RotationPhase::KeyHandoversInProgress(..)
			),
			"Expected KeyHandoversInProgress, got {:?}",
			CurrentRotationPhase::<Test>::get(),
		);
	});
	new_test_ext().execute_with_unchecked_invariants(|| {
		// Too many current authorities banned, we abort.
		failed_handover_with_offenders(0..4);
		assert!(
			matches!(CurrentRotationPhase::<Test>::get(), RotationPhase::Idle),
			"Expected Idle, got {:?}",
			CurrentRotationPhase::<Test>::get(),
		);
		assert_event_sequence!(
			Test,
			RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
				new_phase: RotationPhase::Idle
			}),
			RuntimeEvent::ValidatorPallet(Event::RotationAborted)
		);
	});
	new_test_ext().execute_with_unchecked_invariants(|| {
		// Above the threshold, old validators, and any new validators, we abort.
		failed_handover_with_offenders(0..5);
		assert!(
			matches!(CurrentRotationPhase::<Test>::get(), RotationPhase::Idle),
			"Expected Idle, got {:?}",
			CurrentRotationPhase::<Test>::get(),
		);
		assert_event_sequence!(
			Test,
			RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
				new_phase: RotationPhase::Idle
			}),
			RuntimeEvent::ValidatorPallet(Event::RotationAborted)
		);
	});
	new_test_ext().execute_with_unchecked_invariants(|| {
		// If even one new validator fails, but all old validators were well-behaved,
		// we revert to keygen.
		failed_handover_with_offenders(4..5);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::KeygensInProgress(..)
		));
	});
}

#[test]
fn safe_mode_can_aborts_authority_rotation_before_key_handover() {
	new_test_ext().execute_with(|| {
		MockBidderProvider::set_winning_bids();
		ValidatorPallet::start_authority_rotation();
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeygensInProgress(..)
		));
		MockVaultRotatorA::keygen_success();

		System::reset_events();
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		ValidatorPallet::on_initialize(1);
		assert_event_sequence!(
			Test,
			RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
				new_phase: RotationPhase::Idle
			}),
			RuntimeEvent::ValidatorPallet(Event::RotationAborted)
		);
		assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);
	});
}

#[test]
fn safe_mode_does_not_aborts_authority_rotation_after_key_handover() {
	new_test_ext().execute_with(|| {
		MockBidderProvider::set_winning_bids();
		ValidatorPallet::start_authority_rotation();
		MockVaultRotatorA::keygen_success();
		ValidatorPallet::on_initialize(1);
		MockVaultRotatorA::key_handover_success();

		System::reset_events();
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		ValidatorPallet::on_initialize(1);
		assert_event_sequence!(
			Test,
			RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
				new_phase: RotationPhase::ActivatingKeys(..)
			}),
		);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::ActivatingKeys(..)
		));
	});
}

#[test]
fn safe_mode_does_not_aborts_authority_rotation_during_key_activation() {
	new_test_ext().execute_with(|| {
		MockBidderProvider::set_winning_bids();
		ValidatorPallet::start_authority_rotation();
		MockVaultRotatorA::keygen_success();
		ValidatorPallet::on_initialize(1);
		MockVaultRotatorA::key_handover_success();
		ValidatorPallet::on_initialize(1);
		MockVaultRotatorA::keys_activated();

		System::reset_events();
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		ValidatorPallet::on_initialize(1);
		assert_event_sequence!(
			Test,
			RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
				new_phase: RotationPhase::NewKeysActivated(..)
			}),
		);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::NewKeysActivated(..)
		));
	});
}

#[test]
fn authority_rotation_can_succeed_after_aborted_by_safe_mode() {
	new_test_ext().execute_with(|| {
		MockBidderProvider::set_winning_bids();
		// Abort authority rotation using Safe Mode.
		ValidatorPallet::start_authority_rotation();
		MockVaultRotatorA::keygen_success();
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		ValidatorPallet::on_initialize(1);
		assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);

		// Restart the authority Rotation.
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();
		ValidatorPallet::start_authority_rotation();
		ValidatorPallet::on_initialize(1);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeygensInProgress(..)
		));

		MockVaultRotatorA::keygen_success();
		ValidatorPallet::on_initialize(1);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::KeyHandoversInProgress(..)
		));

		MockVaultRotatorA::key_handover_success();
		ValidatorPallet::on_initialize(1);
		assert!(matches!(
			CurrentRotationPhase::<Test>::get(),
			RotationPhase::<Test>::ActivatingKeys(..)
		));

		MockVaultRotatorA::keys_activated();
		ValidatorPallet::on_initialize(1);
		move_forward_blocks(2);
		assert_default_rotation_outcome!();
	});
}
