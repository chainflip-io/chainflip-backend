use crate::{
	mock::*, CeremonyId, Error, Event as PalletEvent, FailureVoters, KeygenResolutionPendingSince,
	KeygenResponseTimeout, PalletOffence, PendingVaultRotation, SuccessVoters, Vault,
	VaultRotationStatus, Vaults,
};
use cf_chains::eth::Ethereum;
use cf_test_utilities::{last_event, maybe_last_event};
use cf_traits::{
	mocks::{ceremony_id_provider::MockCeremonyIdProvider, threshold_signer::MockThresholdSigner},
	AsyncResult, Chainflip, EpochInfo, VaultRotator, VaultStatus,
};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_std::collections::btree_set::BTreeSet;

pub type EthMockThresholdSigner = MockThresholdSigner<Ethereum, crate::mock::RuntimeCall>;

macro_rules! assert_last_event {
	($pat:pat) => {
		let event = last_event::<MockRuntime>();
		assert!(
			matches!(event, $crate::mock::RuntimeEvent::VaultsPallet($pat)),
			"Unexpected event {:?}",
			event
		);
	};
}

fn current_ceremony_id() -> CeremonyId {
	MockCeremonyIdProvider::<u64>::get()
}

const ALL_CANDIDATES: &[<MockRuntime as Chainflip>::ValidatorId] = &[ALICE, BOB, CHARLIE];

#[test]
#[should_panic]
fn start_panics_with_no_candidates() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::default());
	});
}

#[test]
fn keygen_request_emitted() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone());
		// Confirm we have a new vault rotation process running
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		assert_eq!(
			last_event::<MockRuntime>(),
			PalletEvent::<MockRuntime, _>::KeygenRequest(current_ceremony_id(), btree_candidates)
				.into()
		);
	});
}

#[test]
#[should_panic]
fn start_panics_if_called_while_vault_rotation_in_progress() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone());
		<VaultsPallet as VaultRotator>::keygen(btree_candidates);
	});
}

#[test]
fn keygen_success_triggers_keygen_verification() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone());
		let ceremony_id = current_ceremony_id();

		VaultsPallet::trigger_keygen_verification(ceremony_id, NEW_AGG_PUB_KEY, btree_candidates);

		assert!(matches!(
			PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
			VaultRotationStatus::<MockRuntime, _>::AwaitingKeygenVerification { new_public_key: k } if k == NEW_AGG_PUB_KEY
		));
	});
}

fn keygen_failure(bad_candidates: &[<MockRuntime as Chainflip>::ValidatorId]) {
	VaultsPallet::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));

	let ceremony_id = current_ceremony_id();

	VaultsPallet::terminate_keygen_procedure(
		bad_candidates,
		PalletEvent::KeygenFailure(ceremony_id),
	);

	assert_eq!(last_event::<MockRuntime>(), PalletEvent::KeygenFailure(ceremony_id).into());

	assert_eq!(
		VaultsPallet::status(),
		AsyncResult::Ready(VaultStatus::Failed(bad_candidates.iter().cloned().collect()))
	);

	MockOffenceReporter::assert_reported(
		PalletOffence::FailedKeygen,
		bad_candidates.iter().cloned(),
	);
}

#[test]
fn test_keygen_failure() {
	new_test_ext().execute_with(|| {
		keygen_failure(&[BOB, CHARLIE]);
	});
}

// This happens when the vault reports failure (through its status) to the validator pallet.
// Once all vaults have reported some AsyncResul::Ready status (see all_vaults_rotator) then
// the validator pallet will call keygen() again
#[test]
fn keygen_called_after_keygen_failure_restarts_rotation_at_keygen() {
	new_test_ext().execute_with(|| {
		keygen_failure(&[BOB, CHARLIE]);

		VaultsPallet::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));

		assert_eq!(VaultsPallet::status(), AsyncResult::Pending);

		assert_eq!(
			last_event::<MockRuntime>(),
			PalletEvent::KeygenRequest(
				current_ceremony_id(),
				ALL_CANDIDATES.iter().cloned().collect()
			)
			.into()
		);
	});
}

#[test]
fn keygen_verification_failure() {
	new_test_ext().execute_with(|| {
		let participants = (5u64..15).into_iter().collect::<BTreeSet<_>>();

		let keygen_ceremony_id = 12;
		let (request_id, _) = VaultsPallet::trigger_keygen_verification(
			keygen_ceremony_id,
			NEW_AGG_PUB_KEY,
			participants.clone(),
		);

		let blamed = vec![5, 6, 7, 8];
		assert!(blamed.iter().all(|b| participants.contains(b)));

		EthMockThresholdSigner::set_signature_ready(request_id, Err(blamed.clone()));

		EthMockThresholdSigner::on_signature_ready(request_id);

		assert_last_event!(PalletEvent::KeygenVerificationFailure { .. });
		MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, blamed.clone());
		assert_eq!(
			VaultsPallet::status(),
			AsyncResult::Ready(VaultStatus::Failed(blamed.into_iter().collect()))
		)
	});
}

#[test]
fn no_active_rotation() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				1,
				Ok(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::NoActiveRotation
		);

		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				1,
				Err(Default::default())
			),
			Error::<MockRuntime, _>::NoActiveRotation
		);
	})
}

#[test]
fn cannot_report_keygen_success_twice() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY)
		));

		// Can't report twice.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id,
				Ok(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn cannot_report_two_different_keygen_outcomes() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY)
		));

		// Can't report failure after reporting success
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id,
				Err(BTreeSet::from_iter([BOB, CHARLIE]))
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn only_participants_can_report_keygen_outcome() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY)
		));

		// Only participants can respond.
		let non_participant = u64::MAX;
		assert!(!ALL_CANDIDATES.contains(&non_participant), "Non-participant is a candidate");
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(non_participant),
				ceremony_id,
				Ok(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn reporting_keygen_outcome_must_be_for_pending_ceremony_id() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY)
		));

		// Ceremony id in the past (not the pending one we're waiting for)
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id - 1,
				Ok(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::InvalidCeremonyId
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		// Ceremony id in the future
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id + 1,
				Ok(NEW_AGG_PUB_KEY)
			),
			Error::<MockRuntime, _>::InvalidCeremonyId
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn keygen_report_success() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		let keygen_ceremony_id = current_ceremony_id();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY)
		));

		assert_eq!(
			<VaultsPallet as VaultRotator>::status(),
			AsyncResult::Pending
		);

		VaultsPallet::on_initialize(1);
		// After on initialise we obviously still don't have enough votes.
		// So nothing should have changed.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::status(),
			AsyncResult::Pending
		);

		// Bob agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(BOB),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY)
		));

		// A resolution is still pending - we require 100% response rate.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::status(),
			AsyncResult::Pending
		);
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::status(),
			AsyncResult::Pending
		);

		// Charlie agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(CHARLIE),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY)
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::status(),
			AsyncResult::Pending
		);
		if let VaultRotationStatus::AwaitingKeygen { keygen_ceremony_id: keygen_ceremony_id_from_status, response_status, keygen_participants } = PendingVaultRotation::<MockRuntime, _>::get().unwrap() {
			assert_eq!(keygen_ceremony_id, keygen_ceremony_id_from_status);
			assert_eq!(response_status.success_votes.get(&NEW_AGG_PUB_KEY).expect("new key should have votes"), &3);
			assert_eq!(keygen_participants, BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		} else {
			panic!("Expected to be in AwaitingKeygen state");
		}
		VaultsPallet::on_initialize(1);

		assert!(matches!(PendingVaultRotation::<MockRuntime, _>::get().unwrap(), VaultRotationStatus::AwaitingKeygenVerification { .. }));

		EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));

		assert!(matches!(PendingVaultRotation::<MockRuntime, _>::get().unwrap(), VaultRotationStatus::KeygenVerificationComplete { .. }));

		// Called by validator pallet
		VaultsPallet::activate();

		assert!(matches!(PendingVaultRotation::<MockRuntime, _>::get().unwrap(), VaultRotationStatus::AwaitingRotation { .. }));

		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::status(),
			AsyncResult::Pending
		);

		assert!(matches!(
			PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
			VaultRotationStatus::<MockRuntime, _>::AwaitingRotation { new_public_key: k } if k == NEW_AGG_PUB_KEY
		));

		assert_last_event!(crate::Event::KeygenVerificationSuccess { .. });

		// Voting has been cleared.
		assert_eq!(SuccessVoters::<MockRuntime, _>::iter_keys().next(), None);
		assert!(!FailureVoters::<MockRuntime, _>::exists());
	})
}

#[test]
fn keygen_report_failure() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		let ceremony_id = current_ceremony_id();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		// Bob agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(BOB),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));

		// A resolution is still pending - we expect 100% response rate.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		// Charlie agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(CHARLIE),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		VaultsPallet::on_initialize(1);
		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			VaultsPallet::status(),
			AsyncResult::Ready(VaultStatus::Failed(BTreeSet::from([CHARLIE])))
		);

		MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, vec![CHARLIE]);

		assert_last_event!(crate::Event::KeygenFailure(..));

		// Voting has been cleared.
		assert!(SuccessVoters::<MockRuntime, _>::iter_keys().next().is_none());
		assert!(!FailureVoters::<MockRuntime, _>::exists());
	})
}

#[test]
fn test_keygen_timeout_period() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		let ceremony_id = current_ceremony_id();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));

		// > 25 blocks later we should resolve an error.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(MOCK_KEYGEN_RESPONSE_TIMEOUT);
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		VaultsPallet::on_initialize(MOCK_KEYGEN_RESPONSE_TIMEOUT + 1);
		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());

		// Too many candidates failed to report, so we report nobody.
		MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, vec![]);
	});
}

#[test]
fn vault_key_rotated() {
	new_test_ext().execute_with(|| {
		const ROTATION_BLOCK_NUMBER: u64 = 42;
		const TX_HASH: [u8; 4] = [0xab; 4];
		let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

		assert_noop!(
			VaultsPallet::vault_key_rotated(
				RuntimeOrigin::root(),
				NEW_AGG_PUB_KEY,
				ROTATION_BLOCK_NUMBER,
				TX_HASH,
			),
			Error::<MockRuntime, _>::NoActiveRotation
		);

		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone());
		let ceremony_id = current_ceremony_id();
		VaultsPallet::trigger_keygen_verification(ceremony_id, NEW_AGG_PUB_KEY, btree_candidates);

		EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));

		// validator pallet kicks this off
		VaultsPallet::activate();

		assert_ok!(VaultsPallet::vault_key_rotated(
			RuntimeOrigin::root(),
			NEW_AGG_PUB_KEY,
			ROTATION_BLOCK_NUMBER,
			TX_HASH,
		));

		// Can't repeat.
		assert_noop!(
			VaultsPallet::vault_key_rotated(
				RuntimeOrigin::root(),
				NEW_AGG_PUB_KEY,
				ROTATION_BLOCK_NUMBER,
				TX_HASH,
			),
			Error::<MockRuntime, _>::InvalidRotationStatus
		);

		// We have yet to move to the new epoch
		let current_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index();

		let Vault { public_key, active_from_block } =
			Vaults::<MockRuntime, _>::get(current_epoch).expect("Ethereum Vault should exist");
		assert_eq!(
			public_key, GENESIS_AGG_PUB_KEY,
			"we should have the old agg key in the genesis vault"
		);
		assert_eq!(
			active_from_block, 0,
			"we should have set the from block for the genesis or current epoch"
		);

		// The next epoch
		let next_epoch = current_epoch + 1;
		let Vault { public_key, active_from_block } = Vaults::<MockRuntime, _>::get(next_epoch)
			.expect("Ethereum Vault should exist in the next epoch");
		assert_eq!(
			public_key, NEW_AGG_PUB_KEY,
			"we should have the new public key in the new vault for the next epoch"
		);
		assert_eq!(
			active_from_block,
			ROTATION_BLOCK_NUMBER.saturating_add(1),
			"we should have set the starting point for the new vault's active window as the next
			after the reported block number"
		);

		// Status is complete.
		assert_eq!(
			PendingVaultRotation::<MockRuntime, _>::get(),
			Some(VaultRotationStatus::Complete { tx_id: TX_HASH }),
		);
		assert_last_event!(crate::Event::VaultRotationCompleted { .. });
	});
}

#[test]
fn test_vault_key_rotated_externally() {
	new_test_ext().execute_with(|| {
		const TX_HASH: [u8; 4] = [0xab; 4];
		assert_eq!(MockSystemStateManager::get_current_system_state(), SystemState::Normal);
		assert_ok!(VaultsPallet::vault_key_rotated_externally(
			RuntimeOrigin::root(),
			NEW_AGG_PUB_KEY,
			1,
			TX_HASH,
		));
		assert_eq!(MockSystemStateManager::get_current_system_state(), SystemState::Maintenance);
		assert_last_event!(crate::Event::VaultRotatedExternally(..));
	});
}

#[test]
fn key_unavailabe_on_activate_returns_governance_event() {
	new_test_ext_no_key().execute_with(|| {
		PendingVaultRotation::put(
			VaultRotationStatus::<MockRuntime, _>::KeygenVerificationComplete {
				new_public_key: NEW_AGG_PUB_KEY,
			},
		);

		VaultsPallet::activate();

		assert_last_event!(crate::Event::AwaitingGovernanceActivation { .. });
	})
}

#[test]
fn set_keygen_response_timeout_works() {
	new_test_ext_no_key().execute_with(|| {
		let init_timeout = KeygenResponseTimeout::<MockRuntime, _>::get();

		VaultsPallet::set_keygen_response_timeout(RuntimeOrigin::root(), init_timeout).unwrap();

		assert!(maybe_last_event::<MockRuntime>().is_none());

		let new_timeout = init_timeout + 1;

		VaultsPallet::set_keygen_response_timeout(RuntimeOrigin::root(), new_timeout).unwrap();

		assert_last_event!(crate::Event::KeygenResponseTimeoutUpdated { .. });
		assert_eq!(KeygenResponseTimeout::<MockRuntime, _>::get(), new_timeout)
	})
}

mod keygen_reporting {
	use super::*;
	use crate::{AggKeyFor, KeygenOutcomeFor, KeygenResponseStatus};
	use sp_std::collections::btree_set::BTreeSet;

	macro_rules! assert_failure_outcome {
		($ex:expr) => {
			let outcome: KeygenOutcomeFor<MockRuntime> = $ex;
			assert!(matches!(outcome, Err(_)), "Expected failure, got: {:?}", outcome);
		};
	}

	#[test]
	fn test_threshold() {
		// The success threshold is the smallest number of participants that *can* reach consensus.
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..144))
				.super_majority_threshold(),
			96
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..145))
				.super_majority_threshold(),
			97
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..146))
				.super_majority_threshold(),
			98
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..147))
				.super_majority_threshold(),
			98
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..148))
				.super_majority_threshold(),
			99
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..149))
				.super_majority_threshold(),
			100
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..150))
				.super_majority_threshold(),
			100
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..151))
				.super_majority_threshold(),
			101
		);
	}

	// Takes an IntoIterator of tuples where the usize represents the number of times
	// we want to repeat the T
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
		/// When a node considers the keygen a success, but votes for a key that is actually not
		/// the correct key (according to the majority of nodes)
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
				invalid => panic!("Invalid char {invalid:?} in outcomes."),
			})
			.collect()
	}

	fn get_outcome<F: Fn(u64) -> I, I: IntoIterator<Item = u64>>(
		outcomes: &[ReportedOutcome],
		report_gen: F,
	) -> Result<AggKeyFor<MockRuntime>, BTreeSet<u64>> {
		let mut status = KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(
			1..=outcomes.len() as u64,
		));

		for (index, outcome) in outcomes.iter().enumerate() {
			let id = 1 + index as u64;
			match outcome {
				ReportedOutcome::Success => {
					status.add_success_vote(&id, NEW_AGG_PUB_KEY);
				},
				ReportedOutcome::BadKey => {
					status.add_success_vote(&id, *b"bad!");
				},
				ReportedOutcome::Failure => {
					status.add_failure_vote(&id, BTreeSet::from_iter(report_gen(id)));
				},
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
				assert_ok!(unanimous_success(n));
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
					// a single node is reporting incorrectly
					let outcome = get_outcome(
						&n_times([(n - 1, ReportedOutcome::Success), (1, dissent)]),
						|_| [],
					);
					assert!(
						matches!(
							outcome.clone(),
							Err(blamed) if blamed == BTreeSet::from_iter([n as u64])
						),
						"Expected Failure([{n:?}]), got: {outcome:?}."
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
			let outcome = get_outcome(
				&n_times([(3, ReportedOutcome::Failure), (3, ReportedOutcome::Success)]),
				|_| [4, 5, 6],
			);
			assert!(
				matches!(
					outcome.clone(),
					Err(blamed) if blamed.is_empty(),
				),
				"Got outcome: {outcome:?}",
			);

			// A keygen where more than `threshold` nodes have reported failure, but there is no
			// final agreement on the guilty parties. Only unresponsive nodes will be reported.
			assert!(matches!(
				get_outcome(
					&n_times([(17, ReportedOutcome::Failure), (7, ReportedOutcome::Timeout)]),
					|id| if id < 16 { [17] } else { [16] }
				),
				Err(blamed) if blamed == BTreeSet::from_iter(18..=24)
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
				Err(blamed) if blamed == BTreeSet::from_iter(18..=24)
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
				Err(blamed) if blamed == BTreeSet::from_iter(17..=24)
			));
		});
	}

	#[test]
	fn test_blaming_aggregation() {
		new_test_ext().execute_with(|| {
			// First five candidates all report candidate 6, candidate 6 unresponsive.
			let outcome = get_outcome(&reported_outcomes(b"ffffft"), |_| [6]);
			assert!(
				matches!(
					outcome.clone(),
					Err(blamed) if blamed == BTreeSet::from_iter([6])
				),
				"Got outcome: {outcome:?}",
			);

			// First five candidates all report candidate 6, candidate 6 reports 1.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffft"), |id| if id == 6 { [1] } else { [6] }),
				Err(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// First five candidates all report nobody, candidate 6 unresponsive.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffft"), |_| []),
				Err(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// Candidates 3 and 6 unresponsive.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"fftfft"), |_| []),
				Err(blamed) if blamed == BTreeSet::from_iter([3, 6])
			));
			// One candidate unresponsive, one blamed by majority.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffftf"), |id| if id != 3 { [3] } else { [4] }),
				Err(blamed) if blamed == BTreeSet::from_iter([3, 6])
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
				Err(blamed) if blamed == BTreeSet::from_iter([3, 6])
			));

			let failures = |n| n_times([(n, ReportedOutcome::Failure)]);

			// Candidates don't agree.
			assert!(matches!(
				get_outcome(&failures(6), |id| [(id + 1) % 6]),
				Err(blamed) if blamed.is_empty()
			));

			// Candidate agreement is below reporting threshold.
			assert!(matches!(
				get_outcome(&failures(6), |id| if id < 4 { [6] } else { [2] }),
				Err(blamed) if blamed.is_empty()
			));

			// Candidates agreement just above threshold.
			assert!(matches!(
				get_outcome(&failures(6), |id| if id == 6 { [1] } else { [6] }),
				Err(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// Candidates agree on multiple offenders.
			assert!(matches!(
				get_outcome(&failures(12), |id| if id < 9 { [11, 12] } else { [1, 2] }),
				Err(blamed) if blamed == BTreeSet::from_iter([11, 12])
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
				Err(blamed) if blamed == BTreeSet::from_iter([1, 11])
			));

			// Unresponsive and dissenting nodes are reported.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"tfffsfffbffft"), |_| []),
				Err(blamed) if blamed == BTreeSet::from_iter([1, 5, 9, 13])
			));
		});
	}
}
