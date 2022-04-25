use crate::{
	mock::*, BlockHeightWindow, CeremonyId, Error, Event as PalletEvent, FailureVoters,
	KeygenOutcome, KeygenResolutionPendingSince, PalletOffence, PendingVaultRotation,
	SuccessVoters, Vault, VaultRotationStatus, Vaults,
};
use cf_traits::{
	mocks::ceremony_id_provider::MockCeremonyIdProvider, AsyncResult, Chainflip, EpochInfo,
	SuccessOrFailure, VaultRotator,
};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_std::{collections::btree_set::BTreeSet, iter::FromIterator};

fn last_event() -> Event {
	frame_system::Pallet::<MockRuntime>::events()
		.pop()
		.expect("Event expected")
		.event
}

macro_rules! assert_last_event {
	($pat:pat) => {
		let event = last_event();
		assert!(matches!(last_event(), Event::VaultsPallet($pat)), "Unexpected event {:?}", event);
	};
}

fn current_ceremony_id() -> CeremonyId {
	MockCeremonyIdProvider::<u64>::get()
}

const ALL_CANDIDATES: &[<MockRuntime as Chainflip>::ValidatorId] = &[ALICE, BOB, CHARLIE];

#[test]
fn no_candidates_is_noop_and_error() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			<VaultsPallet as VaultRotator>::start_vault_rotation(vec![]),
			Error::<MockRuntime, _>::EmptyValidatorSet
		);
	});
}

#[test]
fn keygen_request_emitted() {
	new_test_ext().execute_with(|| {
		assert_ok!(<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		// Confirm we have a new vault rotation process running
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);
		// Check the event emitted
		assert_eq!(
			last_event(),
			PalletEvent::<MockRuntime, _>::KeygenRequest(
				current_ceremony_id(),
				ALL_CANDIDATES.to_vec(),
			)
			.into()
		);
	});
}

#[test]
fn only_one_concurrent_request_per_chain() {
	new_test_ext().execute_with(|| {
		assert_ok!(<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		assert_noop!(
			<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()),
			Error::<MockRuntime, _>::DuplicateRotationRequest
		);
	});
}

#[test]
fn keygen_success() {
	new_test_ext().execute_with(|| {
		assert_ok!(<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = current_ceremony_id();

		VaultsPallet::on_keygen_success(ceremony_id, NEW_AGG_PUB_KEY);

		assert_matches!(
			PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
			VaultRotationStatus::<MockRuntime, _>::AwaitingRotation { new_public_key: k } if k == NEW_AGG_PUB_KEY
		);
	});
}

#[test]
fn keygen_failure() {
	new_test_ext().execute_with(|| {
		const BAD_CANDIDATES: &[<MockRuntime as Chainflip>::ValidatorId] = &[BOB, CHARLIE];

		assert_ok!(<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()));

		let ceremony_id = current_ceremony_id();

		// The ceremony failed.
		VaultsPallet::on_keygen_failure(ceremony_id, BAD_CANDIDATES);

		// KeygenAborted event emitted.
		assert_eq!(last_event(), PalletEvent::KeygenFailure(ceremony_id).into());

		// Outcome is ready.
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(SuccessOrFailure::Failure)
		);

		// Too many bad validators, they have not been reported.
		MockOffenceReporter::assert_reported(
			PalletOffence::ParticipateKeygenFailed,
			BAD_CANDIDATES.into_iter().cloned(),
		);
		MockOffenceReporter::assert_reported(
			PalletOffence::SigningOffence,
			BAD_CANDIDATES.into_iter().cloned(),
		);
	});
}

#[test]
fn no_active_rotation() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				1,
				KeygenOutcome::Success(NEW_AGG_PUB_KEY)
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
		assert_ok!(<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = current_ceremony_id();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(ALICE),
			ceremony_id,
			KeygenOutcome::Success(NEW_AGG_PUB_KEY)
		));

		// Can't report twice.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id,
				KeygenOutcome::Success(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
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
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// Only participants can respond.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(u64::MAX),
				ceremony_id,
				KeygenOutcome::Success(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// Wrong ceremony_id.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id + 1,
				KeygenOutcome::Success(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::InvalidCeremonyId
		);
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// A resolution is still pending but no consensus is reached.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// Bob agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(BOB),
			ceremony_id,
			KeygenOutcome::Success(NEW_AGG_PUB_KEY)
		));

		// A resolution is still pending - we 100% response rate.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// Charlie agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(CHARLIE),
			ceremony_id,
			KeygenOutcome::Success(NEW_AGG_PUB_KEY)
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);
		VaultsPallet::on_initialize(1);
		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		assert_matches!(
			PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
			VaultRotationStatus::<MockRuntime, _>::AwaitingRotation { new_public_key: k } if k == NEW_AGG_PUB_KEY
		);

		assert_last_event!(crate::Event::KeygenSuccess(..));

		// Voting has been cleared.
		assert_eq!(SuccessVoters::<MockRuntime, _>::iter_keys().next(), None);
		assert!(!FailureVoters::<MockRuntime, _>::exists());
	})
}

#[test]
fn keygen_report_failure() {
	new_test_ext().execute_with(|| {
		assert_ok!(<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = current_ceremony_id();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(ALICE),
			ceremony_id,
			KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
		));
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// Can't report twice.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id,
				KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// Can't change our mind
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				Origin::signed(ALICE),
				ceremony_id,
				KeygenOutcome::Success(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
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
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
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
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// A resolution is still pending but no consensus is reached.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// Bob agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(BOB),
			ceremony_id,
			KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
		));

		// A resolution is still pending - we expect 100% response rate.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);

		// Charlie agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			Origin::signed(CHARLIE),
			ceremony_id,
			KeygenOutcome::Failure(BTreeSet::from_iter([CHARLIE]))
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Pending
		);
		VaultsPallet::on_initialize(1);
		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(SuccessOrFailure::Failure)
		);

		MockOffenceReporter::assert_reported(PalletOffence::ParticipateKeygenFailed, vec![CHARLIE]);
		MockOffenceReporter::assert_reported(PalletOffence::SigningOffence, vec![CHARLIE]);

		assert_last_event!(crate::Event::KeygenFailure(..));

		// Voting has been cleared.
		assert!(SuccessVoters::<MockRuntime, _>::iter_keys().next().is_none());
		assert!(!FailureVoters::<MockRuntime, _>::exists());
	})
}

#[test]
fn test_grace_period() {
	new_test_ext().execute_with(|| {
		assert_ok!(<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = current_ceremony_id();

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

		// Too many candidates failed to report, so we report nobody.
		MockOffenceReporter::assert_reported(PalletOffence::ParticipateKeygenFailed, vec![]);
		MockOffenceReporter::assert_reported(PalletOffence::SigningOffence, vec![]);
	});
}

#[test]
fn vault_key_rotated() {
	new_test_ext().execute_with(|| {
		const ROTATION_BLOCK_NUMBER: u64 = 42;
		const TX_HASH: [u8; 4] = [0xab; 4];

		assert_noop!(
			VaultsPallet::vault_key_rotated(
				Origin::root(),
				NEW_AGG_PUB_KEY,
				ROTATION_BLOCK_NUMBER,
				TX_HASH,
			),
			Error::<MockRuntime, _>::NoActiveRotation
		);

		assert_ok!(<VaultsPallet as VaultRotator>::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = current_ceremony_id();
		VaultsPallet::on_keygen_success(ceremony_id, NEW_AGG_PUB_KEY);

		assert_ok!(VaultsPallet::vault_key_rotated(
			Origin::root(),
			NEW_AGG_PUB_KEY,
			ROTATION_BLOCK_NUMBER,
			TX_HASH,
		));

		// Can't repeat.
		assert_noop!(
			VaultsPallet::vault_key_rotated(
				Origin::root(),
				NEW_AGG_PUB_KEY,
				ROTATION_BLOCK_NUMBER,
				TX_HASH,
			),
			Error::<MockRuntime, _>::InvalidRotationStatus
		);

		// We have yet to move to the new epoch
		let current_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index();

		let Vault { public_key, active_window } =
			Vaults::<MockRuntime, _>::get(current_epoch).expect("Ethereum Vault should exist");

		assert_eq!(
			public_key, GENESIS_AGG_PUB_KEY,
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
			public_key, NEW_AGG_PUB_KEY,
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
			Some(VaultRotationStatus::Complete { tx_hash: TX_HASH }),
		);
	});
}

mod keygen_reporting {
	use super::*;
	use crate::{AggKeyFor, KeygenOutcome, KeygenOutcomeFor, KeygenResponseStatus};
	use frame_support::assert_err;
	use sp_std::{collections::btree_set::BTreeSet, iter::FromIterator};

	macro_rules! assert_ok_no_repeat {
		($ex:expr) => {{
			assert_ok!($ex);
			assert_err!($ex, Error::<MockRuntime, _>::InvalidRespondent);
		}};
	}

	macro_rules! assert_success_outcome {
		($ex:expr) => {
			let outcome: KeygenOutcomeFor<MockRuntime> = $ex;
			assert!(
				matches!(outcome, KeygenOutcome::Success(_)),
				"Expected success, got: {:?}",
				outcome
			);
		};
	}

	macro_rules! assert_failure_outcome {
		($ex:expr) => {
			let outcome: KeygenOutcomeFor<MockRuntime> = $ex;
			assert!(
				matches!(outcome, KeygenOutcome::Failure(_)),
				"Expected failure, got: {:?}",
				outcome
			);
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

	fn n_times<T: Copy>(things: impl IntoIterator<Item = (usize, T)>) -> Vec<T> {
		things
			.into_iter()
			.flat_map(|(n, thing)| std::iter::repeat(thing).take(n).collect::<Vec<_>>())
			.collect()
	}

	fn unanimous(num_candidates: usize, outcome: ReportedOutcome) -> KeygenOutcomeFor<MockRuntime> {
		get_outcome(&n_times([(num_candidates, outcome)]), |_| [])
	}

	fn unanimous_success(num_candidates: usize) -> KeygenOutcomeFor<MockRuntime> {
		unanimous(num_candidates, ReportedOutcome::Success)
	}

	fn unanimous_failure(num_candidates: usize) -> KeygenOutcomeFor<MockRuntime> {
		unanimous(num_candidates, ReportedOutcome::Failure)
	}

	fn get_outcome_simple<F: Fn(u64) -> I, I: IntoIterator<Item = u64>>(
		num_successes: usize,
		num_failures: usize,
		num_bad_keys: usize,
		num_timeouts: usize,
		report_gen: F,
	) -> KeygenOutcomeFor<MockRuntime> {
		get_outcome(
			n_times([
				(num_successes, ReportedOutcome::Success),
				(num_failures, ReportedOutcome::Failure),
				(num_bad_keys, ReportedOutcome::BadKey),
				(num_timeouts, ReportedOutcome::Timeout),
			])
			.as_slice(),
			report_gen,
		)
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq)]
	enum ReportedOutcome {
		Success,
		BadKey,
		Failure,
		Timeout,
	}

	fn reported_outcomes(outcomes: &[u8]) -> Vec<ReportedOutcome> {
		outcomes
			.iter()
			.map(|o| match *o as char {
				's' => ReportedOutcome::Success,
				'b' => ReportedOutcome::BadKey,
				'f' => ReportedOutcome::Failure,
				't' => ReportedOutcome::Timeout,
				invalid => panic!("Invalid char {:?} in outcomes.", invalid),
			})
			.collect()
	}

	fn get_outcome<F: Fn(u64) -> I, I: IntoIterator<Item = u64>>(
		outcomes: &[ReportedOutcome],
		report_gen: F,
	) -> KeygenOutcome<AggKeyFor<MockRuntime>, u64> {
		let mut status = KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(
			1..=outcomes.len() as u64,
		));

		for (index, outcome) in outcomes.iter().enumerate() {
			let id = 1 + index as u64;
			match outcome {
				ReportedOutcome::Success =>
					assert_ok_no_repeat!(status.add_success_vote(&id, NEW_AGG_PUB_KEY)),
				ReportedOutcome::BadKey =>
					assert_ok_no_repeat!(status.add_success_vote(&id, *b"bad!")),
				ReportedOutcome::Failure => assert_ok_no_repeat!(
					status.add_failure_vote(&id, BTreeSet::from_iter(report_gen(id)))
				),
				ReportedOutcome::Timeout => {},
			}
		}

		let outcome = status.resolve_keygen_outcome();
		assert_eq!(SuccessVoters::<MockRuntime, _>::iter_keys().next(), None);
		assert!(!FailureVoters::<MockRuntime, _>::exists());
		outcome
	}

	/// Keygen can *only* succeed if *all* participants are in agreement.
	#[test]
	fn test_success_consensus() {
		new_test_ext().execute_with(|| {
			for n in 3..200 {
				// Full agreement.
				assert_success_outcome!(unanimous_success(n));
				// Any dissenters cause failure.
				assert_failure_outcome!(get_outcome_simple(n - 1, 1, 0, 0, |_| []));
				assert_failure_outcome!(get_outcome_simple(5, 0, 1, 0, |_| []));
				assert_failure_outcome!(get_outcome_simple(5, 0, 0, 1, |_| []));
			}
		});
	}

	#[test]
	fn test_success_dissent() {
		new_test_ext().execute_with(|| {
			for n in 3..200 {
				for dissent in
					[ReportedOutcome::BadKey, ReportedOutcome::Failure, ReportedOutcome::Timeout]
				{
					let outcome = get_outcome(
						&n_times([(n - 1, ReportedOutcome::Success), (1, dissent)]),
						|_| [],
					);
					assert!(
						matches!(
							outcome.clone(),
							KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([n as u64])
						),
						"Expected Failure([{:?}]), got: {:?}.",
						n,
						outcome
					);
				}
			}
		});
	}

	#[test]
	fn test_failure_consensus() {
		new_test_ext().execute_with(|| {
			for n in 3..200 {
				// Full agreement.
				assert_failure_outcome!(unanimous_failure(n));
				// Minority dissent has no effect.
				assert_failure_outcome!(get_outcome_simple(0, n - 1, 1, 0, |_| []));
				assert_failure_outcome!(get_outcome_simple(1, n - 1, 0, 0, |_| []));
				assert_failure_outcome!(get_outcome_simple(0, n - 1, 0, 1, |_| []));
			}
		});
	}

	#[test]
	fn test_failure_dissent() {
		new_test_ext().execute_with(|| {
			// A keygen where no consensus is reached. Half think we failed, half think we suceeded.
			assert!(matches!(
				get_outcome(
					&n_times([(3, ReportedOutcome::Failure), (3, ReportedOutcome::Success)]),
					|_| [4, 5, 6]
				),
				KeygenOutcome::Failure(blamed) if blamed.is_empty()
			));

			// A keygen where more than `threshold` nodes have reported failure, but there is no
			// final agreement on the guilty parties. Only unresponsive nodes will be reported.
			assert!(matches!(
				get_outcome(
					&n_times([(17, ReportedOutcome::Failure), (7, ReportedOutcome::Timeout)]),
					|id| if id < 16 { [17] } else { [16] }
				),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter(18..=24)
			));

			// As above, but some nodes have reported the wrong outcome.
			assert!(matches!(
				get_outcome(
					&n_times([
						(17, ReportedOutcome::Failure),
						(3, ReportedOutcome::BadKey),
						(2, ReportedOutcome::Success),
						(2, ReportedOutcome::Timeout)
					]),
					|id| if id < 16 { [17] } else { [16] }
				),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter(18..=24)
			));

			// As above, but some nodes have additionally been voted on.
			assert!(matches!(
				get_outcome(
					&n_times([
						(18, ReportedOutcome::Failure),
						(2, ReportedOutcome::BadKey),
						(2, ReportedOutcome::Success),
						(2, ReportedOutcome::Timeout)
					]),
					|id| if id > 16 { [1, 2] } else { [17, 18] }
				),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter(17..=24)
			));
		});
	}

	#[test]
	fn test_blaming_aggregation() {
		new_test_ext().execute_with(|| {
			// First five candidates all report candidate 6, candidate 6 unresponsive.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffft"), |_| [6]),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// First five candidates all report candidate 6, candidate 6 reports 1.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffft"), |id| if id == 6 { [1] } else { [6] }),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// First five candidates all report nobody, candidate 6 unresponsive.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffft"), |_| []),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// Candidates 3 and 6 unresponsive.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"fftfft"), |_| []),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([3, 6])
			));

			// One candidate unresponsive, one blamed by majority.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffftf"), |id| if id != 3 { [3] } else { [4] }),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([3, 6])
			));

			// One candidate unresponsive, one rogue blames everyone else.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffftf"), |id| {
					if id != 3 {
						vec![3, 6]
					} else {
						vec![1, 2, 4, 5, 6, 7]
					}
				}),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([3, 6])
			));

			let failures = |n| n_times([(n, ReportedOutcome::Failure)]);

			// Candidates don't agree.
			assert!(matches!(
				get_outcome(&failures(6), |id| [(id + 1) % 6]),
				KeygenOutcome::Failure(blamed) if blamed.is_empty()
			));

			// Candidate agreement is below reporting threshold.
			assert!(matches!(
				get_outcome(&failures(6), |id| if id < 4 { [6] } else { [2] }),
				KeygenOutcome::Failure(blamed) if blamed.is_empty()
			));

			// Candidates agreement just above threshold.
			assert!(matches!(
				get_outcome(&failures(6), |id| if id == 6 { [1] } else { [6] }),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// Candidates agree on multiple offenders.
			assert!(matches!(
				get_outcome(&failures(12), |id| if id < 9 { [11, 12] } else { [1, 2] }),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([11, 12])
			));

			// Overlapping agreement - no agreement on the entire set but in aggregate we can
			// determine offenders.
			assert!(matches!(
				get_outcome(&failures(12), |id| {
					if id < 5 {
						[11, 12]
					} else if id < 9 {
						[1, 11]
					} else {
						[1, 2]
					}
				}),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([1, 11])
			));

			// Unresponsive and dissenting nodes are reported.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"tfffsfffbffft"), |_| []),
				KeygenOutcome::Failure(blamed) if blamed == BTreeSet::from_iter([1, 5, 9, 13])
			));
		});
	}
}
