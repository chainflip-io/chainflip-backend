use std::collections::{BTreeMap, BTreeSet};

use crate::{
	mock::*, AttemptCount, CeremonyContext, CeremonyId, Error, PalletOffence, RequestContext,
	RequestId, ThresholdSignatureResponseTimeout,
};
use cf_chains::mocks::{MockAggKey, MockEthereumChainCrypto};
use cf_traits::{
	mocks::{
		cfe_interface_mock::{MockCfeEvent, MockCfeInterface},
		key_provider::MockKeyProvider,
		signer_nomination::MockNominator,
	},
	AsyncResult, Chainflip, EpochKey, KeyProvider, ThresholdSigner,
};

use cfe_events::ThresholdSignatureRequest;
use frame_support::{
	assert_err, assert_noop, assert_ok,
	instances::Instance1,
	traits::{Hooks, OnInitialize},
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::BlockNumberProvider;

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
	let events = MockCfeInterface::take_events();
	for event in events {
		for cfe in cfes {
			cfe.process_event(event.clone());
		}
	}
}

fn current_ceremony_id() -> CeremonyId {
	<Test as crate::Config<Instance1>>::CeremonyIdProvider::get()
}

type ValidatorId = <Test as Chainflip>::ValidatorId;

impl MockCfe {
	fn process_event(&self, event: MockCfeEvent<ValidatorId>) {
		match event {
			MockCfeEvent::EthThresholdSignatureRequest(ThresholdSignatureRequest {
				ceremony_id,
				epoch_index: _,
				key,
				signatories,
				payload,
			}) => {
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
			_ => {
				panic!("Unexpected event");
			},
		}
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
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();

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
