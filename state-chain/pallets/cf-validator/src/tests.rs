#![cfg(test)]

use core::ops::Range;

use crate::{mock::*, Error, *};
use cf_test_utilities::{assert_event_sequence, last_event};
use cf_traits::{
	mocks::{
		funding_info::MockFundingInfo, reputation_resetter::MockReputationResetter,
		vault_rotator::MockVaultRotatorA,
	},
	AccountRoleRegistry, SafeMode, SetSafeMode,
};
use cf_utilities::success_threshold_from_share_count;
use frame_support::{assert_noop, assert_ok, traits::OriginTrait};
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

macro_rules! assert_rotation_phase_matches {
	($expected_phase: pat) => {
		assert!(
			matches!(CurrentRotationPhase::<Test>::get(), $expected_phase),
			"Expected {}, got {:?}",
			stringify!($expected_phase),
			CurrentRotationPhase::<Test>::get(),
		);
	};
}

macro_rules! assert_default_rotation_outcome {
	() => {
		assert_rotation_phase_matches!(RotationPhase::Idle);
		assert_epoch_index(GENESIS_EPOCH + 1);
		assert_eq!(Bond::<Test>::get(), EXPECTED_BOND, "bond should be updated");
		assert_eq!(ValidatorPallet::current_authorities(), BTreeSet::from(AUCTION_WINNERS));
	};
}

#[track_caller]
fn assert_rotation_aborted() {
	assert_rotation_phase_matches!(RotationPhase::Idle);
	assert_eq!(<Test as Config>::VaultRotator::status(), AsyncResult::Void);
	assert_event_sequence!(
		Test,
		RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
			new_phase: RotationPhase::Idle
		}),
		RuntimeEvent::ValidatorPallet(Event::RotationAborted)
	);
}

fn simple_rotation_state(
	auction_winners: Vec<u64>,
	bond: Option<u128>,
) -> RuntimeRotationState<Test> {
	RuntimeRotationState::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
		winners: auction_winners,
		bond: bond.unwrap_or(100),
		losers: AUCTION_LOSERS.to_vec(),
	})
}

#[test]
fn changing_epoch_block_size() {
	new_test_ext().then_execute_with_checks(|| {
		assert_noop!(
			ValidatorPallet::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::EpochDuration { blocks: 0 }
			),
			Error::<Test>::InvalidEpochDuration
		);
		const UPDATE: PalletConfigUpdate = PalletConfigUpdate::EpochDuration { blocks: 100 };
		assert_ok!(ValidatorPallet::update_pallet_config(RuntimeOrigin::root(), UPDATE));
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::PalletConfigUpdated {
				update: UPDATE
			}),
		);
	});
}

#[test]
fn should_retry_rotation_until_success_with_failing_auctions() {
	new_test_ext()
		.execute_with(|| {
			assert_eq!(MockBidderProvider::get_bidders().len(), 0);
		})
		// Move forward past the epoch boundary, the auction will be failing
		.then_advance_n_blocks_and_execute_with_checks(EPOCH_DURATION + 100, || {
			assert_epoch_index(GENESIS_EPOCH);
			assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);

			MockBidderProvider::set_default_test_bids();
		})
		// Now that we have bidders, we should succeed the auction, and complete the rotation
		.then_advance_n_blocks_and_execute_with_checks(1, || {
			assert!(matches!(
				CurrentRotationPhase::<Test>::get(),
				RotationPhase::<Test>::KeygensInProgress(..)
			));
			MockVaultRotatorA::keygen_success();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert!(matches!(
				CurrentRotationPhase::<Test>::get(),
				RotationPhase::<Test>::KeyHandoversInProgress(..)
			));
			MockVaultRotatorA::key_handover_success();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert!(matches!(
				CurrentRotationPhase::<Test>::get(),
				RotationPhase::<Test>::ActivatingKeys(..)
			));
			MockVaultRotatorA::keys_activated();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_default_rotation_outcome!();
		});
}

#[test]
fn should_be_unable_to_force_rotation_during_a_rotation() {
	new_test_ext().then_execute_with_checks(|| {
		MockBidderProvider::set_default_test_bids();
		ValidatorPallet::start_authority_rotation();
		assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		assert_noop!(
			ValidatorPallet::force_rotation(RuntimeOrigin::root()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn should_rotate_when_forced() {
	new_test_ext().then_execute_with_checks(|| {
		MockBidderProvider::set_default_test_bids();
		assert_ok!(ValidatorPallet::force_rotation(RuntimeOrigin::root()));
		assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		assert_noop!(
			ValidatorPallet::force_rotation(RuntimeOrigin::root()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn auction_winners_should_be_the_new_authorities_on_new_epoch() {
	let genesis_set = BTreeSet::from(GENESIS_AUTHORITIES);
	new_test_ext()
		.then_execute_with_checks(|| {
			assert_eq!(
				CurrentAuthorities::<Test>::get(),
				genesis_set,
				"the current authorities should be the genesis authorities"
			);
			// Run to the epoch boundary.
			MockBidderProvider::set_default_test_bids();
		})
		.then_advance_n_blocks_and_execute_with_checks(EPOCH_DURATION, || {
			assert_eq!(
				ValidatorPallet::current_authorities(),
				genesis_set,
				"we should still be validating with the genesis authorities"
			);

			assert_rotation_phase_matches!(RotationPhase::<Test>::KeygensInProgress(..));
			MockVaultRotatorA::keygen_success();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_rotation_phase_matches!(RotationPhase::<Test>::KeyHandoversInProgress(..));
			MockVaultRotatorA::key_handover_success();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_rotation_phase_matches!(RotationPhase::<Test>::ActivatingKeys(..));

			MockVaultRotatorA::keys_activated();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_default_rotation_outcome!();
		});
}

#[test]
fn genesis() {
	new_test_ext().then_execute_with_checks(|| {
		assert_eq!(
			CurrentAuthorities::<Test>::get(),
			BTreeSet::from(GENESIS_AUTHORITIES),
			"We should have a set of validators at genesis"
		);
		assert_eq!(Bond::<Test>::get(), GENESIS_BOND, "We should have a minimum bid at genesis");
		assert_epoch_index(GENESIS_EPOCH);
	});
}

#[test]
fn send_cfe_version() {
	new_test_ext().then_execute_with_checks(|| {
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
	new_test_ext().then_execute_with_checks(|| {
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
	new_test_ext()
		.execute_with(|| {
			// Unqualify one of the auction winners
			// Change the auction parameters to simulate a shortage in available candidates
			MockBidderProvider::set_default_test_bids();
			let num_bidders = <MockBidderProvider as BidderProvider>::get_bidders().len() as u32;

			assert_ok!(ValidatorPallet::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::AuctionParameters {
					parameters: SetSizeParameters {
						min_size: num_bidders + 1,
						max_size: 150,
						max_expansion: 150
					}
				}
			));
		})
		// Run to the epoch boundary
		.then_advance_n_blocks_and_execute_with_checks(EPOCH_DURATION, || {
			cf_test_utilities::assert_has_event::<Test>(RuntimeEvent::ValidatorPallet(
				Event::RotationAborted,
			));
			// Assert that we still in the idle phase
			assert_rotation_phase_matches!(RotationPhase::<Test>::Idle);
			let num_bidders = <MockBidderProvider as BidderProvider>::get_bidders().len() as u32;
			assert_ok!(ValidatorPallet::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::AuctionParameters {
					parameters: SetSizeParameters {
						min_size: num_bidders - 1,
						max_size: 150,
						max_expansion: 150
					}
				}
			));
		})
		// Run to the next block - we expect and immediate retry
		.then_advance_n_blocks_and_execute_with_checks(1, || {
			// Expect a resolved auction and kicked-off keygen
			assert_rotation_phase_matches!(RotationPhase::<Test>::KeygensInProgress(..));
		});
}

#[test]
fn historical_epochs() {
	new_test_ext().then_execute_with_checks(|| {
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
	new_test_ext().then_execute_with_checks(|| {
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
	new_test_ext().then_execute_with_checks(|| {
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
	let (expected_authority_index, authored_authority_index) = (1usize, 3usize);
	new_test_ext()
		.then_execute_with_checks(|| {
			// Use a large offset to ensure the modulo math selects the correct validators.
			let offset: u64 = GENESIS_AUTHORITIES.len() as u64 * 123456;
			MockMissedAuthorshipSlots::set(
				expected_authority_index as u64 + offset,
				authored_authority_index as u64 + offset,
			);
		})
		.then_advance_n_blocks_and_execute_with_checks(1, || {
			MockOffenceReporter::assert_reported(
				PalletOffence::MissedAuthorshipSlot,
				ValidatorPallet::current_authorities()
					.into_iter()
					.collect::<Vec<_>>()
					.get(expected_authority_index..authored_authority_index)
					.unwrap()
					.to_vec(),
			)
		});
}

#[test]
fn no_validator_rotation_when_disabled_by_safe_mode() {
	new_test_ext().then_execute_with_checks(|| {
		// Activate Safe Mode: CODE RED
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		assert!(<MockRuntimeSafeMode as Get<PalletSafeMode>>::get() == PalletSafeMode::CODE_RED);

		// Try to start a rotation.
		ValidatorPallet::start_authority_rotation();
		assert_rotation_phase_matches!(RotationPhase::Idle);
		assert_noop!(
			ValidatorPallet::force_rotation(RawOrigin::Root.into()),
			Error::<Test>::RotationsDisabled
		);
		assert_rotation_phase_matches!(RotationPhase::Idle);

		// Change safe mode to CODE GREEN
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();
		assert!(<MockRuntimeSafeMode as Get<PalletSafeMode>>::get() == PalletSafeMode::CODE_GREEN);

		// Try to start a rotation.
		MockBidderProvider::set_default_test_bids();
		ValidatorPallet::start_authority_rotation();
		assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
	});
}

#[test]
fn test_reputation_is_reset_on_expired_epoch() {
	new_test_ext().execute_with(|| {
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
		new_test_ext().execute_with(|| {
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
		new_test_ext().execute_with(|| {
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
	new_test_ext().then_execute_with_checks(|| {
		assert_noop!(
			ValidatorPallet::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::AuctionParameters { parameters: SetSizeParameters::default() }
			),
			Error::<Test>::InvalidAuctionParameters
		);

		assert_ok!(ValidatorPallet::update_pallet_config(
			RuntimeOrigin::root(),
			PalletConfigUpdate::AuctionParameters {
				parameters: SetSizeParameters { min_size: 3, max_size: 10, max_expansion: 10 }
			}
		));
		// Confirm we have an event
		assert!(matches!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(crate::Event::PalletConfigUpdated { .. }),
		));
	});
}

#[test]
fn test_validator_registration_min_balance() {
	new_test_ext().then_execute_with_checks(|| {
		assert_ok!(Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ALICE),));
	});
}

#[test]
fn test_expect_validator_register_fails() {
	new_test_ext().then_execute_with_checks(|| {
		const ID: u64 = 42;
		assert_ok!(ValidatorPallet::update_pallet_config(
			RawOrigin::Root.into(),
			PalletConfigUpdate::RegistrationBondPercentage {
				percentage: Percent::from_percent(60),
			},
		));
		MockFundingInfo::<Test>::credit_funds(&ID, Percent::from_percent(40) * GENESIS_BOND);
		// Reduce the set size target to the current authority count.
		assert_ok!(Pallet::<Test>::update_pallet_config(
			RawOrigin::Root.into(),
			PalletConfigUpdate::AuctionParameters {
				parameters: SetSizeParameters {
					min_size: MIN_AUTHORITY_SIZE,
					max_size: <Pallet<Test> as EpochInfo>::current_authority_count(),
					max_expansion: MAX_AUTHORITY_SET_EXPANSION,
				},
			},
		));
		assert_noop!(
			Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ID),),
			crate::Error::<Test>::NotEnoughFunds
		);
		// Now set it back to the default.
		assert_ok!(Pallet::<Test>::update_pallet_config(
			RawOrigin::Root.into(),
			PalletConfigUpdate::AuctionParameters {
				parameters: SetSizeParameters {
					min_size: MIN_AUTHORITY_SIZE,
					max_size: MAX_AUTHORITY_SIZE,
					max_expansion: MAX_AUTHORITY_SET_EXPANSION,
				},
			},
		));
		// It should be possible to register now since the actual size is below the target.
		assert_ok!(Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ID)));
		MockFundingInfo::<Test>::credit_funds(&ID, Percent::from_percent(20) * GENESIS_BOND);
		// Trying to register again passes the funding check but fails for other reasons.
		assert_noop!(
			Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ID)),
			DispatchError::Other("Account already registered")
		);
	});
}

const CANDIDATES: Range<u64> = 4..14;
const AUTHORITIES: Range<u64> = 0..10;

lazy_static::lazy_static! {
	/// How many candidates can fail without preventing us from re-trying keygen
	static ref MAX_ALLOWED_KEYGEN_OFFENDERS: usize = CANDIDATES.count().checked_sub(MIN_AUTHORITY_SIZE as usize).unwrap();

	/// How many current authorities can fail to leave enough healthy ones to handover the key
	static ref MAX_ALLOWED_SHARING_OFFENDERS: usize = {
		let total = AUTHORITIES.count();
		let needed = success_threshold_from_share_count(total as u32);
		total.checked_sub(needed as usize).unwrap()
	};
}

fn failed_keygen_with_offenders(offenders: impl IntoIterator<Item = u64>) {
	CurrentAuthorities::<Test>::set(AUTHORITIES.collect());
	CurrentRotationPhase::<Test>::put(RotationPhase::KeygensInProgress(
		RuntimeRotationState::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
			winners: CANDIDATES.collect(),
			losers: Default::default(),
			bond: Default::default(),
		}),
	));

	MockVaultRotatorA::failed(offenders);
	Pallet::<Test>::on_initialize(1);
}
#[cfg(test)]
mod keygen {

	use super::*;

	#[test]
	fn restarts_from_keygen_on_keygen_failure() {
		new_test_ext().execute_with(|| {
			// just one node failed
			failed_keygen_with_offenders(CANDIDATES.take(1));
			assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		});

		new_test_ext().execute_with(|| {
			// many nodes failed, but enough left to try to restart keygen
			failed_keygen_with_offenders(CANDIDATES.take(*MAX_ALLOWED_KEYGEN_OFFENDERS));
			assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		});
	}

	#[test]
	fn abort_on_keygen_failure_if_too_many_banned() {
		new_test_ext().execute_with(|| {
			// Not enough unbanned nodes left after this failure, so we should abort
			failed_keygen_with_offenders(CANDIDATES.take(*MAX_ALLOWED_KEYGEN_OFFENDERS + 1));
			assert_rotation_aborted();
		});
	}
}

#[cfg(test)]
mod key_handover {

	use super::*;

	fn failed_handover_with_offenders(offenders: impl IntoIterator<Item = u64>) {
		CurrentAuthorities::<Test>::set(AUTHORITIES.collect());
		CurrentRotationPhase::<Test>::put(RotationPhase::KeygensInProgress(
			RuntimeRotationState::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
				winners: CANDIDATES.collect(),
				losers: Default::default(),
				bond: Default::default(),
			}),
		));
		MockVaultRotatorA::keygen_success();
		System::reset_events();
		Pallet::<Test>::on_initialize(1);

		assert_rotation_phase_matches!(RotationPhase::KeyHandoversInProgress(..));
		MockVaultRotatorA::failed(offenders);
		System::reset_events();
		Pallet::<Test>::on_initialize(2);
	}

	#[test]
	fn banned_nodes_persist() {
		let non_candidates = AUTHORITIES
			.collect::<BTreeSet<_>>()
			.difference(&CANDIDATES.collect())
			.copied()
			.collect::<Vec<_>>();

		let fails_keygen = non_candidates[0];
		let fails_handover = non_candidates[1];

		new_test_ext()
			.then_execute_at_next_block(|_| {
				// Failed keygen should restart (should have enough non-banned nodes)
				failed_keygen_with_offenders([fails_keygen]);
			})
			.then_execute_at_next_block(|_| {
				assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
			})
			.then_execute_at_next_block(|_| {
				// Successful keygen should transition to handover
				MockVaultRotatorA::keygen_success();
			})
			.then_execute_at_next_block(|_| {
				assert_rotation_phase_matches!(RotationPhase::KeyHandoversInProgress(..));
			})
			.then_execute_at_next_block(|_| {
				// Handover fails with a different non-candidate and will be retried
				MockVaultRotatorA::failed([fails_handover]);
			})
			.then_execute_at_next_block(|_| {
				// Ensure that banned nodes banned during either keygen or handover aren't selected
				if let RotationPhase::KeyHandoversInProgress(state) =
					CurrentRotationPhase::<Test>::get()
				{
					assert_eq!(
						state
							.authority_candidates()
							.intersection(&BTreeSet::from([fails_keygen, fails_handover]))
							.count(),
						0,
						"banned nodes should have been selected"
					)
				} else {
					panic!("unexpected rotation phase: {:?}", CurrentRotationPhase::<Test>::get());
				}
			});
	}

	#[test]
	fn restarts_if_non_candidates_fail() {
		new_test_ext().execute_with(|| {
			// Still enough current authorities available, we should try again.
			failed_handover_with_offenders(AUTHORITIES.take(*MAX_ALLOWED_SHARING_OFFENDERS));

			assert_rotation_phase_matches!(RotationPhase::KeyHandoversInProgress(..));
		});
	}

	#[test]
	fn abort_if_too_many_current_authorities_fail() {
		// TODO: should unban and keep trying instead (see PRO-786)
		new_test_ext().execute_with(|| {
			// Too many current authorities banned, we abort.
			failed_handover_with_offenders(AUTHORITIES.take(*MAX_ALLOWED_SHARING_OFFENDERS + 1));
			assert_rotation_aborted();
		});
	}

	#[test]
	fn restart_from_keygen_if_many_authorities_including_candidates_fail() {
		new_test_ext().execute_with(|| {
			// What matters is that at least one of the candidate fails,
			// so any other offenders don't change the outcome: reverting
			// to keygen.
			let offenders =
				CANDIDATES.take(1).chain(AUTHORITIES.take(*MAX_ALLOWED_SHARING_OFFENDERS + 1));
			failed_handover_with_offenders(offenders);
			assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		});
	}

	#[test]
	fn restart_from_keygen_if_a_single_candidate_fails() {
		new_test_ext().execute_with(|| {
			// If even one new validator fails, but all old validators were well-behaved,
			// we revert to keygen.
			failed_handover_with_offenders(CANDIDATES.take(1));
			assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		});
	}
}

#[test]
fn safe_mode_can_aborts_authority_rotation_before_key_handover() {
	new_test_ext().then_execute_with_checks(|| {
		MockBidderProvider::set_default_test_bids();
		ValidatorPallet::start_authority_rotation();

		assert_rotation_phase_matches!(RotationPhase::<Test>::KeygensInProgress(..));

		MockVaultRotatorA::keygen_success();

		System::reset_events();
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		ValidatorPallet::on_initialize(1);
		assert_rotation_aborted();
	});
}

#[test]
fn safe_mode_does_not_aborts_authority_rotation_after_key_handover() {
	new_test_ext().then_execute_with_checks(|| {
		MockBidderProvider::set_default_test_bids();
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

		assert_rotation_phase_matches!(RotationPhase::ActivatingKeys(..));
	});
}

#[test]
fn safe_mode_does_not_aborts_authority_rotation_during_key_activation() {
	new_test_ext().then_execute_with_checks(|| {
		MockBidderProvider::set_default_test_bids();
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
		assert_rotation_phase_matches!(RotationPhase::NewKeysActivated(..));
	});
}

#[test]
fn authority_rotation_can_succeed_after_aborted_by_safe_mode() {
	new_test_ext()
		.then_execute_with_checks(|| {
			MockBidderProvider::set_default_test_bids();
			// Abort authority rotation using Safe Mode.
			ValidatorPallet::start_authority_rotation();
			MockVaultRotatorA::keygen_success();
			<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		})
		.then_execute_at_next_block(|_| {
			assert_rotation_phase_matches!(RotationPhase::<Test>::Idle);

			// Restart the authority Rotation.
			<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();
			ValidatorPallet::start_authority_rotation();
		})
		.then_execute_at_next_block(|_| {
			assert_rotation_phase_matches!(RotationPhase::<Test>::KeygensInProgress(..));

			MockVaultRotatorA::keygen_success();
		})
		.then_execute_at_next_block(|_| {
			assert_rotation_phase_matches!(RotationPhase::<Test>::KeyHandoversInProgress(..));

			MockVaultRotatorA::key_handover_success();
		})
		.then_execute_at_next_block(|_| {
			assert_rotation_phase_matches!(RotationPhase::<Test>::ActivatingKeys(..));

			MockVaultRotatorA::keys_activated();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_default_rotation_outcome!();
		});
}

#[test]
fn can_calculate_percentage_cfe_at_target_version() {
	new_test_ext().execute_with(|| {
		let initial_version = SemVer { major: 5, minor: 0, patch: 0 };
		let next_version = SemVer { major: 6, minor: 0, patch: 0 };

		// We initially submit version
		let authorities = [0u64, 1u64, 2u64, 3u64, 4u64, 5u64, 6u64, 7u64, 8u64, 9u64];
		authorities.iter().for_each(|id| {
			let _ = ValidatorPallet::register_as_validator(RuntimeOrigin::signed(*id));
			assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(*id), initial_version,));
		});
		CurrentAuthorities::<Test>::set(BTreeSet::from(authorities));

		assert_eq!(
			ValidatorPallet::precent_authorities_at_version(initial_version),
			Percent::from_percent(100)
		);
		assert_eq!(
			ValidatorPallet::precent_authorities_at_version(next_version),
			Percent::from_percent(0)
		);

		// Update some authorities' version
		let authorities = [0u64, 1u64, 2u64, 3u64, 4u64, 5u64];
		authorities.iter().for_each(|id| {
			assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(*id), next_version,));
		});
		assert_eq!(
			ValidatorPallet::precent_authorities_at_version(initial_version),
			Percent::from_percent(40)
		);
		assert_eq!(
			ValidatorPallet::precent_authorities_at_version(next_version),
			Percent::from_percent(60)
		);

		// Change authorities
		CurrentAuthorities::<Test>::set(BTreeSet::from(authorities));
		assert_eq!(
			ValidatorPallet::precent_authorities_at_version(initial_version),
			Percent::from_percent(0)
		);
		assert_eq!(
			ValidatorPallet::precent_authorities_at_version(next_version),
			Percent::from_percent(100)
		);

		// Version checking ignores `patch`.
		let compatible_version = SemVer { major: 6, minor: 0, patch: 6 };
		assert_eq!(
			ValidatorPallet::precent_authorities_at_version(compatible_version),
			Percent::from_percent(100)
		);
	});
}

#[test]
fn qualification_by_cfe_version() {
	new_test_ext().execute_with(|| {
		const VALIDATOR: u64 = GENESIS_AUTHORITIES[0];
		// No value reported, no value set:
		assert!(!NodeCFEVersion::<Test>::contains_key(VALIDATOR));
		assert!(!MinimumReportedCfeVersion::<Test>::exists());
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		assert_ok!(ValidatorPallet::update_pallet_config(
			OriginTrait::root(),
			PalletConfigUpdate::MinimumReportedCfeVersion {
				version: SemVer { major: 0, minor: 1, patch: 0 }
			}
		));
		assert!(!QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version below the minimum:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 0, minor: 0, patch: 1 }
		));
		assert!(!QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version equal to the minimum:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 0, minor: 1, patch: 0 }
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version greater than the minimum:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 0, minor: 1, patch: 1 }
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version bumping the minor version:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 0, minor: 2, patch: 0 }
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version bumping the major version:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 1, minor: 0, patch: 0 }
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Raise the minimum:

		assert_ok!(ValidatorPallet::update_pallet_config(
			OriginTrait::root(),
			PalletConfigUpdate::MinimumReportedCfeVersion {
				version: SemVer { major: 1, minor: 0, patch: 0 }
			}
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Raise the minimum again:
		assert_ok!(ValidatorPallet::update_pallet_config(
			OriginTrait::root(),
			PalletConfigUpdate::MinimumReportedCfeVersion {
				version: SemVer { major: 1, minor: 0, patch: 1 }
			}
		));
		assert!(!QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));
	});
}
