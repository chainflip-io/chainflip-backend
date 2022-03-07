use crate::{
	mock::*, BlockHeightWindow, Error, Event as PalletEvent, KeygenOutcome,
	KeygenResolutionPendingSince, PendingVaultRotation, Vault, VaultRotationStatus, Vaults,
};
use cf_chains::eth::AggKey;
use cf_traits::{Chainflip, EpochInfo};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_std::{collections::btree_set::BTreeSet, iter::FromIterator};

fn last_event() -> Event {
	frame_system::Pallet::<MockRuntime>::events()
		.pop()
		.expect("Event expected")
		.event
}

const ALL_CANDIDATES: &[<MockRuntime as Chainflip>::ValidatorId] = &[ALICE, BOB, CHARLIE];

#[test]
fn no_candidates_is_noop_and_error() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			VaultsPallet::start_vault_rotation(vec![]),
			Error::<MockRuntime, _>::EmptyValidatorSet
		);
		assert!(VaultsPallet::no_active_chain_vault_rotations());
	});
}

#[test]
fn keygen_request_emitted() {
	new_test_ext().execute_with(|| {
		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		// Confirm we have a new vault rotation process running
		assert!(!VaultsPallet::no_active_chain_vault_rotations());
		// Check the event emitted
		assert_eq!(
			last_event(),
			PalletEvent::<MockRuntime, _>::KeygenRequest(
				VaultsPallet::ceremony_id_counter(),
				ALL_CANDIDATES.to_vec(),
			)
			.into()
		);
	});
}

#[test]
fn only_one_concurrent_request_per_chain() {
	new_test_ext().execute_with(|| {
		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		assert_noop!(
			VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()),
			Error::<MockRuntime, _>::DuplicateRotationRequest
		);
	});
}

#[test]
fn keygen_success() {
	new_test_ext().execute_with(|| {
		let new_agg_key =
			AggKey::from_pubkey_compressed(GENESIS_ETHEREUM_AGG_PUB_KEY.map(|x| x + 1));

		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = VaultsPallet::ceremony_id_counter();

		VaultsPallet::on_keygen_success(ceremony_id, new_agg_key);

		assert_matches!(
			PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
			VaultRotationStatus::<MockRuntime, _>::AwaitingRotation { new_public_key: k } if k == new_agg_key
		);
	});
}

#[test]
fn keygen_failure() {
	new_test_ext().execute_with(|| {
		const BAD_CANDIDATES: &[<MockRuntime as Chainflip>::ValidatorId] = &[BOB, CHARLIE];

		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));

		let ceremony_id = VaultsPallet::ceremony_id_counter();

		// The ceremony failed.
		VaultsPallet::on_keygen_failure(ceremony_id, BAD_CANDIDATES.to_vec());

		// KeygenAborted event emitted.
		assert_eq!(last_event(), PalletEvent::KeygenFailure(ceremony_id).into());

		// All rotations have been aborted.
		assert!(VaultsPallet::no_active_chain_vault_rotations());

		// Bad validators have been reported.
		assert_eq!(MockOfflineReporter::get_reported(), BAD_CANDIDATES);
	});
}

#[test]
fn no_active_rotation() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				1,
				KeygenOutcome::Success(AggKey::from_pubkey_compressed([0xcf; 33]))
			),
			Error::<MockRuntime, _>::NoActiveRotation
		);

		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				1,
				KeygenOutcome::Failure(Default::default())
			),
			Error::<MockRuntime, _>::NoActiveRotation
		);
	})
}

#[test]
fn keygen_report_success() {
	new_test_ext().execute_with(|| {
		let new_agg_key =
			AggKey::from_pubkey_compressed(GENESIS_ETHEREUM_AGG_PUB_KEY.map(|x| x + 1));

		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = VaultsPallet::ceremony_id_counter();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(ALICE),
			ceremony_id,
			KeygenOutcome::Success(new_agg_key)
		));

		// Can't report twice.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id,
				KeygenOutcome::Success(new_agg_key)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);

		// Can't change our mind
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id,
				KeygenOutcome::Failure(BTreeSet::from_iter([BOB, CHARLIE]))
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);

		// Only participants can respond.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(u64::MAX),
				ceremony_id,
				KeygenOutcome::Success(new_agg_key)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);

		// Wrong ceremony_id.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id + 1,
				KeygenOutcome::Success(new_agg_key)
			),
			Error::<MockRuntime, _>::InvalidCeremonyId
		);

		// A resolution is still pending but no consensus is reached.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());

		// Bob agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(BOB),
			ceremony_id,
			KeygenOutcome::Success(new_agg_key)
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(1);
		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());

		assert_matches!(
			PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
			VaultRotationStatus::<MockRuntime, _>::AwaitingRotation { new_public_key: k } if k == new_agg_key
		);
	})
}

#[test]
fn keygen_report_failure() {
	new_test_ext().execute_with(|| {
		let new_agg_key =
			AggKey::from_pubkey_compressed(GENESIS_ETHEREUM_AGG_PUB_KEY.map(|x| x + 1));

		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = VaultsPallet::ceremony_id_counter();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(ALICE),
			ceremony_id,
			KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
		));

		// Can't report twice.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id,
				KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);

		// Can't change our mind
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id,
				KeygenOutcome::Success(new_agg_key)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);

		// Only participants can respond.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(u64::MAX),
				ceremony_id,
				KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);

		// Wrong ceremony_id.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id + 1,
				KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
			),
			Error::<MockRuntime, _>::InvalidCeremonyId
		);

		// A resolution is still pending but no consensus is reached.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());

		// Bob agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(BOB),
			ceremony_id,
			KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(1);
		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());

		assert_eq!(MockOfflineReporter::get_reported(), vec![CHARLIE]);
	})
}

#[test]
fn test_grace_period() {
	new_test_ext().execute_with(|| {
		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = VaultsPallet::ceremony_id_counter();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(ALICE),
			ceremony_id,
			KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
		));

		// > 25 blocks later we should resolve an error.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(25);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(26);
		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());

		// All non-responding candidates should have been reported.
		assert_eq!(MockOfflineReporter::get_reported(), vec![BOB, CHARLIE]);
	});
}

#[test]
fn vault_key_rotated() {
	new_test_ext().execute_with(|| {
		const ROTATION_BLOCK_NUMBER: u64 = 42;
		const TX_HASH: [u8; 32] = [0xab; 32];
		let new_agg_key =
			AggKey::from_pubkey_compressed(GENESIS_ETHEREUM_AGG_PUB_KEY.map(|x| x + 1));

		assert_noop!(
			VaultsPallet::vault_key_rotated(
				Origin::root(),
				new_agg_key,
				ROTATION_BLOCK_NUMBER,
				TX_HASH.into(),
			),
			Error::<MockRuntime, _>::NoActiveRotation
		);

		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = VaultsPallet::ceremony_id_counter();
		VaultsPallet::on_keygen_success(ceremony_id, new_agg_key);

		assert_ok!(VaultsPallet::vault_key_rotated(
			Origin::root(),
			new_agg_key,
			ROTATION_BLOCK_NUMBER,
			TX_HASH.into(),
		));

		// Can't repeat.
		assert_noop!(
			VaultsPallet::vault_key_rotated(
				Origin::root(),
				new_agg_key,
				ROTATION_BLOCK_NUMBER,
				TX_HASH.into(),
			),
			Error::<MockRuntime, _>::InvalidRotationStatus
		);

		// We have yet to move to the new epoch
		let current_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index();

		let Vault { public_key, active_window } =
			Vaults::<MockRuntime, _>::get(current_epoch).expect("Ethereum Vault should exist");

		assert_eq!(
			public_key,
			AggKey::from_pubkey_compressed(GENESIS_ETHEREUM_AGG_PUB_KEY),
			"we should have the old agg key in the genesis vault"
		);

		assert_eq!(
			active_window,
			BlockHeightWindow { from: 0, to: Some(ROTATION_BLOCK_NUMBER) },
			"we should have the block height set for the genesis or current epoch"
		);

		// The next epoch
		let next_epoch = current_epoch + 1;

		let Vault { public_key, active_window } = Vaults::<MockRuntime, _>::get(next_epoch)
			.expect("Ethereum Vault should exist in the next epoch");

		assert_eq!(
			public_key, new_agg_key,
			"we should have the new public key in the new vault for the next epoch"
		);

		assert_eq!(
			active_window,
			BlockHeightWindow { from: ROTATION_BLOCK_NUMBER.saturating_add(1), to: None },
			"we should have set the starting point for the new vault's active window as the next
			after the reported block number"
		);

		// Status is complete.
		assert_eq!(
			PendingVaultRotation::<MockRuntime, _>::get(),
			Some(VaultRotationStatus::Complete { tx_hash: TX_HASH.into() }),
		);
	});
}

mod keygen_reporting {
	use super::*;
	use crate::{KeygenOutcome, KeygenResponseStatus};
	use frame_support::assert_err;
	use sp_std::{collections::btree_set::BTreeSet, iter::FromIterator};

	const TEST_KEY: &[u8; 33] = &[0xcf; 33];

	macro_rules! assert_ok_no_repeat {
		($ex:expr) => {
			assert_ok!($ex);
			assert_err!($ex, Error::<MockRuntime, _>::InvalidRespondent);
		};
	}

	macro_rules! assert_success_outcome {
		($ex:expr) => {
			let outcome: Option<KeygenOutcome<AggKey, u64>> = $ex;
			assert!(
				matches!(outcome, Some(KeygenOutcome::Success(_))),
				"Expected success, got: {:?}",
				outcome
			);
		};
	}

	macro_rules! assert_failure_outcome {
		($ex:expr) => {
			let outcome: Option<KeygenOutcome<AggKey, u64>> = $ex;
			assert!(
				matches!(outcome, Some(KeygenOutcome::Failure(_))),
				"Expected failure, got: {:?}",
				outcome
			);
		};
	}

	macro_rules! assert_no_outcome {
		($ex:expr) => {
			let outcome: Option<KeygenOutcome<AggKey, u64>> = $ex;
			assert!(matches!(outcome, None), "Expected `None`, got: {:?}", outcome);
		};
	}

	#[test]
	fn test_threshold() {
		// The success threshold is the smallest number of participants that *can* reach consensus.
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..144))
				.success_threshold(),
			96
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..145))
				.success_threshold(),
			97
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..146))
				.success_threshold(),
			98
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..147))
				.success_threshold(),
			98
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..148))
				.success_threshold(),
			99
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..149))
				.success_threshold(),
			100
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..150))
				.success_threshold(),
			100
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..151))
				.success_threshold(),
			101
		);
	}

	fn simple_success(
		num_candidates: u32,
		num_successes: u32,
	) -> Option<KeygenOutcome<AggKey, u64>> {
		get_outcome_simple(num_candidates, num_successes, 0, 0)
	}

	fn simple_failure(
		num_candidates: u32,
		num_failures: u32,
	) -> Option<KeygenOutcome<AggKey, u64>> {
		get_outcome_simple(num_candidates, 0, num_failures, 0)
	}

	fn get_outcome_simple(
		num_candidates: u32,
		num_successes: u32,
		num_failures: u32,
		num_bad_keys: u32,
	) -> Option<KeygenOutcome<AggKey, u64>> {
		get_outcome(num_candidates, num_successes, num_failures, num_bad_keys, |_| [1])
	}

	/// Generate a report given:
	///   - the total number of candidates
	///   - the total number of success reports
	///   - the total number of failure reports
	///   - the total number of false success reports
	///   - a generator function `id -> [id]` for determining the blamed validators `[id]` for
	///     validator `id`
	fn get_outcome<F: Fn(u64) -> I, I: IntoIterator<Item = u64>>(
		num_candidates: u32,
		mut num_successes: u32,
		mut num_failures: u32,
		mut num_bad_keys: u32,
		report_gen: F,
	) -> Option<KeygenOutcome<AggKey, u64>> {
		let mut status = KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(
			1..=(num_candidates as u64),
		));

		let num_responses = num_successes + num_failures + num_bad_keys;
		assert!(
			num_responses <= num_candidates,
			"Can't have more responses than candidates: {} + {} + {} > {}.",
			num_successes,
			num_failures,
			num_bad_keys,
			num_candidates
		);

		for id in 1..=(num_responses as u64) {
			if num_successes > 0 {
				assert_ok_no_repeat!(
					status.add_success_vote(&id, AggKey::from_pubkey_compressed(*TEST_KEY))
				);
				num_successes -= 1;
			} else if num_bad_keys > 0 {
				assert_ok_no_repeat!(
					status.add_success_vote(&id, AggKey::from_pubkey_compressed([0xb0; 33]))
				);
				num_bad_keys -= 1;
			} else if num_failures > 0 {
				assert_ok_no_repeat!(
					status.add_failure_vote(&id, BTreeSet::from_iter(report_gen(id)))
				);
				num_failures -= 1;
			} else {
				panic!("Should not reach here.")
			}
		}
		status.consensus_outcome()
	}

	#[test]
	fn test_success_consensus() {
		// Simple happy-path cases.
		assert_success_outcome!(simple_success(6, 6));
		assert_success_outcome!(simple_success(6, 5));
		assert_success_outcome!(simple_success(6, 4));
		assert_success_outcome!(simple_success(7, 7));
		assert_success_outcome!(simple_success(8, 8));
		assert_success_outcome!(simple_success(9, 9));

		assert_success_outcome!(simple_success(147, 147));
		assert_success_outcome!(simple_success(148, 148));
		assert_success_outcome!(simple_success(149, 149));
		assert_success_outcome!(simple_success(150, 150));
		assert_success_outcome!(simple_success(151, 151));

		// Minority dissent has no effect.
		assert_success_outcome!(get_outcome_simple(6, 5, 1, 0));
		assert_success_outcome!(get_outcome_simple(6, 4, 1, 1));
		assert_success_outcome!(get_outcome_simple(6, 4, 2, 0));
	}

	#[test]
	fn test_failure_consensus() {
		// Simple happy-path cases.
		assert_failure_outcome!(simple_failure(6, 6));
		assert_failure_outcome!(simple_failure(6, 5));
		assert_failure_outcome!(simple_failure(6, 4));
		assert_failure_outcome!(simple_failure(7, 7));
		assert_failure_outcome!(simple_failure(8, 8));
		assert_failure_outcome!(simple_failure(9, 9));

		assert_failure_outcome!(simple_failure(147, 147));
		assert_failure_outcome!(simple_failure(148, 148));
		assert_failure_outcome!(simple_failure(149, 149));
		assert_failure_outcome!(simple_failure(150, 150));
		assert_failure_outcome!(simple_failure(151, 151));

		// Minority dissent has no effect.
		assert_failure_outcome!(get_outcome_simple(6, 2, 4, 0));
		assert_failure_outcome!(get_outcome_simple(6, 1, 4, 1));
		assert_failure_outcome!(get_outcome_simple(6, 1, 5, 0));
		assert_failure_outcome!(get_outcome_simple(6, 0, 6, 0));
	}

	#[test]
	fn test_no_consensus() {
		// No outcome until there is threshold agreement.
		assert_no_outcome!(get_outcome_simple(6, 1, 0, 0));
		assert_no_outcome!(get_outcome_simple(6, 2, 0, 0));
		assert_no_outcome!(get_outcome_simple(6, 3, 0, 0));
		assert_no_outcome!(get_outcome_simple(6, 3, 0, 1));
		assert_success_outcome!(get_outcome_simple(6, 4, 0, 0));
		assert_success_outcome!(get_outcome_simple(6, 4, 0, 1));
		assert_success_outcome!(get_outcome_simple(6, 6, 0, 0));

		assert_no_outcome!(get_outcome_simple(6, 0, 1, 0));
		assert_no_outcome!(get_outcome_simple(6, 0, 2, 0));
		assert_no_outcome!(get_outcome_simple(6, 0, 3, 0));
		assert_no_outcome!(get_outcome_simple(6, 1, 3, 0));
		assert_failure_outcome!(get_outcome_simple(6, 1, 4, 0));
		assert_failure_outcome!(get_outcome_simple(6, 0, 4, 0));
		assert_failure_outcome!(get_outcome_simple(6, 0, 5, 0));
		assert_failure_outcome!(get_outcome_simple(6, 0, 6, 0));

		// Failure if there is no other option (ie. deadlock).
		assert_no_outcome!(get_outcome_simple(6, 3, 0, 2));
		assert_no_outcome!(get_outcome_simple(6, 3, 2, 0));
		assert_no_outcome!(get_outcome_simple(6, 3, 1, 1));
		assert_no_outcome!(get_outcome_simple(6, 3, 1, 1));
		assert_no_outcome!(get_outcome_simple(6, 2, 3, 0));

		// Failure if we reach full response count with no consensus.
		assert_failure_outcome!(get_outcome_simple(6, 3, 0, 3));
		assert_failure_outcome!(get_outcome_simple(6, 3, 1, 2));
		assert_failure_outcome!(get_outcome_simple(6, 3, 2, 1));
		assert_failure_outcome!(get_outcome_simple(6, 2, 3, 1));
		assert_failure_outcome!(get_outcome_simple(6, 3, 3, 0));
		assert_failure_outcome!(get_outcome_simple(6, 2, 3, 1));

		// A keygen where more than `threshold` nodes have reported failure, but there is no final
		// agreement on the guilty parties.
		assert_no_outcome!(get_outcome(25, 0, 17, 0, |id| if id < 10 { 17..25 } else { 20..25 }));
	}

	#[test]
	fn test_blaming_aggregation() {
		// First five candidates all report candidate 6.
		let outcome = get_outcome(6, 0, 5, 1, |_| [6]);
		assert!(
			matches!(outcome.unwrap(), KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([6]))
		);

		// Candidates don't agree.
		let outcome = get_outcome(6, 0, 5, 1, |id| [id + 1]);
		assert!(matches!(outcome.unwrap(), KeygenOutcome::Failure(blamed) if blamed.is_empty()));

		// Candidates agree but not enough to report.
		let outcome = get_outcome(6, 0, 5, 1, |id| if id < 4 { [6] } else { [id + 1] });
		assert!(matches!(outcome.unwrap(), KeygenOutcome::Failure(blamed) if blamed.is_empty()));

		// Candidates agree on one but not all.
		let outcome = get_outcome(6, 0, 5, 1, |id| if id < 5 { [6] } else { [id + 1] });
		assert!(
			matches!(outcome.unwrap(), KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([6]))
		);

		// Candidates agree on multiple offenders.
		let outcome = get_outcome(12, 0, 12, 0, |id| if id < 9 { [11, 12] } else { [1, 2] });
		assert!(
			matches!(outcome.unwrap(), KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([11, 12]))
		);

		// Overlapping agreement.
		let outcome = get_outcome(12, 0, 12, 0, |id| {
			if id < 5 {
				[11, 12]
			} else if id < 9 {
				[1, 11]
			} else {
				[1, 2]
			}
		});
		assert!(
			matches!(outcome.unwrap(), KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([1, 11]))
		);
	}
}
