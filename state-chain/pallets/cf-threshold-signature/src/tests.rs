use core::marker::PhantomData;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
	mock::*, AttemptCount, AuthorityCount, CeremonyContext, CeremonyId, CurrentEpochIndex, Error,
	Event as PalletEvent, KeyHandoverResolutionPendingSince, KeyRotationStatus,
	KeygenFailureVoters, KeygenOutcomeFor, KeygenResolutionPendingSince, KeygenResponseTimeout,
	KeygenSuccessVoters, PalletOffence, PendingKeyRotation, RequestContext, RequestId,
	ThresholdSignatureResponseTimeout,
};

use cf_chains::mocks::{MockAggKey, MockEthereumChainCrypto};
use cf_primitives::GENESIS_EPOCH;
use cf_test_utilities::{last_event, maybe_last_event};
use cf_traits::{
	mocks::{
		cfe_interface_mock::{MockCfeEvent, MockCfeInterface},
		signer_nomination::MockNominator,
	},
	AccountRoleRegistry, AsyncResult, Chainflip, EpochInfo, EpochKey, KeyProvider,
	KeyRotationStatusOuter, KeyRotator, SetSafeMode, VaultActivator,
};
pub use frame_support::traits::Get;

use cfe_events::{KeyHandoverRequest, KeygenRequest, ThresholdSignatureRequest};
use frame_support::{
	assert_err, assert_noop, assert_ok,
	instances::Instance1,
	pallet_prelude::DispatchResultWithPostInfo,
	traits::{Hooks, OnInitialize},
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::BlockNumberProvider;

const ALL_CANDIDATES: &[<Test as Chainflip>::ValidatorId] = &[ALICE, BOB, CHARLIE];

// assert an arbitrary number of last events with the last one first and going in reverse from
// there.
macro_rules! assert_last_events {
	($($pat:pat),*) => {
		let mut events = frame_system::Pallet::<Test>::events();
		$(let event = events.pop().map(|e| e.event).unwrap();
		assert!(
			matches!(event, $crate::mock::RuntimeEvent::EvmThresholdSigner($pat)),
			"Unexpected event {:?}",
			event
		);)*
	};
}

fn get_ceremony_context(
	ceremony_id: CeremonyId,
	expected_request_id: RequestId,
	expected_attempt: AttemptCount,
) -> CeremonyContext<Test, Instance1> {
	let CeremonyContext::<Test, Instance1> {
		request_context: RequestContext { request_id, attempt_count, .. },
		..
	} = EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
	assert_eq!(request_id, expected_request_id);
	assert_eq!(attempt_count, expected_attempt);
	EvmThresholdSigner::pending_ceremonies(ceremony_id)
		.unwrap_or_else(|| panic!("Expected a ceremony with id {ceremony_id:?}"))
}

#[derive(Debug, PartialEq, Eq)]
enum CfeBehaviour {
	Success,
	Timeout,
	ReportFailure(Vec<u64>),
}

struct MockCfe {
	id: u64,
	behaviour: CfeBehaviour,
}

fn run_cfes_on_sc_events(cfes: &[MockCfe]) {
	let events = MockCfeInterface::take_events();
	for event in events {
		for cfe in cfes {
			cfe.process_event(event.clone());
		}
	}
}

type ValidatorId = <Test as Chainflip>::ValidatorId;

impl MockCfe {
	fn process_event(&self, event: MockCfeEvent<ValidatorId>) {
		if let MockCfeEvent::EvmThresholdSignatureRequest(ThresholdSignatureRequest {
			ceremony_id,
			epoch_index: _,
			key,
			signatories,
			payload,
		}) = event
		{
			match &self.behaviour {
				CfeBehaviour::Success => {
					// Wrong request id is a no-op
					assert_noop!(
						EvmThresholdSigner::signature_success(
							RuntimeOrigin::none(),
							ceremony_id + 1,
							sign(payload, key)
						),
						Error::<Test, Instance1>::InvalidThresholdSignatureCeremonyId
					);

					assert_ok!(EvmThresholdSigner::signature_success(
						RuntimeOrigin::none(),
						ceremony_id,
						sign(payload, key),
					));
				},
				CfeBehaviour::ReportFailure(bad) => {
					// Invalid ceremony id.
					assert_noop!(
						EvmThresholdSigner::report_signature_failed(
							RuntimeOrigin::signed(self.id),
							ceremony_id * 2,
							BTreeSet::from_iter(bad.clone()),
						),
						Error::<Test, Instance1>::InvalidThresholdSignatureCeremonyId
					);

					// Unsolicited responses are rejected.
					assert_noop!(
						EvmThresholdSigner::report_signature_failed(
							RuntimeOrigin::signed(signatories.iter().max().unwrap() + 1),
							ceremony_id,
							BTreeSet::from_iter(bad.clone()),
						),
						Error::<Test, Instance1>::InvalidThresholdSignatureRespondent
					);

					assert_ok!(EvmThresholdSigner::report_signature_failed(
						RuntimeOrigin::signed(self.id),
						ceremony_id,
						BTreeSet::from_iter(bad.clone()),
					));

					// Can't respond twice.
					assert_noop!(
						EvmThresholdSigner::report_signature_failed(
							RuntimeOrigin::signed(self.id),
							ceremony_id,
							BTreeSet::from_iter(bad.clone()),
						),
						Error::<Test, Instance1>::InvalidThresholdSignatureRespondent
					);
				},
				CfeBehaviour::Timeout => {
					// Oops
				},
			};
		};
	}
}

#[test]
fn happy_path_no_callback() {
	const NOMINEES: [u64; 2] = [1, 2];
	const AUTHORITIES: [u64; 3] = [1, 2, 3];
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.execute_with_consistency_checks(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> { request_context, .. } =
				EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			run_cfes_on_sc_events(&[cfe]);

			// Request is complete
			assert!(EvmThresholdSigner::pending_ceremonies(ceremony_id).is_none());

			// Signature is available
			assert!(matches!(
				EvmThresholdSigner::signature(request_context.request_id),
				AsyncResult::Ready(..)
			));

			// No callback was provided.
			assert!(!MockCallback::has_executed(request_context.request_id));
		});
}

#[test]
fn happy_path_with_callback() {
	const NOMINEES: [u64; 2] = [1, 2];
	const AUTHORITIES: [u64; 3] = [1, 2, 3];
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request_and_callback(b"OHAI", MockCallback::new)
		.execute_with_consistency_checks(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> { request_context, .. } =
				EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			run_cfes_on_sc_events(&[cfe]);

			// Request is complete
			assert!(EvmThresholdSigner::pending_ceremonies(ceremony_id).is_none());

			// Callback has triggered.
			assert!(MockCallback::has_executed(request_context.request_id));

			// Signature has been consumed.
			assert!(
				matches!(
					EvmThresholdSigner::signature(request_context.request_id),
					AsyncResult::Void
				),
				"Expected Void, got {:?}",
				EvmThresholdSigner::signature(request_context.request_id)
			);
		});
}

#[test]
fn signature_success_can_only_succeed_once_per_request() {
	const NOMINEES: [u64; 2] = [1, 2];
	const AUTHORITIES: [u64; 3] = [1, 2, 3];
	const PAYLOAD: &[u8; 4] = b"OHAI";
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request_and_callback(PAYLOAD, MockCallback::new)
		.execute_with_consistency_checks(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> { request_context, .. } =
				EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			assert_eq!(MockCallback::times_called(), 0);
			// report signature success
			run_cfes_on_sc_events(&[MockCfe { id: 1, behaviour: CfeBehaviour::Success }]);

			assert!(MockCallback::has_executed(request_context.request_id));
			assert_eq!(MockCallback::times_called(), 1);

			// Submit the same success again
			assert_err!(
				EvmThresholdSigner::signature_success(
					RuntimeOrigin::none(),
					ceremony_id,
					sign(*PAYLOAD, current_agg_key())
				),
				Error::<Test, Instance1>::InvalidThresholdSignatureCeremonyId
			);
			assert_eq!(MockCallback::times_called(), 1);
		});
}

// The assumption here is that when we don't want to retry, it's a special case, and the error will
// be handled by the callback itself, allowing a more custom failure logic than simply "retrying".
#[test]
fn keygen_verification_ceremony_calls_callback_on_failure() {
	const NOMINEES: [u64; 2] = [1, 2];
	const AUTHORITIES: [u64; 3] = [1, 2, 3];
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.execute_with_consistency_checks(|| {
			let EpochKey { key, .. } = EvmThresholdSigner::active_epoch_key().unwrap();
			let request_id = EvmThresholdSigner::trigger_key_verification(
				key,
				NOMINEES.into_iter().collect(),
				false,
				0,
				MockCallback::new,
				KeyRotationStatus::<Test, _>::AwaitingKeygenVerification { new_public_key: key },
			);

			// Callback was just registered, so cannot have been called.
			assert!(!MockCallback::has_executed(request_id));
			assert_eq!(MockCallback::times_called(), 0);

			let cfes = NOMINEES
				.iter()
				.map(|id| MockCfe { id: *id, behaviour: CfeBehaviour::ReportFailure(vec![]) })
				.collect::<Vec<_>>();
			run_cfes_on_sc_events(&cfes);

			<AllPalletsWithSystem as OnInitialize<_>>::on_initialize(
				System::current_block_number() + 1,
			);

			assert!(MockCallback::has_executed(request_id));
			assert_eq!(MockCallback::times_called(), 1);
		});
}

#[test]
fn fail_path_with_timeout() {
	const NOMINEES: [u64; 2] = [1, 2];
	const AUTHORITIES: [u64; 3] = [1, 2, 3];
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.execute_with_consistency_checks(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context: RequestContext { request_id, attempt_count, .. },
				..
			} = EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfes = [
				MockCfe { id: 1, behaviour: CfeBehaviour::Timeout },
				MockCfe { id: 2, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
			];

			// CFEs respond
			run_cfes_on_sc_events(&cfes[..]);

			// Request is still pending waiting for account 1 to respond.

			// Account 1 has 1 blame vote against it.
			assert_eq!(
				EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap().blame_counts,
				BTreeMap::from_iter([(1, 1)])
			);

			// Callback has *not* executed but is scheduled for a retry after the timeout has
			// elapsed.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				EvmThresholdSigner::threshold_signature_response_timeout();

			assert!(!MockCallback::has_executed(request_id));
			assert_eq!(EvmThresholdSigner::ceremony_retry_queues(retry_block).len(), 1);

			// The offender has not yet been reported.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![]);

			// Process retries.
			System::set_block_number(retry_block);
			<AllPalletsWithSystem as OnInitialize<_>>::on_initialize(retry_block);

			// Expect the retry queue for this block to be empty.
			assert!(EvmThresholdSigner::ceremony_retry_queues(retry_block).is_empty());
			// Another timeout should have been added for the new ceremony.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				EvmThresholdSigner::threshold_signature_response_timeout();
			assert!(!EvmThresholdSigner::ceremony_retry_queues(retry_block).is_empty());

			// Participant 1 was reported for not responding.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![1]);

			// We have a new request pending: New ceremony_id, same request context.
			assert_eq!(
				get_ceremony_context(ceremony_id + 1, request_id, attempt_count + 1)
					.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap())
			);
		});
}

#[test]
fn fail_path_due_to_report_signature_failed() {
	const NOMINEES: [u64; 5] = [1, 2, 3, 4, 5];
	const AUTHORITIES: [u64; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.execute_with_consistency_checks(|| {
			// progress by one block *after* the initial request is inserted (in the ExtBuilder)
			System::set_block_number(frame_system::Pallet::<Test>::current_block_number() + 1);
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context: RequestContext { request_id, attempt_count, .. },
				..
			} = EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfes = [(1, vec![]), (2, vec![1]), (3, vec![1]), (4, vec![1]), (5, vec![1])]
				.into_iter()
				.map(|(id, report)| MockCfe { id, behaviour: CfeBehaviour::ReportFailure(report) })
				.collect::<Vec<_>>();

			// CFEs responds, triggering a retry for the next block.
			run_cfes_on_sc_events(&cfes[..]);
			let next_block_retry = frame_system::Pallet::<Test>::current_block_number() + 1;
			let timeout_block_for_next_retry =
				next_block_retry + EvmThresholdSigner::threshold_signature_response_timeout();

			assert_eq!(EvmThresholdSigner::ceremony_retry_queues(next_block_retry).len(), 1);

			// Account 1 has 4 blame votes against it. The other accounts have no votes against
			// them.
			assert_eq!(
				EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap().blame_counts,
				BTreeMap::from_iter([(1, 4)])
			);

			// after the block is process, we of course have moved to the next block.
			System::set_block_number(next_block_retry);
			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(next_block_retry);

			// We did reach the reporting threshold, participant 1 was reported.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![1]);

			assert!(!MockCallback::has_executed(request_id));
			assert!(EvmThresholdSigner::ceremony_retry_queues(next_block_retry).is_empty());

			assert_eq!(
				EvmThresholdSigner::ceremony_retry_queues(timeout_block_for_next_retry).len(),
				1
			);

			assert_eq!(
				get_ceremony_context(ceremony_id + 1, request_id, attempt_count + 1)
					.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap())
			);

			System::set_block_number(timeout_block_for_next_retry);
			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				timeout_block_for_next_retry,
			);
			assert!(
				EvmThresholdSigner::ceremony_retry_queues(timeout_block_for_next_retry).is_empty()
			);

			assert_eq!(
				EvmThresholdSigner::ceremony_retry_queues(
					timeout_block_for_next_retry +
						EvmThresholdSigner::threshold_signature_response_timeout()
				)
				.len(),
				1
			);
		});
}

#[test]
fn test_not_enough_signers_for_threshold_schedules_retry() {
	const NOMINEES: [u64; 0] = [];
	const AUTHORITIES: [u64; 5] = [1, 2, 3, 4, 5];
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.execute_with_consistency_checks(|| {
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				<Test as crate::Config<Instance1>>::CeremonyRetryDelay::get();
			assert_eq!(EvmThresholdSigner::request_retry_queues(retry_block).len(), 1);
		});
}

#[test]
fn test_retries() {
	const NOMINEES: [u64; 4] = [1, 2, 3, 4];
	const AUTHORITIES: [u64; 5] = [1, 2, 3, 4, 5];
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.execute_with_consistency_checks(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context: RequestContext { request_id, attempt_count: first_attempt, .. },
				..
			} = EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();

			// Retry should re-use the same key.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				ThresholdSignatureResponseTimeout::<Test, _>::get();
			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);

			let retry_ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context:
					RequestContext { request_id: request_id_2, attempt_count: second_attempt, .. },
				..
			} = EvmThresholdSigner::pending_ceremonies(retry_ceremony_id).unwrap();
			assert_eq!(request_id, request_id_2);
			assert_eq!(second_attempt, first_attempt + 1);
			assert_eq!(retry_ceremony_id, ceremony_id + 1);
		});
}

#[test]
fn test_retries_for_immutable_key() {
	const NOMINEES: [u64; 4] = [1, 2, 3, 4];
	const AUTHORITIES: [u64; 5] = [1, 2, 3, 4, 5];
	new_test_ext()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.execute_with_consistency_checks(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context: RequestContext { request_id, attempt_count: first_attempt, .. },
				..
			} = EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();

			// Pretend the key has been updated to the next one.
			EvmThresholdSigner::set_key_for_epoch(
				CurrentEpochIndex::<Test>::get().saturating_add(1),
				MockAggKey(*b"NEXT"),
			);
			EvmThresholdSigner::activate_new_key();

			// Retry should re-use the original key.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				ThresholdSignatureResponseTimeout::<Test, _>::get();
			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);

			let retry_ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context:
					RequestContext { request_id: request_id_2, attempt_count: second_attempt, .. },
				key,
				..
			} = EvmThresholdSigner::pending_ceremonies(retry_ceremony_id).unwrap();
			assert_eq!(request_id, request_id_2);
			assert_eq!(second_attempt, first_attempt + 1);
			assert_eq!(retry_ceremony_id, ceremony_id + 1);
			assert_eq!(key, GENESIS_AGG_PUB_KEY);
		});
}

#[cfg(test)]
mod unsigned_validation {
	use super::*;
	use crate::{Call as PalletCall, CeremonyRetryQueues, PendingCeremonies};
	use cf_chains::{mocks::MockAggKey, ChainCrypto};
	use cf_traits::ThresholdSigner;
	use frame_support::{pallet_prelude::InvalidTransaction, unsigned::TransactionSource};
	use sp_runtime::traits::ValidateUnsigned;

	#[test]
	fn start_custom_signing_ceremony() {
		new_test_ext().execute_with_consistency_checks(|| {
			const CUSTOM_AGG_KEY: <MockEthereumChainCrypto as ChainCrypto>::AggKey =
				MockAggKey(*b"AKEY");

			let participants: BTreeSet<u64> = BTreeSet::from_iter([1, 2, 3, 4, 5, 6]);
			EvmThresholdSigner::trigger_key_verification(
				CUSTOM_AGG_KEY,
				participants,
				false,
				0,
				MockCallback::new,
				KeyRotationStatus::<Test, _>::AwaitingKeygenVerification {
					new_public_key: CUSTOM_AGG_KEY,
				},
			);
			let ceremony_id = current_ceremony_id();

			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				EvmThresholdSigner::threshold_signature_response_timeout();

			// Process retries.
			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);
			assert!(CeremonyRetryQueues::<Test, Instance1>::take(retry_block).is_empty());
			assert!(PendingCeremonies::<Test, Instance1>::take(ceremony_id).is_none());
		});
	}

	#[test]
	fn valid_unsigned_extrinsic() {
		const NOMINEES: [u64; 3] = [1, 2, 3];
		const AUTHORITIES: [u64; 5] = [1, 2, 3, 4, 5];
		new_test_ext()
			.with_authorities(AUTHORITIES)
			.with_nominees(NOMINEES)
			.execute_with_consistency_checks(|| {
				const PAYLOAD: <MockEthereumChainCrypto as ChainCrypto>::Payload = *b"OHAI";

				<EvmThresholdSigner as ThresholdSigner<_>>::request_signature(PAYLOAD);
				let ceremony_id = current_ceremony_id();
				let EpochKey { key: current_key, .. } =
					EvmThresholdSigner::active_epoch_key().unwrap();

				assert!(
					Test::validate_unsigned(
						TransactionSource::External,
						&PalletCall::signature_success {
							ceremony_id,
							signature: sign(PAYLOAD, current_agg_key())
						}
						.into(),
					)
					.is_ok(),
					"Validation Failed: {:?} / {:?}",
					EvmThresholdSigner::active_epoch_key(),
					current_key
				);
			});
	}

	#[test]
	fn reject_invalid_ceremony() {
		const NOMINEES: [u64; 3] = [1, 2, 3];
		const AUTHORITIES: [u64; 5] = [1, 2, 3, 4, 5];
		new_test_ext()
			.with_authorities(AUTHORITIES)
			.with_nominees(NOMINEES)
			.execute_with_consistency_checks(|| {
				const PAYLOAD: <MockEthereumChainCrypto as ChainCrypto>::Payload = *b"OHAI";
				<EvmThresholdSigner as ThresholdSigner<_>>::request_signature(PAYLOAD);
				assert_eq!(
					Test::validate_unsigned(
						TransactionSource::External,
						&PalletCall::signature_success {
							// Incorrect ceremony id
							ceremony_id: current_ceremony_id() + 1,
							signature: sign(PAYLOAD, current_agg_key())
						}
						.into()
					)
					.unwrap_err(),
					InvalidTransaction::Stale.into()
				);
			});
	}

	#[test]
	fn reject_invalid_signature() {
		const NOMINEES: [u64; 3] = [1, 2, 3];
		const AUTHORITIES: [u64; 5] = [1, 2, 3, 4, 5];
		new_test_ext()
			.with_authorities(AUTHORITIES)
			.with_nominees(NOMINEES)
			.with_request(b"OHAI")
			.execute_with_consistency_checks(|| {
				assert_eq!(
					Test::validate_unsigned(
						TransactionSource::External,
						&PalletCall::signature_success {
							ceremony_id: current_ceremony_id(),
							signature: INVALID_SIGNATURE
						}
						.into()
					)
					.unwrap_err(),
					InvalidTransaction::BadProof.into()
				);
			});
	}

	#[test]
	fn reject_invalid_call() {
		new_test_ext().execute_with_consistency_checks(|| {
			assert_eq!(
				EvmThresholdSigner::validate_unsigned(
					TransactionSource::External,
					&PalletCall::report_signature_failed {
						ceremony_id: 0,
						offenders: Default::default()
					}
				)
				.unwrap_err(),
				InvalidTransaction::Call.into()
			);
		});
	}

	#[test]
	fn can_only_report_candidates() {
		const NOMINEES: [u64; 3] = [1, 2, 3];
		const AUTHORITIES: [u64; 5] = [1, 2, 3, 4, 5];

		let valid_blames = BTreeSet::from_iter([NOMINEES[1], NOMINEES[2]]);
		// AUTHORITIES[4] is not a candidate in the ceremony and u64::MAX is not an id of any
		// authority.
		let invalid_blames = BTreeSet::from_iter([AUTHORITIES[4], u64::MAX]);

		new_test_ext()
			.with_authorities(AUTHORITIES)
			.with_nominees(NOMINEES)
			.with_request(b"OHAI")
			.execute_with_consistency_checks(|| {
				let ceremony_id = current_ceremony_id();
				EvmThresholdSigner::report_signature_failed(
					RuntimeOrigin::signed(NOMINEES[0]),
					ceremony_id,
					valid_blames.iter().cloned().chain(invalid_blames.clone()).collect(),
				)
				.unwrap();

				let CeremonyContext::<Test, Instance1> { blame_counts, .. } =
					EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
				let blamed: BTreeSet<_> = blame_counts.keys().cloned().collect();

				assert_eq!(&valid_blames, &blamed);
				assert!(invalid_blames.is_disjoint(&blamed));
			});
	}
}

#[cfg(test)]
mod failure_reporting {
	use super::*;
	use crate::{CeremonyContext, RequestContext, ThresholdCeremonyType};
	use cf_chains::ChainCrypto;

	fn init_context(
		validator_set: impl IntoIterator<Item = <Test as Chainflip>::ValidatorId> + Copy,
	) -> CeremonyContext<Test, Instance1> {
		const PAYLOAD: <MockEthereumChainCrypto as ChainCrypto>::Payload = *b"OHAI";
		MockEpochInfo::set_authorities(validator_set.into_iter().collect());
		CeremonyContext {
			request_context: RequestContext { request_id: 1, attempt_count: 0, payload: PAYLOAD },
			threshold_ceremony_type: ThresholdCeremonyType::Standard,
			epoch: 0,
			key: GENESIS_AGG_PUB_KEY,
			remaining_respondents: BTreeSet::from_iter(validator_set),
			blame_counts: Default::default(),
			candidates: BTreeSet::from_iter(validator_set),
		}
	}

	fn report(context: &mut CeremonyContext<Test, Instance1>, reporter: u64, blamed: Vec<u64>) {
		for i in blamed {
			*context.blame_counts.entry(i).or_default() += 1;
		}
		context.remaining_respondents.remove(&reporter);
	}

	#[test]
	fn basic_thresholds() {
		let mut ctx = init_context([1, 2, 3, 4, 5]);

		// Blame validators.
		report(&mut ctx, 1, vec![2]);
		report(&mut ctx, 2, vec![1]);
		report(&mut ctx, 3, vec![1]);

		// Status: 3 responses in, votes: [1:2, 2:1]
		// Vote threshold not met, but two validators have failed to respond - they would be
		// reported.
		assert_eq!(ctx.offenders(), vec![4, 5], "Context was {ctx:?}.");

		// Fourth report, reporting threshold passed.
		report(&mut ctx, 4, vec![1]);

		// Status: 4 responses in, votes: [1:3, 2:1]
		// Vote threshold has not been met for authority `1`, and `5` has not responded.
		// As things stand, [5] would be reported.
		assert_eq!(ctx.offenders(), vec![5], "Context was {ctx:?}.");

		// Fifth report, reporting threshold passed.
		report(&mut ctx, 5, vec![1, 2]);

		// Status: 5 responses in, votes: [1:4, 2:2]. Only 1 has met the vote threshold.
		assert_eq!(ctx.offenders(), vec![1], "Context was {ctx:?}.");
	}
}

#[test]
#[should_panic]
fn start_panics_with_no_candidates() {
	new_test_ext().execute_with(|| {
		<EvmThresholdSigner as KeyRotator>::keygen(BTreeSet::default(), GENESIS_EPOCH);
	});
}

#[test]
fn keygen_request_emitted() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index();
		<EvmThresholdSigner as KeyRotator>::keygen(btree_candidates.clone(), rotation_epoch);
		// Confirm we have a new key rotation process running
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
		let events = MockCfeInterface::take_events::<ValidatorId>();
		assert_eq!(
			events[0],
			MockCfeEvent::EvmKeygenRequest(KeygenRequest {
				ceremony_id: current_ceremony_id(),
				participants: btree_candidates.clone(),
				epoch_index: rotation_epoch,
			})
		);
		assert_eq!(
			last_event::<Test>(),
			PalletEvent::<Test, _>::KeygenRequest {
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
	let authorities = BTreeSet::from_iter(ALL_CANDIDATES.iter().take(2).cloned());
	let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().skip(1).take(2).cloned());

	new_test_ext().execute_with(|| {
		let current_epoch = <Test as Chainflip>::EpochInfo::epoch_index();
		let next_epoch = current_epoch + 1;

		PendingKeyRotation::<Test, _>::put(KeyRotationStatus::KeygenVerificationComplete {
			new_public_key: Default::default(),
		});
		let ceremony_id = current_ceremony_id();

		<EvmThresholdSigner as KeyRotator>::key_handover(
			authorities.clone(),
			candidates.clone(),
			next_epoch,
		);

		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
		let events = MockCfeInterface::take_events::<ValidatorId>();
		assert_eq!(
			events[0],
			MockCfeEvent::EthKeyHandoverRequest(KeyHandoverRequest {
				// It should be incremented when the request is made.
				ceremony_id: ceremony_id + 1,
				from_epoch: current_epoch,
				to_epoch: next_epoch,
				key_to_share: EvmThresholdSigner::active_epoch_key().unwrap().key,
				sharing_participants: authorities.clone(),
				receiving_participants: candidates.clone(),
				new_key: Default::default()
			})
		);
		assert_eq!(
			last_event::<Test>(),
			PalletEvent::<Test, _>::KeyHandoverRequest {
				// It should be incremented when the request is made.
				ceremony_id: ceremony_id + 1,
				from_epoch: current_epoch,
				key_to_share: EvmThresholdSigner::active_epoch_key().unwrap().key,
				sharing_participants: authorities,
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
fn start_panics_if_called_while_key_rotation_in_progress() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		<EvmThresholdSigner as KeyRotator>::keygen(btree_candidates.clone(), GENESIS_EPOCH);
		<EvmThresholdSigner as KeyRotator>::keygen(btree_candidates, GENESIS_EPOCH);
	});
}

#[test]
fn keygen_success_triggers_keygen_verification() {
	let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		let rotation_epoch_index = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
		<EvmThresholdSigner as KeyRotator>::keygen(candidates.clone(), rotation_epoch_index);
		let ceremony_id = current_ceremony_id();

		for candidate in &candidates {
			assert_ok!(EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(*candidate),
				ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER),
			));
		}

		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);

		assert!(matches!(
			PendingKeyRotation::<Test, _>::get().unwrap(),
			KeyRotationStatus::AwaitingKeygenVerification { .. }
		));
	});
}

#[test]
fn handover_success_triggers_handover_verification() {
	let authorities = BTreeSet::from_iter(ALL_CANDIDATES.iter().take(2).cloned());
	let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().skip(1).take(2).cloned());
	let all_participants: BTreeSet<_> = authorities.union(&candidates).copied().collect();

	new_test_ext().execute_with(|| {
		let rotation_epoch_index = <Test as Chainflip>::EpochInfo::epoch_index() + 1;

		PendingKeyRotation::<Test, _>::put(KeyRotationStatus::KeygenVerificationComplete {
			new_public_key: NEW_AGG_PUB_KEY_PRE_HANDOVER,
		});

		<EvmThresholdSigner as KeyRotator>::key_handover(
			authorities.clone(),
			candidates.clone(),
			rotation_epoch_index,
		);

		for candidate in &all_participants {
			assert_ok!(EvmThresholdSigner::report_key_handover_outcome(
				RuntimeOrigin::signed(*candidate),
				current_ceremony_id(),
				Ok(NEW_AGG_PUB_KEY_POST_HANDOVER),
			));
		}

		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);

		assert!(matches!(
			PendingKeyRotation::<Test, _>::get().unwrap(),
			KeyRotationStatus::AwaitingKeyHandoverVerification { .. }
		));
	});
}

fn keygen_failure(
	bad_candidates: impl IntoIterator<Item = <Test as Chainflip>::ValidatorId> + Clone,
) {
	EvmThresholdSigner::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()), GENESIS_EPOCH);

	let ceremony_id = current_ceremony_id();

	EvmThresholdSigner::terminate_rotation(
		bad_candidates.clone(),
		PalletEvent::KeygenFailure(ceremony_id),
	);

	assert_eq!(last_event::<Test>(), PalletEvent::KeygenFailure(ceremony_id).into());

	assert_eq!(
		EvmThresholdSigner::status(),
		AsyncResult::Ready(KeyRotationStatusOuter::Failed(
			bad_candidates.clone().into_iter().collect()
		))
	);

	MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, bad_candidates);
}

#[test]
fn test_keygen_failure() {
	new_test_ext().execute_with(|| {
		keygen_failure([BOB, CHARLIE]);
	});
}

// This happens when the threshold signer reports failure (through its status) to the validator
// pallet. Once all threshold signers have reported some AsyncResult::Ready status (see
// all_keys_rotator) then the validator pallet will call keygen() again
#[test]
fn keygen_called_after_keygen_failure_restarts_rotation_at_keygen() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
		keygen_failure([BOB, CHARLIE]);
		EvmThresholdSigner::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);

		assert_eq!(EvmThresholdSigner::status(), AsyncResult::Pending);

		assert_eq!(
			last_event::<Test>(),
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
	new_test_ext()
		.with_authorities((5u64..16).collect::<BTreeSet<_>>())
		.execute_with(|| {
			let participants = (5u64..15).collect::<BTreeSet<_>>();

			let rotation_epoch_index = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
			let keygen_ceremony_id = 12;

			let _request_id = EvmThresholdSigner::trigger_keygen_verification(
				keygen_ceremony_id,
				NEW_AGG_PUB_KEY_PRE_HANDOVER,
				participants.clone(),
				rotation_epoch_index,
			);

			let blamed = vec![5, 6, 7, 8];
			assert!(blamed.iter().all(|b| participants.contains(b)));

			let cfes = participants
				.iter()
				.map(|id| MockCfe {
					id: *id,
					behaviour: CfeBehaviour::ReportFailure(blamed.clone()),
				})
				.collect::<Vec<_>>();
			run_cfes_on_sc_events(&cfes);

			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				frame_system::Pallet::<Test>::current_block_number() +
					EvmThresholdSigner::threshold_signature_response_timeout(),
			);

			println!("{:?}", System::events());
			assert_last_events!(
				PalletEvent::ThresholdSignatureFailed { .. },
				PalletEvent::ThresholdDispatchComplete { .. },
				PalletEvent::KeygenVerificationFailure { .. }
			);
			MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, blamed.clone());
			assert_eq!(
				EvmThresholdSigner::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::Failed(blamed.into_iter().collect()))
			)
		});
}

#[test]
fn no_active_rotation() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				1,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<Test, _>::NoActiveRotation
		);

		assert_noop!(
			EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				1,
				Err(Default::default())
			),
			Error::<Test, _>::NoActiveRotation
		);

		assert_noop!(
			EvmThresholdSigner::report_key_handover_outcome(
				RuntimeOrigin::signed(ALICE),
				1,
				Err(Default::default())
			),
			Error::<Test, _>::NoActiveRotation
		);

		assert_noop!(
			EvmThresholdSigner::report_key_handover_outcome(
				RuntimeOrigin::signed(ALICE),
				1,
				Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)
			),
			Error::<Test, _>::NoActiveRotation
		);
	});
}

#[test]
fn cannot_report_keygen_success_twice() {
	new_test_ext().execute_with(|| {
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));

		// Can't report twice.
		assert_noop!(
			EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<Test, _>::InvalidKeygenRespondent
		);
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn cannot_report_two_different_keygen_outcomes() {
	new_test_ext().execute_with(|| {
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));

		// Can't report failure after reporting success
		assert_noop!(
			EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id,
				Err(BTreeSet::from_iter([BOB, CHARLIE]))
			),
			Error::<Test, _>::InvalidKeygenRespondent
		);
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn only_candidates_can_report_keygen_outcome() {
	new_test_ext().execute_with(|| {
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));

		// Only candidates can respond.
		let non_candidate = u64::MAX;
		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(
			&non_candidate,
		)
		.unwrap();
		assert!(!ALL_CANDIDATES.contains(&non_candidate));
		assert_noop!(
			EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(non_candidate),
				ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<Test, _>::InvalidKeygenRespondent
		);
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
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

		<EvmThresholdSigner as KeyRotator>::keygen(candidates, GENESIS_EPOCH);

		EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			current_ceremony_id(),
			// Report both the valid and invalid offenders
			Err(valid_blames.union(&invalid_blames).copied().collect()),
		)
		.unwrap();

		match PendingKeyRotation::<Test, _>::get().unwrap() {
			KeyRotationStatus::AwaitingKeygen { response_status, .. } => {
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
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));

		// Ceremony id in the past (not the pending one we're waiting for)
		assert_noop!(
			EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id - 1,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<Test, _>::InvalidKeygenCeremonyId
		);
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

		// Ceremony id in the future
		assert_noop!(
			EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id + 1,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<Test, _>::InvalidKeygenCeremonyId
		);
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn cannot_report_key_handover_outcome_when_awaiting_keygen() {
	new_test_ext().execute_with(|| {
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			<Test as Chainflip>::EpochInfo::epoch_index() + 1,
		);

		assert_noop!(
			EvmThresholdSigner::report_key_handover_outcome(
				RuntimeOrigin::signed(ALICE),
				current_ceremony_id(),
				Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)
			),
			Error::<Test, _>::InvalidRotationStatus
		);
	});
}

fn do_full_key_rotation() {
	let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
	<EvmThresholdSigner as KeyRotator>::keygen(
		BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
		rotation_epoch,
	);
	let keygen_ceremony_id = current_ceremony_id();

	assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 1);

	assert_ok!(EvmThresholdSigner::report_keygen_outcome(
		RuntimeOrigin::signed(ALICE),
		keygen_ceremony_id,
		Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
	));

	assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

	<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);
	// After on initialise we obviously still don't have enough votes.
	// So nothing should have changed.
	assert!(KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

	// Bob agrees.
	assert_ok!(EvmThresholdSigner::report_keygen_outcome(
		RuntimeOrigin::signed(BOB),
		keygen_ceremony_id,
		Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
	));

	// A resolution is still pending - we require 100% response rate.
	assert!(KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
	<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);
	assert!(KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

	// Charlie agrees.
	assert_ok!(EvmThresholdSigner::report_keygen_outcome(
		RuntimeOrigin::signed(CHARLIE),
		keygen_ceremony_id,
		Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
	));

	// This time we should have enough votes for consensus.
	assert!(KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
	if let KeyRotationStatus::AwaitingKeygen {
		ceremony_id: keygen_ceremony_id_from_status,
		response_status,
		keygen_participants,
		new_epoch_index,
	} = PendingKeyRotation::<Test, _>::get().unwrap()
	{
		assert_eq!(keygen_ceremony_id, keygen_ceremony_id_from_status);
		assert_eq!(
			response_status
				.success_votes()
				.get(&NEW_AGG_PUB_KEY_PRE_HANDOVER)
				.expect("new key should have votes"),
			&(ALL_CANDIDATES.len() as AuthorityCount)
		);
		assert_eq!(keygen_participants, BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()));
		assert_eq!(new_epoch_index, rotation_epoch);
	} else {
		panic!("Expected to be in AwaitingKeygen state");
	}
	<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);

	assert!(matches!(
		PendingKeyRotation::<Test, _>::get().unwrap(),
		KeyRotationStatus::AwaitingKeygenVerification { .. }
	));

	let cfes = [ALICE]
		.iter()
		.map(|id| MockCfe { id: *id, behaviour: CfeBehaviour::Success })
		.collect::<Vec<_>>();
	run_cfes_on_sc_events(&cfes);

	assert_eq!(
		<EvmThresholdSigner as KeyRotator>::status(),
		AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete)
	);

	assert!(matches!(
		PendingKeyRotation::<Test, _>::get().unwrap(),
		KeyRotationStatus::KeygenVerificationComplete { .. }
	));

	const SHARING_PARTICIPANTS: [u64; 2] = [ALICE, BOB];
	EvmThresholdSigner::key_handover(
		BTreeSet::from(SHARING_PARTICIPANTS),
		BTreeSet::from_iter(ALL_CANDIDATES.iter().copied()),
		rotation_epoch,
	);
	assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

	let handover_ceremony_id = current_ceremony_id();
	for p in ALL_CANDIDATES {
		assert_ok!(EvmThresholdSigner::report_key_handover_outcome(
			RuntimeOrigin::signed(*p),
			handover_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)
		));
	}

	<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);
	assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

	assert_last_events!(
		crate::Event::ThresholdSignatureRequest { .. },
		crate::Event::KeyHandoverSuccess { .. }
	);

	assert!(matches!(
		PendingKeyRotation::<Test, _>::get().unwrap(),
		KeyRotationStatus::AwaitingKeyHandoverVerification { .. }
	));

	run_cfes_on_sc_events(&cfes);

	assert_eq!(
		<EvmThresholdSigner as KeyRotator>::status(),
		AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete)
	);

	assert_last_events!(
		crate::Event::ThresholdDispatchComplete { .. },
		crate::Event::KeyHandoverVerificationSuccess { .. }
	);

	assert!(matches!(
		PendingKeyRotation::<Test, _>::get().unwrap(),
		KeyRotationStatus::KeyHandoverComplete { .. }
	));

	// Called by validator pallet
	EvmThresholdSigner::activate_keys();

	assert!(!KeygenResolutionPendingSince::<Test, _>::exists());
	// Voting has been cleared.
	assert_eq!(KeygenSuccessVoters::<Test, _>::iter_keys().next(), None);
	assert!(!KeygenFailureVoters::<Test, _>::exists());

	assert_eq!(
		<EvmThresholdSigner as KeyRotator>::status(),
		AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete)
	);
	MockVaultActivator::set_activation_completed();

	assert_last_events!(crate::Event::KeyRotationCompleted);
	assert_eq!(PendingKeyRotation::<Test, _>::get(), Some(KeyRotationStatus::Complete));
	assert_eq!(
		EvmThresholdSigner::status(),
		AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete)
	);
}

#[test]
fn keygen_report_success() {
	new_test_ext().execute_with(do_full_key_rotation);
}

#[test]
fn keygen_report_failure() {
	new_test_ext().execute_with(|| {
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 1);

		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

		// Bob agrees.
		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(BOB),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));

		// A resolution is still pending - we expect 100% response rate.
		assert!(KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);

		// Charlie agrees.
		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(CHARLIE),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(<EvmThresholdSigner as KeyRotator>::status(), AsyncResult::Pending);
		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);
		assert!(!KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(
			EvmThresholdSigner::status(),
			AsyncResult::Ready(KeyRotationStatusOuter::Failed(BTreeSet::from([CHARLIE])))
		);

		MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, vec![CHARLIE]);

		assert_last_events!(crate::Event::KeygenFailure(..));

		// Voting has been cleared.
		assert!(KeygenSuccessVoters::<Test, _>::iter_keys().next().is_none());
		assert!(!KeygenFailureVoters::<Test, _>::exists());
	});
}

fn test_key_ceremony_timeout_period<PendingSince, ReportFn>(report_fn: ReportFn)
where
	PendingSince: frame_support::StorageValue<BlockNumberFor<Test>, Query = BlockNumberFor<Test>>,
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
	<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);
	assert!(PendingSince::exists());
	<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
		MOCK_KEYGEN_RESPONSE_TIMEOUT,
	);
	assert!(PendingSince::exists());
	<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
		MOCK_KEYGEN_RESPONSE_TIMEOUT + 1,
	);
	assert!(!PendingSince::exists());

	// Too many candidates failed to report, so we report nobody.
	MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, vec![]);
}

#[test]
fn test_keygen_timeout_period() {
	new_test_ext().execute_with(|| {
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		test_key_ceremony_timeout_period::<KeygenResolutionPendingSince<Test, _>, _>(
			EvmThresholdSigner::report_keygen_outcome,
		)
	});
}

#[test]
fn test_key_handover_timeout_period() {
	new_test_ext().execute_with(|| {
		let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());
		PendingKeyRotation::<Test, _>::put(KeyRotationStatus::KeygenVerificationComplete {
			new_public_key: Default::default(),
		});
		<EvmThresholdSigner as KeyRotator>::key_handover(candidates.clone(), candidates, 2);
		test_key_ceremony_timeout_period::<KeyHandoverResolutionPendingSince<Test, _>, _>(
			EvmThresholdSigner::report_key_handover_outcome,
		)
	});
}

#[cfg(test)]
mod key_rotation {
	use cf_chains::mocks::{MockEthereum, BAD_AGG_KEY_POST_HANDOVER};
	use cf_traits::mocks::block_height_provider::BlockHeightProvider;

	use crate::Keys;

	use super::*;

	const AUTHORITIES: [u64; 3] = [1, 2, 3];
	const CANDIDATES: [u64; 2] = [1, 2];

	fn setup(key_handover_outcome: KeygenOutcomeFor<Test, Instance1>) -> TestRunner<()> {
		let ext = new_test_ext().with_authorities(AUTHORITIES);
		ext.execute_with(|| {
			let authorities = BTreeSet::from_iter(AUTHORITIES.iter().cloned());
			let candidates = BTreeSet::from_iter(CANDIDATES.iter().cloned());

			let rotation_epoch_index = <Test as Chainflip>::EpochInfo::epoch_index() + 1;

			assert_noop!(
				EvmThresholdSigner::report_keygen_outcome(
					RuntimeOrigin::signed(ALICE),
					Default::default(),
					Err(Default::default()),
				),
				Error::<Test, _>::NoActiveRotation
			);

			<EvmThresholdSigner as KeyRotator>::keygen(candidates.clone(), GENESIS_EPOCH);
			let ceremony_id = current_ceremony_id();
			EvmThresholdSigner::trigger_keygen_verification(
				ceremony_id,
				NEW_AGG_PUB_KEY_PRE_HANDOVER,
				candidates.clone(),
				rotation_epoch_index,
			);

			let cfes = [ALICE]
				.iter()
				.map(|id| MockCfe { id: *id, behaviour: CfeBehaviour::Success })
				.collect::<Vec<_>>();
			run_cfes_on_sc_events(&cfes);

			EvmThresholdSigner::key_handover(
				authorities.clone(),
				candidates.clone(),
				rotation_epoch_index,
			);

			// Note that we require all participants to respond
			for candidate in authorities.union(&candidates) {
				assert_ok!(EvmThresholdSigner::report_key_handover_outcome(
					RuntimeOrigin::signed(*candidate),
					current_ceremony_id(),
					key_handover_outcome.clone()
				));
			}

			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);
		})
	}

	fn final_checks(ext: TestRunner<()>) {
		ext.execute_with(|| {
			let current_epoch = <Test as Chainflip>::EpochInfo::epoch_index();

			assert_eq!(
				Keys::<Test, _>::get(current_epoch).expect("key should exist"),
				GENESIS_AGG_PUB_KEY,
				"we should have the genesis key here"
			);

			// The next epoch
			let next_epoch = current_epoch + 1;

			assert_eq!(
				Keys::<Test, _>::get(next_epoch).expect("key should exist in the next epoch"),
				NEW_AGG_PUB_KEY_POST_HANDOVER,
				"we should have the new public key for the next epoch"
			);

			// Status is complete.
			assert_eq!(PendingKeyRotation::<Test, _>::get(), Some(KeyRotationStatus::Complete));
			assert_last_events!(crate::Event::KeyRotationCompleted { .. });
		});
	}

	#[test]
	fn optimistic_activation() {
		const HANDOVER_ACTIVATION_BLOCK: u64 = 420;
		let ext = setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)).execute_with(|| {
			let cfes = [CANDIDATES[0]]
				.iter()
				.map(|id| MockCfe { id: *id, behaviour: CfeBehaviour::Success })
				.collect::<Vec<_>>();
			run_cfes_on_sc_events(&cfes);

			BlockHeightProvider::<MockEthereum>::set_block_height(HANDOVER_ACTIVATION_BLOCK);
			EvmThresholdSigner::activate_keys();
			EvmThresholdSigner::status();

			assert!(matches!(
				PendingKeyRotation::<Test, _>::get().unwrap(),
				KeyRotationStatus::Complete,
			));
		});
		final_checks(ext);
	}

	#[test]
	fn can_recover_after_handover_failure() {
		let ext = setup(Err(Default::default())).execute_with(|| {
			assert!(matches!(
				PendingKeyRotation::<Test, _>::get().unwrap(),
				KeyRotationStatus::KeyHandoverFailed { .. }
			));

			// Start handover again, but successful this time.
			let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());
			EvmThresholdSigner::key_handover(
				btree_candidates.clone(),
				btree_candidates.clone(),
				<Test as Chainflip>::EpochInfo::epoch_index() + 1,
			);

			for candidate in btree_candidates {
				assert_ok!(EvmThresholdSigner::report_key_handover_outcome(
					RuntimeOrigin::signed(candidate),
					current_ceremony_id(),
					Ok(NEW_AGG_PUB_KEY_POST_HANDOVER),
				));
			}

			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);

			let cfes = [ALICE]
				.iter()
				.map(|id| MockCfe { id: *id, behaviour: CfeBehaviour::Success })
				.collect::<Vec<_>>();
			run_cfes_on_sc_events(&cfes);

			EvmThresholdSigner::activate_keys();
			EvmThresholdSigner::status();
		});

		final_checks(ext);
	}

	#[test]
	fn key_handover_success_triggers_key_handover_verification() {
		setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)).execute_with(|| {
			assert!(matches!(
				PendingKeyRotation::<Test, _>::get(),
				Some(KeyRotationStatus::AwaitingKeyHandoverVerification { .. })
			));
		});
	}

	#[test]
	fn key_handover_fails_on_key_mismatch() {
		setup(Ok(BAD_AGG_KEY_POST_HANDOVER)).execute_with(|| {
			assert_last_events!(crate::Event::KeyHandoverFailure { .. });
			assert!(matches!(
				PendingKeyRotation::<Test, _>::get(),
				Some(KeyRotationStatus::KeyHandoverFailed { .. })
			));
		});
	}

	#[test]
	fn can_recover_after_key_handover_verification_failure() {
		setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)).execute_with(|| {
			let offenders = vec![CANDIDATES[0]];

			let cfes = CANDIDATES
				.iter()
				.map(|id| MockCfe {
					id: *id,
					behaviour: CfeBehaviour::ReportFailure(offenders.clone()),
				})
				.collect::<Vec<_>>();
			run_cfes_on_sc_events(&cfes);

			<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				frame_system::Pallet::<Test>::current_block_number() +
					EvmThresholdSigner::threshold_signature_response_timeout(),
			);

			assert_last_events!(
				PalletEvent::ThresholdSignatureFailed { .. },
				PalletEvent::ThresholdDispatchComplete { .. },
				PalletEvent::KeyHandoverVerificationFailure { .. }
			);

			MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, offenders.clone());

			let offenders = BTreeSet::from_iter(offenders);
			assert_eq!(
				EvmThresholdSigner::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::Failed(offenders.clone()))
			);

			assert_eq!(
				PendingKeyRotation::<Test, _>::get(),
				Some(KeyRotationStatus::Failed { offenders })
			);

			// Can restart the key rotation and succeed.
			do_full_key_rotation();
		});
	}

	#[test]
	fn wait_for_activating_key_tss_before_completing_rotation() {
		let ext = setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)).execute_with(|| {
			// KeyHandoverComplete
			PendingKeyRotation::<Test, _>::put(KeyRotationStatus::KeyHandoverComplete {
				new_public_key: NEW_AGG_PUB_KEY_POST_HANDOVER,
			});
			// Start ActivatingKeys
			EvmThresholdSigner::activate_keys();
			assert_eq!(
				PendingKeyRotation::<Test, _>::get().unwrap(),
				KeyRotationStatus::AwaitingActivation {
					request_ids: vec![4],
					new_public_key: NEW_AGG_PUB_KEY_POST_HANDOVER
				}
			);

			// Vault activation started and it is now pending
			assert_eq!(MockVaultActivator::status(), AsyncResult::Pending);

			// Request is complete
			assert_eq!(EvmThresholdSigner::signature(4), AsyncResult::Void);

			// Proceed to complete activation
			EvmThresholdSigner::status();

			assert_eq!(MockVaultActivator::status(), AsyncResult::Ready(()));
		});
		final_checks(ext);
	}
}

#[test]
fn set_keygen_response_timeout_works() {
	new_test_ext_no_key().execute_with(|| {
		let init_timeout = KeygenResponseTimeout::<Test, _>::get();

		EvmThresholdSigner::set_keygen_response_timeout(RuntimeOrigin::root(), init_timeout)
			.unwrap();

		assert!(maybe_last_event::<Test>().is_none());

		let new_timeout = init_timeout + 1;

		EvmThresholdSigner::set_keygen_response_timeout(RuntimeOrigin::root(), new_timeout)
			.unwrap();

		assert_last_events!(crate::Event::KeygenResponseTimeoutUpdated { .. });
		assert_eq!(KeygenResponseTimeout::<Test, _>::get(), new_timeout)
	});
}

#[test]
fn dont_slash_in_safe_mode() {
	new_test_ext().execute_with(|| {
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			threshold_signature: crate::PalletSafeMode {
				slashing_enabled: false,
				_phantom: PhantomData,
			},
		});
		keygen_failure([BOB, CHARLIE]);
		assert!(MockSlasher::slash_count(BOB) == 0);
		assert!(MockSlasher::slash_count(CHARLIE) == 0);

		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			threshold_signature: crate::PalletSafeMode {
				slashing_enabled: true,
				_phantom: PhantomData,
			},
		});
		keygen_failure([BOB, CHARLIE]);
		assert!(MockSlasher::slash_count(BOB) == 1);
		assert!(MockSlasher::slash_count(CHARLIE) == 1);
	});
}

#[test]
fn can_recover_from_abort_key_rotation_after_failed_key_gen() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);
		let keygen_ceremony_id = current_ceremony_id();

		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));
		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(BOB),
			keygen_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
		));
		assert_ok!(EvmThresholdSigner::report_keygen_outcome(
			RuntimeOrigin::signed(CHARLIE),
			keygen_ceremony_id,
			Err(Default::default())
		));
		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(2);
		assert!(matches!(
			PendingKeyRotation::<Test, _>::get(),
			Some(KeyRotationStatus::Failed { .. })
		));

		// Abort by resetting key rotation state
		EvmThresholdSigner::reset_key_rotation();

		assert!(PendingKeyRotation::<Test, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 0);
		assert_eq!(EvmThresholdSigner::status(), AsyncResult::Void);

		// Can restart the key rotation and succeed.
		do_full_key_rotation();
	});
}

#[test]
fn can_recover_from_abort_key_rotation_after_key_verification() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);
		let keygen_ceremony_id = current_ceremony_id();

		for p in ALL_CANDIDATES {
			assert_ok!(EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(*p),
				keygen_ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			));
		}

		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);

		let cfes = [ALICE]
			.iter()
			.map(|id| MockCfe { id: *id, behaviour: CfeBehaviour::Success })
			.collect::<Vec<_>>();
		run_cfes_on_sc_events(&cfes);

		assert!(matches!(
			PendingKeyRotation::<Test, _>::get(),
			Some(KeyRotationStatus::KeygenVerificationComplete { .. })
		));

		// Abort the key rotation now
		EvmThresholdSigner::reset_key_rotation();

		assert!(PendingKeyRotation::<Test, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 0);
		assert_eq!(EvmThresholdSigner::status(), AsyncResult::Void);

		// Can restart the key rotation and succeed.
		do_full_key_rotation();
	});
}

#[test]
fn can_recover_from_abort_key_rotation_after_key_handover_failed() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
		<EvmThresholdSigner as KeyRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);
		let keygen_ceremony_id = current_ceremony_id();
		for p in ALL_CANDIDATES {
			assert_ok!(EvmThresholdSigner::report_keygen_outcome(
				RuntimeOrigin::signed(*p),
				keygen_ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			));
		}

		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(1);

		let cfes = [ALICE]
			.iter()
			.map(|id| MockCfe { id: *id, behaviour: CfeBehaviour::Success })
			.collect::<Vec<_>>();
		run_cfes_on_sc_events(&cfes);

		// Key handover
		const SHARING_PARTICIPANTS: [u64; 2] = [ALICE, BOB];
		EvmThresholdSigner::key_handover(
			BTreeSet::from(SHARING_PARTICIPANTS),
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);

		let handover_ceremony_id = current_ceremony_id();

		for p in ALL_CANDIDATES {
			assert_ok!(EvmThresholdSigner::report_key_handover_outcome(
				RuntimeOrigin::signed(*p),
				handover_ceremony_id,
				Err(Default::default())
			));
		}

		<EvmThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(2);
		assert!(matches!(
			PendingKeyRotation::<Test, _>::get(),
			Some(KeyRotationStatus::KeyHandoverFailed { .. })
		));

		// Abort by resetting key rotation state
		EvmThresholdSigner::reset_key_rotation();

		assert!(PendingKeyRotation::<Test, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 0);
		assert_eq!(EvmThresholdSigner::status(), AsyncResult::Void);

		// Can restart the key rotation and succeed.
		do_full_key_rotation();
	});
}
