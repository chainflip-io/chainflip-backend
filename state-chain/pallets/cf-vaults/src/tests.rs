#![cfg(test)]

use core::panic;

use crate::{
	mock::*, CeremonyId, Error, Event as PalletEvent, KeyHandoverResolutionPendingSince,
	KeygenFailureVoters, KeygenOutcomeFor, KeygenResolutionPendingSince, KeygenResponseTimeout,
	KeygenSuccessVoters, PalletOffence, PendingVaultRotation, Vault, VaultRotationStatus, Vaults,
};
use cf_chains::{
	eth::Ethereum,
	mocks::{MockAggKey, MockOptimisticActivation},
};
use cf_primitives::GENESIS_EPOCH;
use cf_test_utilities::{last_event, maybe_last_event};
use cf_traits::{
	mocks::threshold_signer::MockThresholdSigner, AccountRoleRegistry, AsyncResult, Chainflip,
	EpochInfo, KeyProvider, SafeMode, SetSafeMode, VaultRotator, VaultStatus,
};
use frame_support::{
	assert_noop, assert_ok, pallet_prelude::DispatchResultWithPostInfo, traits::Hooks,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::Get;
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
	VaultsPallet::ceremony_id_counter()
}

const ALL_CANDIDATES: &[<MockRuntime as Chainflip>::ValidatorId] = &[ALICE, BOB, CHARLIE];

#[test]
#[should_panic]
fn start_panics_with_no_candidates() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::default(), GENESIS_EPOCH);
	});
}

#[test]
fn keygen_request_emitted() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		let rotation_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index();
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone(), rotation_epoch);
		// Confirm we have a new vault rotation process running
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		assert_eq!(
			last_event::<MockRuntime>(),
			PalletEvent::<MockRuntime, _>::KeygenRequest {
				ceremony_id: current_ceremony_id(),
				participants: btree_candidates.clone(),
				epoch_index: rotation_epoch,
			}
			.into()
		);
	});
}

#[test]
fn keygen_handover_request_emitted() {
	let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		let current_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index();
		let next_epoch = current_epoch + 1;

		PendingVaultRotation::<MockRuntime, _>::put(
			VaultRotationStatus::KeygenVerificationComplete { new_public_key: Default::default() },
		);
		let ceremony_id = current_ceremony_id();

		<VaultsPallet as VaultRotator>::key_handover(
			candidates.clone(),
			candidates.clone(),
			next_epoch,
		);

		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		assert_eq!(
			last_event::<MockRuntime>(),
			PalletEvent::<MockRuntime, _>::KeyHandoverRequest {
				// It should be incremented when the request is made.
				ceremony_id: ceremony_id + 1,
				from_epoch: current_epoch,
				key_to_share: VaultsPallet::active_epoch_key().unwrap().key,
				sharing_participants: candidates.clone(),
				receiving_participants: candidates,
				new_key: Default::default(),
				to_epoch: next_epoch,
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
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone(), GENESIS_EPOCH);
		<VaultsPallet as VaultRotator>::keygen(btree_candidates, GENESIS_EPOCH);
	});
}

#[test]
fn keygen_success_triggers_keygen_verification() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		let rotation_epoch_index = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone(), rotation_epoch_index);
		let ceremony_id = current_ceremony_id();

		VaultsPallet::trigger_keygen_verification(
			ceremony_id,
			NEW_AGG_PUB_KEY_PRE_HANDOVER,
			btree_candidates,
			rotation_epoch_index,
		);
	})
}

fn keygen_failure(bad_candidates: &[<MockRuntime as Chainflip>::ValidatorId]) {
	VaultsPallet::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()), GENESIS_EPOCH);

	let ceremony_id = current_ceremony_id();

	VaultsPallet::terminate_rotation(bad_candidates, PalletEvent::KeygenFailure(ceremony_id));

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
		let rotation_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
		keygen_failure(&[BOB, CHARLIE]);
		VaultsPallet::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()), rotation_epoch);

		assert_eq!(VaultsPallet::status(), AsyncResult::Pending);

		assert_eq!(
			last_event::<MockRuntime>(),
			PalletEvent::KeygenRequest {
				ceremony_id: current_ceremony_id(),
				participants: ALL_CANDIDATES.iter().cloned().collect(),
				epoch_index: rotation_epoch,
			}
			.into()
		);
	});
}

#[test]
fn keygen_verification_failure() {
	new_test_ext().execute_with(|| {
		let rotation_epoch_index = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
		let participants = (5u64..15).collect::<BTreeSet<_>>();
		let keygen_ceremony_id = 12;

		let request_id = VaultsPallet::trigger_keygen_verification(
			keygen_ceremony_id,
			NEW_AGG_PUB_KEY_PRE_HANDOVER,
			participants.clone(),
			rotation_epoch_index,
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
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
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
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));

		// Can't report twice.
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn cannot_report_two_different_keygen_outcomes() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
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
fn only_candidates_can_report_keygen_outcome() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()) , GENESIS_EPOCH);
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));

		// Only candidates can respond.
		let non_candidate = u64::MAX;
		<<MockRuntime as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<MockRuntime>>::register_as_validator(
			&non_candidate,
		)
		.unwrap();
		assert!(!ALL_CANDIDATES.contains(&non_candidate));
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(non_candidate),
				ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<MockRuntime, _>::InvalidRespondent
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn can_only_blame_keygen_candidates() {
	new_test_ext().execute_with(|| {
		let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());
		let valid_blames = BTreeSet::from_iter([BOB, CHARLIE]);
		let invalid_blames = BTreeSet::from_iter([u64::MAX - 1, u64::MAX]);
		assert!(valid_blames.is_subset(&candidates));
		assert!(invalid_blames.is_disjoint(&candidates));

		<VaultsPallet as VaultRotator>::keygen(candidates, GENESIS_EPOCH);

		VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			current_ceremony_id(),
			// Report both the valid and invalid offenders
			Err(valid_blames.iter().cloned().chain(invalid_blames.clone()).collect()),
		)
		.unwrap();

		match PendingVaultRotation::<MockRuntime, _>::get().unwrap() {
			VaultRotationStatus::AwaitingKeygen { response_status, .. } => {
				let blamed: BTreeSet<_> = response_status.blame_votes().keys().cloned().collect();

				assert_eq!(&valid_blames, &blamed);
				assert!(invalid_blames.is_disjoint(&blamed));
			},
			_ => panic!("Expected to be in AwaitingKeygen state"),
		}
	});
}

#[test]
fn reporting_keygen_outcome_must_be_for_pending_ceremony_id() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));

		// Ceremony id in the past (not the pending one we're waiting for)
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id - 1,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<MockRuntime, _>::InvalidCeremonyId
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		// Ceremony id in the future
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id + 1,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<MockRuntime, _>::InvalidCeremonyId
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn cannot_report_key_handover_outcome_when_awaiting_keygen() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			<MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1,
		);

		assert_noop!(
			VaultsPallet::report_key_handover_outcome(
				RuntimeOrigin::signed(ALICE),
				current_ceremony_id(),
				Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)
			),
			Error::<MockRuntime, _>::InvalidRotationStatus
		);
	});
}

#[test]
fn keygen_report_success() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()), rotation_epoch);
		let keygen_ceremony_id = current_ceremony_id();

		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
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
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
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
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::status(),
			AsyncResult::Pending
		);
		if let VaultRotationStatus::AwaitingKeygen { ceremony_id: keygen_ceremony_id_from_status, response_status, keygen_participants, new_epoch_index } = PendingVaultRotation::<MockRuntime, _>::get().unwrap() {
			assert_eq!(keygen_ceremony_id, keygen_ceremony_id_from_status);
			assert_eq!(response_status.success_votes().get(&NEW_AGG_PUB_KEY_PRE_HANDOVER).expect("new key should have votes"), &3);
			assert_eq!(keygen_participants, BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
			assert_eq!(new_epoch_index, rotation_epoch);
		} else {
			panic!("Expected to be in AwaitingKeygen state");
		}
		VaultsPallet::on_initialize(1);

		assert!(matches!(PendingVaultRotation::<MockRuntime, _>::get().unwrap(), VaultRotationStatus::AwaitingKeygenVerification { .. }));

		EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));

		assert!(matches!(PendingVaultRotation::<MockRuntime, _>::get().unwrap(), VaultRotationStatus::KeygenVerificationComplete { .. }));

		const HANDOVER_PARTICIPANTS: [u64; 2] = [ALICE, BOB];
		VaultsPallet::key_handover(BTreeSet::from(HANDOVER_PARTICIPANTS), BTreeSet::from(HANDOVER_PARTICIPANTS), rotation_epoch);

		let handover_ceremony_id = current_ceremony_id();
		for p in HANDOVER_PARTICIPANTS {
			assert_ok!(VaultsPallet::report_key_handover_outcome(
				RuntimeOrigin::signed(p),
				handover_ceremony_id,
				Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)
			));
		}

		VaultsPallet::on_initialize(1);

		// Called by validator pallet
		VaultsPallet::activate();

		assert!(matches!(PendingVaultRotation::<MockRuntime, _>::get().unwrap(), VaultRotationStatus::AwaitingActivation { .. }));

		assert!(!KeygenResolutionPendingSince::<MockRuntime, _>::exists());
		assert_eq!(
			<VaultsPallet as VaultRotator>::status(),
			AsyncResult::Pending
		);

		assert!(matches!(
			PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
			VaultRotationStatus::<MockRuntime, _>::AwaitingActivation { new_public_key: k } if k == NEW_AGG_PUB_KEY_POST_HANDOVER
		));

		assert_last_event!(crate::Event::KeyHandoverSuccess { .. });

		// Voting has been cleared.
		assert_eq!(KeygenSuccessVoters::<MockRuntime, _>::iter_keys().next(), None);
		assert!(!KeygenFailureVoters::<MockRuntime, _>::exists());
	})
}

#[test]
fn keygen_report_failure() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
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

fn test_key_ceremony_timeout_period<PendingSince, ReportFn>(report_fn: ReportFn)
where
	PendingSince: frame_support::StorageValue<
		BlockNumberFor<MockRuntime>,
		Query = BlockNumberFor<MockRuntime>,
	>,
	ReportFn: Fn(
		RuntimeOrigin,
		CeremonyId,
		Result<MockAggKey, BTreeSet<u64>>,
	) -> DispatchResultWithPostInfo,
{
	let ceremony_id = current_ceremony_id();

	assert_eq!(PendingSince::get(), 1);

	assert_ok!(report_fn(
		RuntimeOrigin::signed(ALICE),
		ceremony_id,
		Err(BTreeSet::from_iter([CHARLIE]))
	));

	// > 25 blocks later we should resolve an error.
	assert!(PendingSince::exists());
	VaultsPallet::on_initialize(1);
	assert!(PendingSince::exists());
	VaultsPallet::on_initialize(MOCK_KEYGEN_RESPONSE_TIMEOUT);
	assert!(PendingSince::exists());
	VaultsPallet::on_initialize(MOCK_KEYGEN_RESPONSE_TIMEOUT + 1);
	assert!(!PendingSince::exists());

	// Too many candidates failed to report, so we report nobody.
	MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, vec![]);
}

#[test]
fn test_keygen_timeout_period() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		test_key_ceremony_timeout_period::<KeygenResolutionPendingSince<MockRuntime, _>, _>(
			VaultsPallet::report_keygen_outcome,
		)
	});
}

#[test]
fn test_key_handover_timeout_period() {
	new_test_ext().execute_with(|| {
		let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());
		PendingVaultRotation::<MockRuntime, _>::put(
			VaultRotationStatus::KeygenVerificationComplete { new_public_key: Default::default() },
		);
		<VaultsPallet as VaultRotator>::key_handover(candidates.clone(), candidates, 2);
		test_key_ceremony_timeout_period::<KeyHandoverResolutionPendingSince<MockRuntime, _>, _>(
			VaultsPallet::report_key_handover_outcome,
		)
	});
}

#[cfg(test)]
mod vault_key_rotation {
	use super::*;

	const ACTIVATION_BLOCK_NUMBER: u64 = 42;
	const TX_HASH: [u8; 4] = [0xab; 4];

	fn setup(outcome: KeygenOutcomeFor<MockRuntime>) -> sp_io::TestExternalities {
		let mut ext = new_test_ext();
		ext.execute_with(|| {
			let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

			let rotation_epoch_index = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;

			assert_noop!(
				VaultsPallet::vault_key_rotated(
					RuntimeOrigin::root(),
					ACTIVATION_BLOCK_NUMBER,
					TX_HASH,
				),
				Error::<MockRuntime, _>::NoActiveRotation
			);

			<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone(), GENESIS_EPOCH);
			let ceremony_id = current_ceremony_id();
			VaultsPallet::trigger_keygen_verification(
				ceremony_id,
				NEW_AGG_PUB_KEY_PRE_HANDOVER,
				btree_candidates.clone(),
				rotation_epoch_index,
			);

			EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(
				ETH_DUMMY_SIG,
			));

			VaultsPallet::key_handover(
				btree_candidates.clone(),
				btree_candidates.clone(),
				rotation_epoch_index,
			);

			for candidate in btree_candidates {
				assert_ok!(VaultsPallet::report_key_handover_outcome(
					RuntimeOrigin::signed(candidate),
					current_ceremony_id(),
					outcome.clone()
				));
			}

			VaultsPallet::on_initialize(1);
		});
		ext
	}

	fn final_checks(ext: &mut sp_io::TestExternalities, expected_activation_block: u64) {
		ext.execute_with(|| {
			// Can't repeat.
			assert_noop!(
				VaultsPallet::vault_key_rotated(
					RuntimeOrigin::root(),
					expected_activation_block,
					TX_HASH,
				),
				Error::<MockRuntime, _>::InvalidRotationStatus
			);

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
				public_key, NEW_AGG_PUB_KEY_POST_HANDOVER,
				"we should have the new public key in the new vault for the next epoch"
			);
			assert_eq!(
				active_from_block,
				expected_activation_block.saturating_add(1),
				"we should have set the starting point for the new vault's active window as the next
				after the reported block number"
			);

			// Status is complete.
			assert_eq!(
				PendingVaultRotation::<MockRuntime, _>::get(),
				Some(VaultRotationStatus::Complete),
			);
			assert_last_event!(crate::Event::VaultRotationCompleted { .. });
		});
	}

	#[test]
	fn non_optimistic_activation() {
		let mut ext = setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER));
		ext.execute_with(|| {
			MockOptimisticActivation::set(false);
			VaultsPallet::activate();

			assert!(matches!(
				PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
				VaultRotationStatus::AwaitingActivation { .. }
			));

			assert_ok!(VaultsPallet::vault_key_rotated(
				RuntimeOrigin::root(),
				ACTIVATION_BLOCK_NUMBER,
				TX_HASH,
			));
		});
		final_checks(&mut ext, ACTIVATION_BLOCK_NUMBER);
	}

	#[test]
	fn optimistic_activation() {
		let mut ext = setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER));
		ext.execute_with(|| {
			MockOptimisticActivation::set(true);
			VaultsPallet::activate();

			// No need to call vault_key_rotated.
			assert_noop!(
				VaultsPallet::vault_key_rotated(
					RuntimeOrigin::root(),
					ACTIVATION_BLOCK_NUMBER,
					TX_HASH,
				),
				Error::<MockRuntime, _>::InvalidRotationStatus
			);

			assert!(matches!(
				PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
				VaultRotationStatus::Complete,
			));
		});
		final_checks(&mut ext, HANDOVER_ACTIVATION_BLOCK);
	}

	#[test]
	fn handover_failure() {
		let mut ext = setup(Err(Default::default()));
		ext.execute_with(|| {
			assert!(matches!(
				PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
				VaultRotationStatus::KeyHandoverFailed { .. }
			));

			// Start handover again, but successful this time.
			let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());
			VaultsPallet::key_handover(
				btree_candidates.clone(),
				btree_candidates.clone(),
				<MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1,
			);

			for candidate in btree_candidates {
				assert_ok!(VaultsPallet::report_key_handover_outcome(
					RuntimeOrigin::signed(candidate),
					current_ceremony_id(),
					Ok(NEW_AGG_PUB_KEY_POST_HANDOVER),
				));
			}

			VaultsPallet::on_initialize(1);

			MockOptimisticActivation::set(true);
			VaultsPallet::activate();
		});
		final_checks(&mut ext, HANDOVER_ACTIVATION_BLOCK);
	}
}

#[test]
fn test_vault_key_rotated_externally_triggers_code_red() {
	new_test_ext().execute_with(|| {
		const TX_HASH: [u8; 4] = [0xab; 4];
		assert_eq!(<MockRuntimeSafeMode as Get<MockRuntimeSafeMode>>::get(), SafeMode::CODE_GREEN);
		assert_ok!(VaultsPallet::vault_key_rotated_externally(
			RuntimeOrigin::root(),
			NEW_AGG_PUB_KEY_POST_HANDOVER,
			1,
			TX_HASH,
		));
		assert_eq!(<MockRuntimeSafeMode as Get<MockRuntimeSafeMode>>::get(), SafeMode::CODE_RED);
		assert_last_event!(crate::Event::VaultRotatedExternally(..));
	});
}

#[test]
fn key_unavailable_on_activate_returns_governance_event() {
	new_test_ext_no_key().execute_with(|| {
		PendingVaultRotation::put(VaultRotationStatus::<MockRuntime, _>::KeyHandoverComplete {
			new_public_key: NEW_AGG_PUB_KEY_POST_HANDOVER,
		});

		VaultsPallet::activate();

		assert_last_event!(crate::Event::AwaitingGovernanceActivation { .. });

		// we're awaiting the governance action, so we are pending from
		// perspective of an outside observer (e.g. the validator pallet)
		assert_eq!(VaultsPallet::status(), AsyncResult::Pending);
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

#[test]
fn when_set_agg_key_with_agg_key_not_required_we_skip_to_completion() {
	new_test_ext().execute_with(|| {
		PendingVaultRotation::put(VaultRotationStatus::<MockRuntime, _>::KeyHandoverComplete {
			new_public_key: NEW_AGG_PUB_KEY_POST_HANDOVER,
		});

		MockSetAggKeyWithAggKey::set_required(false);

		VaultsPallet::activate();

		assert!(matches!(
			PendingVaultRotation::<MockRuntime, _>::get().unwrap(),
			VaultRotationStatus::Complete
		))
	})
}

#[test]
fn dont_slash_in_safe_mode() {
	new_test_ext().execute_with(|| {
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			vault: crate::PalletSafeMode { slashing_enabled: false },
		});
		keygen_failure(&[BOB, CHARLIE]);
		assert!(MockSlasher::slash_count(BOB) == 0);
		assert!(MockSlasher::slash_count(CHARLIE) == 0);

		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			vault: crate::PalletSafeMode { slashing_enabled: true },
		});
		keygen_failure(&[BOB, CHARLIE]);
		assert!(MockSlasher::slash_count(BOB) == 1);
		assert!(MockSlasher::slash_count(CHARLIE) == 1);
	});
}

fn do_full_key_rotation() {
	let rotation_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
	// Start Key gen
	<VaultsPallet as VaultRotator>::keygen(
		BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
		rotation_epoch,
	);
	let keygen_ceremony_id = current_ceremony_id();

	for p in ALL_CANDIDATES {
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(*p),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));
	}

	// Key verification
	VaultsPallet::on_initialize(2);
	EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));

	// Key handover
	const HANDOVER_PARTICIPANTS: [u64; 2] = [ALICE, BOB];
	VaultsPallet::key_handover(
		BTreeSet::from(HANDOVER_PARTICIPANTS),
		BTreeSet::from(HANDOVER_PARTICIPANTS),
		rotation_epoch,
	);

	let handover_ceremony_id = current_ceremony_id();
	for p in HANDOVER_PARTICIPANTS {
		assert_ok!(VaultsPallet::report_key_handover_outcome(
			RuntimeOrigin::signed(p),
			handover_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)
		));
	}
	VaultsPallet::on_initialize(3);

	// Key activation
	VaultsPallet::activate();

	assert_last_event!(crate::Event::VaultRotationCompleted);
	assert_eq!(PendingVaultRotation::<MockRuntime, _>::get(), Some(VaultRotationStatus::Complete));
	assert_eq!(VaultsPallet::status(), AsyncResult::Ready(VaultStatus::RotationComplete));
}

#[test]
fn can_recover_from_abort_vault_rotation_after_failed_key_gen() {
	new_test_ext().execute_with(|| {
		MockOptimisticActivation::set(true);
		let rotation_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);
		let keygen_ceremony_id = current_ceremony_id();

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(BOB),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(CHARLIE),
			keygen_ceremony_id,
			Err(Default::default())
		));
		VaultsPallet::on_initialize(2);
		matches!(
			PendingVaultRotation::<MockRuntime, _>::get(),
			Some(VaultRotationStatus::Failed { .. })
		);

		// Abort the vault rotation now
		VaultsPallet::abort_vault_rotation();

		assert!(PendingVaultRotation::<MockRuntime, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 0);
		assert_eq!(VaultsPallet::status(), AsyncResult::Void);

		// Can restart the vault rotation and succeed.
		do_full_key_rotation();
	})
}

#[test]
fn can_recover_from_abort_vault_rotation_after_key_verification() {
	new_test_ext().execute_with(|| {
		MockOptimisticActivation::set(true);
		let rotation_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);
		let keygen_ceremony_id = current_ceremony_id();

		for p in ALL_CANDIDATES {
			assert_ok!(VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(*p),
				keygen_ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			));
		}

		VaultsPallet::on_initialize(1);
		EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));
		matches!(
			PendingVaultRotation::<MockRuntime, _>::get(),
			Some(VaultRotationStatus::KeygenVerificationComplete { .. })
		);

		// Abort the vault rotation now
		VaultsPallet::abort_vault_rotation();

		assert!(PendingVaultRotation::<MockRuntime, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 0);
		assert_eq!(VaultsPallet::status(), AsyncResult::Void);

		// Can restart the vault rotation and succeed.
		do_full_key_rotation();
	})
}

#[test]
fn can_recover_from_abort_vault_rotation_after_key_handover_failed() {
	new_test_ext().execute_with(|| {
		MockOptimisticActivation::set(true);
		let rotation_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index() + 1;
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);
		let keygen_ceremony_id = current_ceremony_id();
		for p in ALL_CANDIDATES {
			assert_ok!(VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(*p),
				keygen_ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			));
		}

		VaultsPallet::on_initialize(1);
		EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));

		// Key handover
		const HANDOVER_PARTICIPANTS: [u64; 2] = [ALICE, BOB];
		VaultsPallet::key_handover(
			BTreeSet::from(HANDOVER_PARTICIPANTS),
			BTreeSet::from(HANDOVER_PARTICIPANTS),
			rotation_epoch,
		);

		let handover_ceremony_id = current_ceremony_id();
		assert_ok!(VaultsPallet::report_key_handover_outcome(
			RuntimeOrigin::signed(ALICE),
			handover_ceremony_id,
			Err(Default::default())
		));
		assert_ok!(VaultsPallet::report_key_handover_outcome(
			RuntimeOrigin::signed(BOB),
			handover_ceremony_id,
			Err(Default::default())
		));

		VaultsPallet::on_initialize(2);
		matches!(
			PendingVaultRotation::<MockRuntime, _>::get(),
			Some(VaultRotationStatus::KeyHandoverFailed { .. })
		);

		// Abort the vault rotation now
		VaultsPallet::abort_vault_rotation();

		assert!(PendingVaultRotation::<MockRuntime, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<MockRuntime, _>::get(), 0);
		assert_eq!(VaultsPallet::status(), AsyncResult::Void);

		// Can restart the vault rotation and succeed.
		do_full_key_rotation();
	})
}
