use std::collections::{BTreeMap, BTreeSet};

use crate::{
	self as pallet_cf_threshold_signature, mock::*, AttemptCount, CeremonyContext, CeremonyId,
	Error, PalletOffence, RequestContext, RequestId, ThresholdSignatureResponseTimeout,
};
use cf_chains::mocks::{MockAggKey, MockEthereumChainCrypto, MockFixedKeySigningRequests};
use cf_traits::{
	mocks::{key_provider::MockKeyProvider, signer_nomination::MockNominator},
	AsyncResult, Chainflip, EpochKey, KeyProvider, ThresholdSigner,
};

use frame_support::{
	assert_err, assert_noop, assert_ok,
	instances::Instance1,
	traits::{Hooks, OnInitialize},
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::BlockNumberProvider;

macro_rules! assert_last_event {
	($pat:pat) => {
		let event = last_event::<Test>();
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

fn get_ceremony_context(
	ceremony_id: CeremonyId,
	expected_request_id: RequestId,
	expected_attempt: AttemptCount,
) -> CeremonyContext<Test, Instance1> {
	let CeremonyContext::<Test, Instance1> {
		request_context: RequestContext { request_id, attempt_count, .. },
		..
	} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
	assert_eq!(request_id, expected_request_id);
	assert_eq!(attempt_count, expected_attempt);
	EthereumThresholdSigner::pending_ceremonies(ceremony_id)
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
	let events = System::events();
	System::reset_events();
	for event_record in events {
		for cfe in cfes {
			cfe.process_event(event_record.event.clone());
		}
	}
}

fn current_ceremony_id() -> CeremonyId {
	<Test as crate::Config<Instance1>>::CeremonyIdProvider::get()
}

impl MockCfe {
	fn process_event(&self, event: RuntimeEvent) {
		match event {
			RuntimeEvent::EthereumThresholdSigner(
				pallet_cf_threshold_signature::Event::ThresholdSignatureRequest {
					ceremony_id,
					key,
					signatories,
					payload,
					..
				},
			) => {
				assert_eq!(key, current_agg_key());
				assert_eq!(signatories, MockNominator::get_nominees().unwrap());

				match &self.behaviour {
					CfeBehaviour::Success => {
						// Wrong request id is a no-op
						assert_noop!(
							EthereumThresholdSigner::signature_success(
								RuntimeOrigin::none(),
								ceremony_id + 1,
								sign(payload)
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						assert_ok!(EthereumThresholdSigner::signature_success(
							RuntimeOrigin::none(),
							ceremony_id,
							sign(payload),
						));
					},
					CfeBehaviour::ReportFailure(bad) => {
						// Invalid ceremony id.
						assert_noop!(
							EthereumThresholdSigner::report_signature_failed(
								RuntimeOrigin::signed(self.id),
								ceremony_id * 2,
								BTreeSet::from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						// Unsolicited responses are rejected.
						assert_noop!(
							EthereumThresholdSigner::report_signature_failed(
								RuntimeOrigin::signed(signatories.iter().max().unwrap() + 1),
								ceremony_id,
								BTreeSet::from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidRespondent
						);

						assert_ok!(EthereumThresholdSigner::report_signature_failed(
							RuntimeOrigin::signed(self.id),
							ceremony_id,
							BTreeSet::from_iter(bad.clone()),
						));

						// Can't respond twice.
						assert_noop!(
							EthereumThresholdSigner::report_signature_failed(
								RuntimeOrigin::signed(self.id),
								ceremony_id,
								BTreeSet::from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidRespondent
						);
					},
					CfeBehaviour::Timeout => {
						// Oops
					},
				};
			},
			_ => panic!("Unexpected event"),
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
				EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			run_cfes_on_sc_events(&[cfe]);

			// Request is complete
			assert!(EthereumThresholdSigner::pending_ceremonies(ceremony_id).is_none());

			// Signature is available
			assert!(matches!(
				EthereumThresholdSigner::signature(request_context.request_id),
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
				EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			run_cfes_on_sc_events(&[cfe]);

			// Request is complete
			assert!(EthereumThresholdSigner::pending_ceremonies(ceremony_id).is_none());

			// Callback has triggered.
			assert!(MockCallback::has_executed(request_context.request_id));

			// Signature has been consumed.
			assert!(
				matches!(
					EthereumThresholdSigner::signature(request_context.request_id),
					AsyncResult::Void
				),
				"Expected Void, got {:?}",
				EthereumThresholdSigner::signature(request_context.request_id)
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
				EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			assert_eq!(MockCallback::times_called(), 0);
			// report signature success
			run_cfes_on_sc_events(&[MockCfe { id: 1, behaviour: CfeBehaviour::Success }]);

			assert!(MockCallback::has_executed(request_context.request_id));
			assert_eq!(MockCallback::times_called(), 1);

			// Submit the same success again
			assert_err!(
				EthereumThresholdSigner::signature_success(
					RuntimeOrigin::none(),
					ceremony_id,
					sign(*PAYLOAD)
				),
				Error::<Test, Instance1>::InvalidCeremonyId
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
			const PAYLOAD: &[u8; 4] = b"OHAI";
			let EpochKey { key, .. } =
				<Test as crate::Config<_>>::KeyProvider::active_epoch_key().unwrap();
			let request_id = EthereumThresholdSigner::request_verification_signature(
				*PAYLOAD,
				NOMINEES.into_iter().collect(),
				key,
				0,
				MockCallback::new,
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
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfes = [
				MockCfe { id: 1, behaviour: CfeBehaviour::Timeout },
				MockCfe { id: 2, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
			];

			// CFEs respond
			run_cfes_on_sc_events(&cfes[..]);

			// Request is still pending waiting for account 1 to respond.

			// Account 1 has 1 blame vote against it.
			assert_eq!(
				EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap().blame_counts,
				BTreeMap::from_iter([(1, 1)])
			);

			// Callback has *not* executed but is scheduled for a retry after the timeout has
			// elapsed.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				EthereumThresholdSigner::threshold_signature_response_timeout();

			assert!(!MockCallback::has_executed(request_id));
			assert_eq!(EthereumThresholdSigner::ceremony_retry_queues(retry_block).len(), 1);

			// The offender has not yet been reported.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![]);

			// Process retries.
			System::set_block_number(retry_block);
			<AllPalletsWithSystem as OnInitialize<_>>::on_initialize(retry_block);

			// Expect the retry queue for this block to be empty.
			assert!(EthereumThresholdSigner::ceremony_retry_queues(retry_block).is_empty());
			// Another timeout should have been added for the new ceremony.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				EthereumThresholdSigner::threshold_signature_response_timeout();
			assert!(!EthereumThresholdSigner::ceremony_retry_queues(retry_block).is_empty());

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
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfes = [(1, vec![]), (2, vec![1]), (3, vec![1]), (4, vec![1]), (5, vec![1])]
				.into_iter()
				.map(|(id, report)| MockCfe { id, behaviour: CfeBehaviour::ReportFailure(report) })
				.collect::<Vec<_>>();

			// CFEs responds, triggering a retry for the next block.
			run_cfes_on_sc_events(&cfes[..]);
			let next_block_retry = frame_system::Pallet::<Test>::current_block_number() + 1;
			let timeout_block_for_next_retry =
				next_block_retry + EthereumThresholdSigner::threshold_signature_response_timeout();

			assert_eq!(EthereumThresholdSigner::ceremony_retry_queues(next_block_retry).len(), 1);

			// Account 1 has 4 blame votes against it. The other accounts have no votes against
			// them.
			assert_eq!(
				EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap().blame_counts,
				BTreeMap::from_iter([(1, 4)])
			);

			// after the block is process, we of course have moved to the next block.
			System::set_block_number(next_block_retry);
			<EthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				next_block_retry,
			);

			// We did reach the reporting threshold, participant 1 was reported.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![1]);

			assert!(!MockCallback::has_executed(request_id));
			assert!(EthereumThresholdSigner::ceremony_retry_queues(next_block_retry).is_empty());

			assert_eq!(
				EthereumThresholdSigner::ceremony_retry_queues(timeout_block_for_next_retry).len(),
				1
			);

			assert_eq!(
				get_ceremony_context(ceremony_id + 1, request_id, attempt_count + 1)
					.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap())
			);

			System::set_block_number(timeout_block_for_next_retry);
			<EthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				timeout_block_for_next_retry,
			);
			assert!(EthereumThresholdSigner::ceremony_retry_queues(timeout_block_for_next_retry)
				.is_empty());

			assert_eq!(
				EthereumThresholdSigner::ceremony_retry_queues(
					timeout_block_for_next_retry +
						EthereumThresholdSigner::threshold_signature_response_timeout()
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
			assert_eq!(EthereumThresholdSigner::request_retry_queues(retry_block).len(), 1);
		});
}

#[test]
fn test_retries_for_locked_key() {
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
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();

			MockKeyProvider::<MockEthereumChainCrypto>::lock_key(request_id);

			// Key is now locked and should be unavailable for new requests.
			<EthereumThresholdSigner as ThresholdSigner<_>>::request_signature(*b"SUP?");
			// Ceremony counter should not have changed.
			assert_eq!(ceremony_id, current_ceremony_id());

			// Retry should re-use the same key.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				ThresholdSignatureResponseTimeout::<Test, _>::get();
			<EthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);

			let retry_ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context:
					RequestContext { request_id: request_id_2, attempt_count: second_attempt, .. },
				..
			} = EthereumThresholdSigner::pending_ceremonies(retry_ceremony_id).unwrap();
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
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();

			MockFixedKeySigningRequests::set(true);

			// Pretend the key has been updated to the next one.
			MockKeyProvider::<MockEthereumChainCrypto>::add_key(MockAggKey(*b"NEXT"));

			// Retry should re-use the original key.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				ThresholdSignatureResponseTimeout::<Test, _>::get();
			<EthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);

			let retry_ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context:
					RequestContext { request_id: request_id_2, attempt_count: second_attempt, .. },
				key,
				..
			} = EthereumThresholdSigner::pending_ceremonies(retry_ceremony_id).unwrap();
			assert_eq!(request_id, request_id_2);
			assert_eq!(second_attempt, first_attempt + 1);
			assert_eq!(retry_ceremony_id, ceremony_id + 1);
			assert_eq!(key, MockAggKey(AGG_KEY));
		});
}

#[cfg(test)]
mod unsigned_validation {
	use super::*;
	use crate::{Call as PalletCall, CeremonyRetryQueues, PendingCeremonies};
	use cf_chains::{mocks::MockAggKey, ChainCrypto};
	use cf_traits::{
		mocks::ceremony_id_provider::MockCeremonyIdProvider, KeyProvider, ThresholdSigner,
	};
	use frame_support::{pallet_prelude::InvalidTransaction, unsigned::TransactionSource};
	use sp_runtime::traits::ValidateUnsigned;

	#[test]
	fn start_custom_signing_ceremony() {
		new_test_ext().execute_with_consistency_checks(|| {
			const PAYLOAD: <MockEthereumChainCrypto as ChainCrypto>::Payload = *b"OHAI";
			const CUSTOM_AGG_KEY: <MockEthereumChainCrypto as ChainCrypto>::AggKey =
				MockAggKey(*b"AKEY");

			let participants: BTreeSet<u64> = BTreeSet::from_iter([1, 2, 3, 4, 5, 6]);
			EthereumThresholdSigner::request_verification_signature(
				PAYLOAD,
				participants,
				CUSTOM_AGG_KEY,
				0,
				MockCallback::new,
			);
			let ceremony_id = MockCeremonyIdProvider::get();

			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				EthereumThresholdSigner::threshold_signature_response_timeout();

			// Process retries.
			<EthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);
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

				<EthereumThresholdSigner as ThresholdSigner<_>>::request_signature(PAYLOAD);
				let ceremony_id = MockCeremonyIdProvider::get();
				let EpochKey { key: current_key, .. } =
					<Test as crate::Config<_>>::KeyProvider::active_epoch_key().unwrap();

				assert!(
					Test::validate_unsigned(
						TransactionSource::External,
						&PalletCall::signature_success { ceremony_id, signature: sign(PAYLOAD) }
							.into(),
					)
					.is_ok(),
					"Validation Failed: {:?} / {:?}",
					<Test as crate::Config<_>>::KeyProvider::active_epoch_key(),
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
				<EthereumThresholdSigner as ThresholdSigner<_>>::request_signature(PAYLOAD);
				assert_eq!(
					Test::validate_unsigned(
						TransactionSource::External,
						&PalletCall::signature_success {
							// Incorrect ceremony id
							ceremony_id: MockCeremonyIdProvider::get() + 1,
							signature: sign(PAYLOAD)
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
							ceremony_id: MockCeremonyIdProvider::get(),
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
				EthereumThresholdSigner::validate_unsigned(
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
				let ceremony_id = MockCeremonyIdProvider::get();
				EthereumThresholdSigner::report_signature_failed(
					RuntimeOrigin::signed(NOMINEES[0]),
					ceremony_id,
					valid_blames.iter().cloned().chain(invalid_blames.clone()).collect(),
				)
				.unwrap();

				let CeremonyContext::<Test, Instance1> { blame_counts, .. } =
					EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
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
	use cf_chains::{mocks::MockAggKey, ChainCrypto};

	fn init_context(
		validator_set: impl IntoIterator<Item = <Test as Chainflip>::ValidatorId> + Copy,
	) -> CeremonyContext<Test, Instance1> {
		const PAYLOAD: <MockEthereumChainCrypto as ChainCrypto>::Payload = *b"OHAI";
		MockEpochInfo::set_authorities(validator_set.into_iter().collect());
		CeremonyContext {
			request_context: RequestContext { request_id: 1, attempt_count: 0, payload: PAYLOAD },
			threshold_ceremony_type: ThresholdCeremonyType::Standard,
			epoch: 0,
			key: MockAggKey(AGG_KEY),
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
		<VaultsPallet as VaultRotator>::keygen(BTreeSet::default(), GENESIS_EPOCH);
	});
}

#[test]
fn keygen_request_emitted() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index();
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone(), rotation_epoch);
		// Confirm we have a new vault rotation process running
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
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

		PendingVaultRotation::<Test, _>::put(VaultRotationStatus::KeygenVerificationComplete {
			new_public_key: Default::default(),
		});
		let ceremony_id = current_ceremony_id();

		<VaultsPallet as VaultRotator>::key_handover(
			authorities.clone(),
			candidates.clone(),
			next_epoch,
		);

		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		assert_eq!(
			last_event::<Test>(),
			PalletEvent::<Test, _>::KeyHandoverRequest {
				// It should be incremented when the request is made.
				ceremony_id: ceremony_id + 1,
				from_epoch: current_epoch,
				key_to_share: VaultsPallet::active_epoch_key().unwrap().key,
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
fn start_panics_if_called_while_vault_rotation_in_progress() {
	let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(btree_candidates.clone(), GENESIS_EPOCH);
		<VaultsPallet as VaultRotator>::keygen(btree_candidates, GENESIS_EPOCH);
	});
}

#[test]
fn keygen_success_triggers_keygen_verification() {
	let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());

	new_test_ext().execute_with(|| {
		let rotation_epoch_index = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
		<VaultsPallet as VaultRotator>::keygen(candidates.clone(), rotation_epoch_index);
		let ceremony_id = current_ceremony_id();

		for candidate in &candidates {
			assert_ok!(VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(*candidate),
				ceremony_id,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER),
			));
		}

		VaultsPallet::on_initialize(1);

		assert!(matches!(
			PendingVaultRotation::<Test, _>::get().unwrap(),
			VaultRotationStatus::AwaitingKeygenVerification { .. }
		));

		let verification_request =
			<Test as crate::Config>::ThresholdSigner::last_key_verification_request()
				.expect("request should have been created");

		assert_eq!(
			verification_request,
			VerificationParams {
				participants: candidates,
				key: NEW_AGG_PUB_KEY_PRE_HANDOVER,
				epoch_index: rotation_epoch_index
			}
		);
	});
}

#[test]
fn handover_success_triggers_handover_verification() {
	let authorities = BTreeSet::from_iter(ALL_CANDIDATES.iter().take(2).cloned());
	let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().skip(1).take(2).cloned());
	let all_participants: BTreeSet<_> = authorities.union(&candidates).copied().collect();

	new_test_ext().execute_with(|| {
		let rotation_epoch_index = <Test as Chainflip>::EpochInfo::epoch_index() + 1;

		PendingVaultRotation::<Test, _>::put(VaultRotationStatus::KeygenVerificationComplete {
			new_public_key: NEW_AGG_PUB_KEY_PRE_HANDOVER,
		});

		<VaultsPallet as VaultRotator>::key_handover(
			authorities.clone(),
			candidates.clone(),
			rotation_epoch_index,
		);

		for candidate in &all_participants {
			assert_ok!(VaultsPallet::report_key_handover_outcome(
				RuntimeOrigin::signed(*candidate),
				current_ceremony_id(),
				Ok(NEW_AGG_PUB_KEY_POST_HANDOVER),
			));
		}

		VaultsPallet::on_initialize(1);

		assert!(matches!(
			PendingVaultRotation::<Test, _>::get().unwrap(),
			VaultRotationStatus::AwaitingKeyHandoverVerification { .. }
		));

		let verification_request =
			<Test as crate::Config>::ThresholdSigner::last_key_verification_request()
				.expect("request should have been created");

		// Check that only candidates (i.e. receiving parties) receive the request,
		// and the key is for the new epoch index (if participants wouldn't be able
		// to use any existing key shares by mistake):
		assert_eq!(
			verification_request,
			VerificationParams {
				participants: candidates,
				key: NEW_AGG_PUB_KEY_POST_HANDOVER,
				epoch_index: rotation_epoch_index
			}
		);
	});
}

fn keygen_failure(bad_candidates: &[<Test as Chainflip>::ValidatorId]) {
	VaultsPallet::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()), GENESIS_EPOCH);

	let ceremony_id = current_ceremony_id();

	VaultsPallet::terminate_rotation(bad_candidates, PalletEvent::KeygenFailure(ceremony_id));

	assert_eq!(last_event::<Test>(), PalletEvent::KeygenFailure(ceremony_id).into());

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
// Once all vaults have reported some AsyncResult::Ready status (see all_vaults_rotator) then
// the validator pallet will call keygen() again
#[test]
fn keygen_called_after_keygen_failure_restarts_rotation_at_keygen() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
		keygen_failure(&[BOB, CHARLIE]);
		VaultsPallet::keygen(BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()), rotation_epoch);

		assert_eq!(VaultsPallet::status(), AsyncResult::Pending);

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
	new_test_ext().execute_with(|| {
		let rotation_epoch_index = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
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
			Error::<Test, _>::NoActiveRotation
		);

		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				1,
				Err(Default::default())
			),
			Error::<Test, _>::NoActiveRotation
		);

		assert_noop!(
			VaultsPallet::report_key_handover_outcome(
				RuntimeOrigin::signed(ALICE),
				1,
				Err(Default::default())
			),
			Error::<Test, _>::NoActiveRotation
		);

		assert_noop!(
			VaultsPallet::report_key_handover_outcome(
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
			Error::<Test, _>::InvalidRespondent
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
			Error::<Test, _>::InvalidRespondent
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn only_candidates_can_report_keygen_outcome() {
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

		// Only candidates can respond.
		let non_candidate = u64::MAX;
		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(
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
			Error::<Test, _>::InvalidRespondent
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
			Err(valid_blames.union(&invalid_blames).copied().collect()),
		)
		.unwrap();

		match PendingVaultRotation::<Test, _>::get().unwrap() {
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
			Error::<Test, _>::InvalidCeremonyId
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		// Ceremony id in the future
		assert_noop!(
			VaultsPallet::report_keygen_outcome(
				RuntimeOrigin::signed(ALICE),
				ceremony_id + 1,
				Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
			),
			Error::<Test, _>::InvalidCeremonyId
		);
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	});
}

#[test]
fn cannot_report_key_handover_outcome_when_awaiting_keygen() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			<Test as Chainflip>::EpochInfo::epoch_index() + 1,
		);

		assert_noop!(
			VaultsPallet::report_key_handover_outcome(
				RuntimeOrigin::signed(ALICE),
				current_ceremony_id(),
				Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)
			),
			Error::<Test, _>::InvalidRotationStatus
		);
	});
}

fn do_full_key_rotation() {
	assert!(!MockOptimisticActivation::get(), "Test expects non-optimistic activation");

	let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
	<VaultsPallet as VaultRotator>::keygen(
		BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
		rotation_epoch,
	);
	let keygen_ceremony_id = current_ceremony_id();

	assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 1);

	assert_ok!(VaultsPallet::report_keygen_outcome(
		RuntimeOrigin::signed(ALICE),
		keygen_ceremony_id,
		Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
	));

	assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

	VaultsPallet::on_initialize(1);
	// After on initialise we obviously still don't have enough votes.
	// So nothing should have changed.
	assert!(KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

	// Bob agrees.
	assert_ok!(VaultsPallet::report_keygen_outcome(
		RuntimeOrigin::signed(BOB),
		keygen_ceremony_id,
		Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
	));

	// A resolution is still pending - we require 100% response rate.
	assert!(KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	VaultsPallet::on_initialize(1);
	assert!(KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

	// Charlie agrees.
	assert_ok!(VaultsPallet::report_keygen_outcome(
		RuntimeOrigin::signed(CHARLIE),
		keygen_ceremony_id,
		Ok(NEW_AGG_PUB_KEY_PRE_HANDOVER)
	));

	// This time we should have enough votes for consensus.
	assert!(KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
	if let VaultRotationStatus::AwaitingKeygen {
		ceremony_id: keygen_ceremony_id_from_status,
		response_status,
		keygen_participants,
		new_epoch_index,
	} = PendingVaultRotation::<Test, _>::get().unwrap()
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
	VaultsPallet::on_initialize(1);

	assert!(matches!(
		PendingVaultRotation::<Test, _>::get().unwrap(),
		VaultRotationStatus::AwaitingKeygenVerification { .. }
	));

	EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));
	assert_eq!(
		<VaultsPallet as VaultRotator>::status(),
		AsyncResult::Ready(VaultStatus::KeygenComplete)
	);

	assert!(matches!(
		PendingVaultRotation::<Test, _>::get().unwrap(),
		VaultRotationStatus::KeygenVerificationComplete { .. }
	));

	const SHARING_PARTICIPANTS: [u64; 2] = [ALICE, BOB];
	VaultsPallet::key_handover(
		BTreeSet::from(SHARING_PARTICIPANTS),
		BTreeSet::from_iter(ALL_CANDIDATES.iter().copied()),
		rotation_epoch,
	);
	assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

	let handover_ceremony_id = current_ceremony_id();
	for p in ALL_CANDIDATES {
		assert_ok!(VaultsPallet::report_key_handover_outcome(
			RuntimeOrigin::signed(*p),
			handover_ceremony_id,
			Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)
		));
	}

	VaultsPallet::on_initialize(1);
	assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

	assert_last_event!(crate::Event::KeyHandoverSuccess { .. });

	assert!(matches!(
		PendingVaultRotation::<Test, _>::get().unwrap(),
		VaultRotationStatus::AwaitingKeyHandoverVerification { .. }
	));

	BtcMockThresholdSigner::execute_signature_result_against_last_request(Ok(vec![BTC_DUMMY_SIG]));

	assert_eq!(
		<VaultsPallet as VaultRotator>::status(),
		AsyncResult::Ready(VaultStatus::KeyHandoverComplete)
	);

	assert_last_event!(crate::Event::KeyHandoverVerificationSuccess { .. });

	assert!(matches!(
		PendingVaultRotation::<Test, _>::get().unwrap(),
		VaultRotationStatus::KeyHandoverComplete { .. }
	));

	// Called by validator pallet
	VaultsPallet::activate();

	assert!(!KeygenResolutionPendingSince::<Test, _>::exists());
	assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

	assert!(matches!(
		PendingVaultRotation::<Test, _>::get().unwrap(),
		VaultRotationStatus::<Test, _>::AwaitingActivation { new_public_key: k } if k == NEW_AGG_PUB_KEY_POST_HANDOVER
	));

	// Voting has been cleared.
	assert_eq!(KeygenSuccessVoters::<Test, _>::iter_keys().next(), None);
	assert!(!KeygenFailureVoters::<Test, _>::exists());

	assert_ok!(VaultsPallet::vault_key_rotated(RuntimeOrigin::root(), 1, [0xab; 4],));

	assert_last_event!(crate::Event::VaultRotationCompleted);
	assert_eq!(PendingVaultRotation::<Test, _>::get(), Some(VaultRotationStatus::Complete));
	assert_eq!(VaultsPallet::status(), AsyncResult::Ready(VaultStatus::RotationComplete));
}

#[test]
fn keygen_report_success() {
	new_test_ext().execute_with(do_full_key_rotation);
}

#[test]
fn keygen_report_failure() {
	new_test_ext().execute_with(|| {
		<VaultsPallet as VaultRotator>::keygen(
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			GENESIS_EPOCH,
		);
		let ceremony_id = current_ceremony_id();

		assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 1);

		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(ALICE),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		// Bob agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(BOB),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));

		// A resolution is still pending - we expect 100% response rate.
		assert!(KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		VaultsPallet::on_initialize(1);
		assert!(KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);

		// Charlie agrees.
		assert_ok!(VaultsPallet::report_keygen_outcome(
			RuntimeOrigin::signed(CHARLIE),
			ceremony_id,
			Err(BTreeSet::from_iter([CHARLIE]))
		));

		// This time we should have enough votes for consensus.
		assert!(KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(<VaultsPallet as VaultRotator>::status(), AsyncResult::Pending);
		VaultsPallet::on_initialize(1);
		assert!(!KeygenResolutionPendingSince::<Test, _>::exists());
		assert_eq!(
			VaultsPallet::status(),
			AsyncResult::Ready(VaultStatus::Failed(BTreeSet::from([CHARLIE])))
		);

		MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, vec![CHARLIE]);

		assert_last_event!(crate::Event::KeygenFailure(..));

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
		test_key_ceremony_timeout_period::<KeygenResolutionPendingSince<Test, _>, _>(
			VaultsPallet::report_keygen_outcome,
		)
	});
}

#[test]
fn test_key_handover_timeout_period() {
	new_test_ext().execute_with(|| {
		let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());
		PendingVaultRotation::<Test, _>::put(VaultRotationStatus::KeygenVerificationComplete {
			new_public_key: Default::default(),
		});
		<VaultsPallet as VaultRotator>::key_handover(candidates.clone(), candidates, 2);
		test_key_ceremony_timeout_period::<KeyHandoverResolutionPendingSince<Test, _>, _>(
			VaultsPallet::report_key_handover_outcome,
		)
	});
}

#[cfg(test)]
mod vault_key_rotation {
	use cf_chains::mocks::{MockEthereum, BAD_AGG_KEY_POST_HANDOVER};
	use cf_traits::mocks::block_height_provider::BlockHeightProvider;

	use super::*;

	const ACTIVATION_BLOCK_NUMBER: u64 = 42;
	const TX_HASH: [u8; 4] = [0xab; 4];

	fn setup(key_handover_outcome: KeygenOutcomeFor<Test>) -> TestRunner<()> {
		let ext = new_test_ext();
		ext.execute_with(|| {
			let authorities = BTreeSet::from_iter(ALL_CANDIDATES.iter().take(2).cloned());
			let candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().skip(1).take(2).cloned());

			let rotation_epoch_index = <Test as Chainflip>::EpochInfo::epoch_index() + 1;

			assert_noop!(
				VaultsPallet::vault_key_rotated(
					RuntimeOrigin::root(),
					ACTIVATION_BLOCK_NUMBER,
					TX_HASH,
				),
				Error::<Test, _>::NoActiveRotation
			);

			<VaultsPallet as VaultRotator>::keygen(candidates.clone(), GENESIS_EPOCH);
			let ceremony_id = current_ceremony_id();
			VaultsPallet::trigger_keygen_verification(
				ceremony_id,
				NEW_AGG_PUB_KEY_PRE_HANDOVER,
				candidates.clone(),
				rotation_epoch_index,
			);

			EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(
				ETH_DUMMY_SIG,
			));

			VaultsPallet::key_handover(
				authorities.clone(),
				candidates.clone(),
				rotation_epoch_index,
			);

			// Note that we require all participants to respond
			for candidate in authorities.union(&candidates) {
				assert_ok!(VaultsPallet::report_key_handover_outcome(
					RuntimeOrigin::signed(*candidate),
					current_ceremony_id(),
					key_handover_outcome.clone()
				));
			}

			VaultsPallet::on_initialize(1);
		})
	}

	fn final_checks(ext: TestRunner<()>, expected_activation_block: u64) {
		ext.execute_with(|| {
			// Can't repeat.
			assert_noop!(
				VaultsPallet::vault_key_rotated(
					RuntimeOrigin::root(),
					expected_activation_block,
					TX_HASH,
				),
				Error::<Test, _>::InvalidRotationStatus
			);

			let current_epoch = <Test as Chainflip>::EpochInfo::epoch_index();

			let Vault { public_key, active_from_block } =
				Vaults::<Test, _>::get(current_epoch).expect("Ethereum Vault should exist");
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
			let Vault { public_key, active_from_block } = Vaults::<Test, _>::get(next_epoch)
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
			assert_eq!(PendingVaultRotation::<Test, _>::get(), Some(VaultRotationStatus::Complete),);
			assert_last_event!(crate::Event::VaultRotationCompleted { .. });
		});
	}

	#[test]
	fn non_optimistic_activation() {
		let ext = setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)).execute_with(|| {
			BtcMockThresholdSigner::execute_signature_result_against_last_request(Ok(vec![
				BTC_DUMMY_SIG,
			]));

			MockOptimisticActivation::set(false);
			VaultsPallet::activate();

			assert!(matches!(
				PendingVaultRotation::<Test, _>::get().unwrap(),
				VaultRotationStatus::AwaitingActivation { .. }
			));

			assert_ok!(VaultsPallet::vault_key_rotated(
				RuntimeOrigin::root(),
				ACTIVATION_BLOCK_NUMBER,
				TX_HASH,
			));
		});

		final_checks(ext, ACTIVATION_BLOCK_NUMBER);
	}

	#[test]
	fn optimistic_activation() {
		const HANDOVER_ACTIVATION_BLOCK: u64 = 420;
		let ext = setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)).execute_with(|| {
			BtcMockThresholdSigner::execute_signature_result_against_last_request(Ok(vec![
				BTC_DUMMY_SIG,
			]));

			BlockHeightProvider::<MockEthereum>::set_block_height(HANDOVER_ACTIVATION_BLOCK);
			MockOptimisticActivation::set(true);
			VaultsPallet::activate();

			// No need to call vault_key_rotated.
			assert_noop!(
				VaultsPallet::vault_key_rotated(
					RuntimeOrigin::root(),
					ACTIVATION_BLOCK_NUMBER,
					TX_HASH,
				),
				Error::<Test, _>::InvalidRotationStatus
			);

			assert!(matches!(
				PendingVaultRotation::<Test, _>::get().unwrap(),
				VaultRotationStatus::Complete,
			));
		});
		final_checks(ext, HANDOVER_ACTIVATION_BLOCK);
	}

	#[test]
	fn can_recover_after_handover_failure() {
		const HANDOVER_ACTIVATION_BLOCK: u64 = 420;
		let ext = setup(Err(Default::default())).execute_with(|| {
			assert!(matches!(
				PendingVaultRotation::<Test, _>::get().unwrap(),
				VaultRotationStatus::KeyHandoverFailed { .. }
			));
			BlockHeightProvider::<MockEthereum>::set_block_height(HANDOVER_ACTIVATION_BLOCK);

			// Start handover again, but successful this time.
			let btree_candidates = BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned());
			VaultsPallet::key_handover(
				btree_candidates.clone(),
				btree_candidates.clone(),
				<Test as Chainflip>::EpochInfo::epoch_index() + 1,
			);

			for candidate in btree_candidates {
				assert_ok!(VaultsPallet::report_key_handover_outcome(
					RuntimeOrigin::signed(candidate),
					current_ceremony_id(),
					Ok(NEW_AGG_PUB_KEY_POST_HANDOVER),
				));
			}

			VaultsPallet::on_initialize(1);

			BtcMockThresholdSigner::execute_signature_result_against_last_request(Ok(vec![
				BTC_DUMMY_SIG,
			]));

			MockOptimisticActivation::set(true);
			VaultsPallet::activate();
		});

		final_checks(ext, HANDOVER_ACTIVATION_BLOCK);
	}

	#[test]
	fn key_handover_success_triggers_key_handover_verification() {
		setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)).execute_with(|| {
			assert!(matches!(
				PendingVaultRotation::<Test, _>::get(),
				Some(VaultRotationStatus::AwaitingKeyHandoverVerification { .. })
			));
		});
	}

	#[test]
	fn key_handover_fails_on_key_mismatch() {
		setup(Ok(BAD_AGG_KEY_POST_HANDOVER)).execute_with(|| {
			assert_last_event!(crate::Event::KeyHandoverFailure { .. });
			assert!(matches!(
				PendingVaultRotation::<Test, _>::get(),
				Some(VaultRotationStatus::KeyHandoverFailed { .. })
			));
		});
	}

	#[test]
	fn can_recover_after_key_handover_verification_failure() {
		setup(Ok(NEW_AGG_PUB_KEY_POST_HANDOVER)).execute_with(|| {
			let offenders = vec![ALICE];

			BtcMockThresholdSigner::execute_signature_result_against_last_request(Err(
				offenders.clone()
			));

			VaultsPallet::on_initialize(1);

			assert_last_event!(crate::Event::KeyHandoverVerificationFailure { .. });

			MockOffenceReporter::assert_reported(PalletOffence::FailedKeygen, offenders.clone());

			let offenders = BTreeSet::from_iter(offenders);
			assert_eq!(
				VaultsPallet::status(),
				AsyncResult::Ready(VaultStatus::Failed(offenders.clone()))
			);

			assert_eq!(
				PendingVaultRotation::<Test, _>::get(),
				Some(VaultRotationStatus::Failed { offenders })
			);

			// Can restart the vault rotation and succeed.
			do_full_key_rotation();
		});
	}
}

#[test]
fn set_keygen_response_timeout_works() {
	new_test_ext_no_key().execute_with(|| {
		let init_timeout = KeygenResponseTimeout::<Test, _>::get();

		VaultsPallet::set_keygen_response_timeout(RuntimeOrigin::root(), init_timeout).unwrap();

		assert!(maybe_last_event::<Test>().is_none());

		let new_timeout = init_timeout + 1;

		VaultsPallet::set_keygen_response_timeout(RuntimeOrigin::root(), new_timeout).unwrap();

		assert_last_event!(crate::Event::KeygenResponseTimeoutUpdated { .. });
		assert_eq!(KeygenResponseTimeout::<Test, _>::get(), new_timeout)
	});
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

#[test]
fn can_recover_from_abort_vault_rotation_after_failed_key_gen() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
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
		assert!(matches!(
			PendingVaultRotation::<Test, _>::get(),
			Some(VaultRotationStatus::Failed { .. })
		));

		// Abort by resetting vault rotation state
		VaultsPallet::reset_vault_rotation();

		assert!(PendingVaultRotation::<Test, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 0);
		assert_eq!(VaultsPallet::status(), AsyncResult::Void);

		// Can restart the vault rotation and succeed.
		do_full_key_rotation();
	});
}

#[test]
fn can_recover_from_abort_vault_rotation_after_key_verification() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
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
		assert!(matches!(
			PendingVaultRotation::<Test, _>::get(),
			Some(VaultRotationStatus::KeygenVerificationComplete { .. })
		));

		// Abort the vault rotation now
		VaultsPallet::reset_vault_rotation();

		assert!(PendingVaultRotation::<Test, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 0);
		assert_eq!(VaultsPallet::status(), AsyncResult::Void);

		// Can restart the vault rotation and succeed.
		do_full_key_rotation();
	});
}

#[test]
fn can_recover_from_abort_vault_rotation_after_key_handover_failed() {
	new_test_ext().execute_with(|| {
		let rotation_epoch = <Test as Chainflip>::EpochInfo::epoch_index() + 1;
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
		const SHARING_PARTICIPANTS: [u64; 2] = [ALICE, BOB];
		VaultsPallet::key_handover(
			BTreeSet::from(SHARING_PARTICIPANTS),
			BTreeSet::from_iter(ALL_CANDIDATES.iter().cloned()),
			rotation_epoch,
		);

		let handover_ceremony_id = current_ceremony_id();

		for p in ALL_CANDIDATES {
			assert_ok!(VaultsPallet::report_key_handover_outcome(
				RuntimeOrigin::signed(*p),
				handover_ceremony_id,
				Err(Default::default())
			));
		}

		VaultsPallet::on_initialize(2);
		assert!(matches!(
			PendingVaultRotation::<Test, _>::get(),
			Some(VaultRotationStatus::KeyHandoverFailed { .. })
		));

		// Abort by resetting vault rotation state
		VaultsPallet::reset_vault_rotation();

		assert!(PendingVaultRotation::<Test, _>::get().is_none());
		assert_eq!(KeygenResolutionPendingSince::<Test, _>::get(), 0);
		assert_eq!(VaultsPallet::status(), AsyncResult::Void);

		// Can restart the vault rotation and succeed.
		do_full_key_rotation();
	});
}
