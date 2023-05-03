use crate::{
	mock::*, CeremonyId, Error, Event as PalletEvent, KeygenFailureVoters,
	KeygenResolutionPendingSince, KeygenResponseTimeout, KeygenSuccessVoters, PalletOffence,
	PendingVaultRotation, Vault, VaultRotationStatus, Vaults,
};
use cf_chains::eth::Ethereum;
use cf_test_utilities::{last_event, maybe_last_event};
use cf_traits::{
	mocks::{ceremony_id_provider::MockCeremonyIdProvider, threshold_signer::MockThresholdSigner},
	AccountRoleRegistry, AsyncResult, Chainflip, EpochInfo, VaultRotator, VaultStatus,
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
	MockCeremonyIdProvider::get()
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
		let next_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone());
		// Confirm we have a new vault rotation process running
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		assert_eq!(
			last_event::<MockRuntime>(),
			PalletEvent::<MockRuntime, _>::KeygenRequest {
				ceremony_id: current_ceremony_id(),
				participants: btree_candidates.clone(),
				epoch_index: next_epoch,
			}
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
			PalletEvent::KeygenRequest {
				ceremony_id: current_ceremony_id(),
				participants: ALL_CANDIDATES.iter().cloned().collect(),
				epoch_index: <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1,
			}
			.into()
		);
	});
}

#[test]
fn keygen_verification_failure() {
	new_test_ext().execute_with(|| {
		let participants = (5u64..15).into_iter().collect::<BTreeSet<_>>();
		let keygen_ceremony_id = 12;

		let request_id = VaultsPallet::trigger_keygen_verification(
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
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
		);
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY)
		));

		// Only participants can respond.
		let non_participant = u64::MAX;
		<<MockRuntime as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<MockRuntime>>::register_as_validator(
			&non_participant,
		)
		.unwrap();
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
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
		);
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
		if let VaultRotationStatus::AwaitingKeygen { keygen_ceremony_id: keygen_ceremony_id_from_status, response_status, keygen_participants, } = PendingVaultRotation::<MockRuntime, _>::get().unwrap() {
			assert_eq!(keygen_ceremony_id, keygen_ceremony_id_from_status);
			assert_eq!(response_status.success_votes().get(&NEW_AGG_PUB_KEY).expect("new key should have votes"), &3);
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
		assert_eq!(KeygenSuccessVoters::<MockRuntime, _>::iter_keys().next(), None);
		assert!(!KeygenFailureVoters::<MockRuntime, _>::exists());
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
		assert!(KeygenSuccessVoters::<MockRuntime, _>::iter_keys().next().is_none());
		assert!(!KeygenFailureVoters::<MockRuntime, _>::exists());
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
