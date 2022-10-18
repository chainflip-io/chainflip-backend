use std::collections::{BTreeMap, BTreeSet};

use crate::{
	self as pallet_cf_threshold_signature, mock::*, AttemptCount, CeremonyContext, CeremonyId,
	Error, PalletOffence, RequestContext, RequestId,
};
use cf_chains::mocks::MockEthereum;
use cf_traits::{mocks::signer_nomination::MockNominator, AsyncResult, Chainflip};
use frame_support::{
	assert_noop, assert_ok,
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
		request_context: RequestContext::<Test, Instance1> { request_id, attempt_count, .. },
		..
	} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
	assert_eq!(request_id, expected_request_id);
	assert_eq!(attempt_count, expected_attempt);
	EthereumThresholdSigner::pending_ceremonies(ceremony_id)
		.unwrap_or_else(|| panic!("Expected a ceremony with id {:?}", ceremony_id))
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
	fn process_event(&self, event: Event) {
		match event {
			Event::EthereumThresholdSigner(
				pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
					req_id,
					key_id,
					signers,
					payload,
				),
			) => {
				assert_eq!(key_id, &MOCK_AGG_KEY);
				assert_eq!(signers, MockNominator::get_nominees().unwrap());

				match &self.behaviour {
					CfeBehaviour::Success => {
						// Wrong request id is a no-op
						assert_noop!(
							EthereumThresholdSigner::signature_success(
								Origin::none(),
								req_id + 1,
								sign(payload)
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						assert_ok!(EthereumThresholdSigner::signature_success(
							Origin::none(),
							req_id,
							sign(payload),
						));
					},
					CfeBehaviour::ReportFailure(bad) => {
						// Invalid ceremony id.
						assert_noop!(
							EthereumThresholdSigner::report_signature_failed(
								Origin::signed(self.id),
								req_id * 2,
								BTreeSet::from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						// Unsolicited responses are rejected.
						assert_noop!(
							EthereumThresholdSigner::report_signature_failed(
								Origin::signed(signers.iter().max().unwrap() + 1),
								req_id,
								BTreeSet::from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidRespondent
						);

						assert_ok!(EthereumThresholdSigner::report_signature_failed(
							Origin::signed(self.id),
							req_id,
							BTreeSet::from_iter(bad.clone()),
						));

						// Can't respond twice.
						assert_noop!(
							EthereumThresholdSigner::report_signature_failed(
								Origin::signed(self.id),
								req_id,
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
	ExtBuilder::new()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context: RequestContext::<Test, Instance1> { request_id, .. },
				..
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			run_cfes_on_sc_events(&[cfe]);

			// Request is complete
			assert!(EthereumThresholdSigner::pending_ceremonies(ceremony_id).is_none());

			// Signature is available
			assert!(matches!(
				EthereumThresholdSigner::signature(request_id),
				AsyncResult::Ready(..)
			));

			// No callback was provided.
			assert!(!MockCallback::has_executed(request_id));
		});
}

#[test]
fn happy_path_with_callback() {
	const NOMINEES: [u64; 2] = [1, 2];
	const AUTHORITIES: [u64; 3] = [1, 2, 3];
	ExtBuilder::new()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request_and_callback(b"OHAI", MockCallback::new)
		.build()
		.execute_with(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context: RequestContext::<Test, Instance1> { request_id, .. },
				..
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			run_cfes_on_sc_events(&[cfe]);

			// Request is complete
			assert!(EthereumThresholdSigner::pending_ceremonies(ceremony_id).is_none());

			// Callback has triggered.
			assert!(MockCallback::has_executed(request_id));

			// Signature has been consumed.
			assert!(
				matches!(EthereumThresholdSigner::signature(request_id), AsyncResult::Void),
				"Expected Void, got {:?}",
				EthereumThresholdSigner::signature(request_id)
			);
		});
}

#[test]
fn fail_path_with_timeout() {
	const NOMINEES: [u64; 2] = [1, 2];
	const AUTHORITIES: [u64; 3] = [1, 2, 3];
	ExtBuilder::new()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context: RequestContext::<Test, Instance1> { request_id, attempt_count, .. },
				..
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfes = [
				MockCfe { id: 1, behaviour: CfeBehaviour::Timeout },
				MockCfe { id: 2, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
			];

			// CFEs respond
			run_cfes_on_sc_events(&cfes[..]);

			// Request is still pending waiting for account 1.
			let request_context = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();

			// Account 1 has 1 blame vote against it.
			assert_eq!(request_context.blame_counts, BTreeMap::from_iter([(1, 1)]));

			// Callback has *not* executed but is scheduled for a retry after the timeout has
			// elapsed.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				EthereumThresholdSigner::threshold_signature_response_timeout() as u64;

			assert!(!MockCallback::has_executed(request_id));
			assert_eq!(EthereumThresholdSigner::retry_queues(retry_block).len(), 1);

			// The offender has not yet been reported.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![]);

			// Process retries.
			System::set_block_number(retry_block);
			<AllPalletsWithSystem as OnInitialize<_>>::on_initialize(retry_block);

			// Expect the retry queue for this block to be empty.
			assert!(EthereumThresholdSigner::retry_queues(retry_block).is_empty());
			// Another timeout should have been added for the new ceremony.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				EthereumThresholdSigner::threshold_signature_response_timeout() as u64;
			assert!(!EthereumThresholdSigner::retry_queues(retry_block).is_empty());

			// Participant 1 was reported for not responding.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![1]);

			// We have a new request pending: New ceremony_id, same request context.
			assert_eq!(
				get_ceremony_context(ceremony_id + 1, request_id, attempt_count + 1)
					.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap().into_iter())
			);
		});
}

#[test]
fn fail_path_due_to_report_signature_failed() {
	const NOMINEES: [u64; 5] = [1, 2, 3, 4, 5];
	const AUTHORITIES: [u64; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
	ExtBuilder::new()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			// progress by one block *after* the initial request is inserted (in the ExtBuilder)
			System::set_block_number(frame_system::Pallet::<Test>::current_block_number() + 1);
			let ceremony_id = current_ceremony_id();
			let CeremonyContext::<Test, Instance1> {
				request_context: RequestContext::<Test, Instance1> { request_id, attempt_count, .. },
				..
			} = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			let cfes = [(1, vec![]), (2, vec![1]), (3, vec![1]), (4, vec![1]), (5, vec![1])]
				.into_iter()
				.map(|(id, report)| MockCfe { id, behaviour: CfeBehaviour::ReportFailure(report) })
				.collect::<Vec<_>>();

			// CFEs responds, triggering a retry for the next block.
			run_cfes_on_sc_events(&cfes[..]);
			let next_block_retry = frame_system::Pallet::<Test>::current_block_number() + 1;
			let timeout_block_for_next_retry = next_block_retry +
				EthereumThresholdSigner::threshold_signature_response_timeout() as u64;

			assert_eq!(EthereumThresholdSigner::retry_queues(next_block_retry).len(), 1);

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
			assert!(EthereumThresholdSigner::retry_queues(next_block_retry).is_empty());

			assert_eq!(
				EthereumThresholdSigner::retry_queues(timeout_block_for_next_retry).len(),
				1
			);

			assert_eq!(
				get_ceremony_context(ceremony_id + 1, request_id, attempt_count + 1)
					.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap().into_iter())
			);

			System::set_block_number(timeout_block_for_next_retry);
			<EthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				timeout_block_for_next_retry,
			);
			assert!(EthereumThresholdSigner::retry_queues(timeout_block_for_next_retry).is_empty());

			assert_eq!(
				EthereumThresholdSigner::retry_queues(
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
	ExtBuilder::new()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			let retry_block = frame_system::Pallet::<Test>::current_block_number() +
				<Test as crate::Config<Instance1>>::CeremonyRetryDelay::get();
			assert_eq!(EthereumThresholdSigner::retry_queues(retry_block).len(), 1);
		});
}

#[cfg(test)]
mod unsigned_validation {
	use super::*;
	use crate::{Call as PalletCall, LiveCeremonies, PendingCeremonies, RetryPolicy, RetryQueues};
	use cf_chains::ChainCrypto;
	use cf_traits::{KeyProvider, ThresholdSigner};
	use frame_support::{pallet_prelude::InvalidTransaction, unsigned::TransactionSource};
	use sp_runtime::traits::ValidateUnsigned;

	#[test]
	fn start_custom_signing_ceremony() {
		new_test_ext().execute_with(|| {
			const PAYLOAD: <MockEthereum as ChainCrypto>::Payload = *b"OHAI";
			const CUSTOM_AGG_KEY: <MockEthereum as ChainCrypto>::AggKey = *b"AKEY";
			let participants: BTreeSet<u64> = BTreeSet::from_iter([1, 2, 3, 4, 5, 6]);
			let (request_id, ceremony_id_from_req) =
				EthereumThresholdSigner::request_signature_with(
					CUSTOM_AGG_KEY.into(),
					participants.clone(),
					PAYLOAD,
					RetryPolicy::Never,
				);
			let (ceremony_id, _) = LiveCeremonies::<Test, _>::get(request_id).unwrap();
			assert_eq!(ceremony_id, ceremony_id_from_req);
			let ceremony = PendingCeremonies::<Test, Instance1>::get(ceremony_id).unwrap();
			let timeout_delay: <Test as frame_system::Config>::BlockNumber =
				EthereumThresholdSigner::threshold_signature_response_timeout().into();
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + timeout_delay;
			assert_eq!(ceremony.clone().key_id, &CUSTOM_AGG_KEY);
			assert_eq!(ceremony.remaining_respondents, participants);
			assert_eq!(ceremony.request_context.retry_policy, RetryPolicy::Never);
			// Process retries.
			<EthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);
			assert!(RetryQueues::<Test, Instance1>::take(retry_block).is_empty());
			assert!(PendingCeremonies::<Test, Instance1>::take(retry_block).is_none());
		});
	}

	#[test]
	fn valid_unsigned_extrinsic() {
		new_test_ext().execute_with(|| {
			const PAYLOAD: <MockEthereum as ChainCrypto>::Payload = *b"OHAI";
			// Initiate request
			let request_id =
				<EthereumThresholdSigner as ThresholdSigner<_>>::request_signature(PAYLOAD);
			let (ceremony_id, _) = LiveCeremonies::<Test, _>::get(request_id).unwrap();
			assert!(
				Test::validate_unsigned(
					TransactionSource::External,
					&PalletCall::signature_success { ceremony_id, signature: sign(PAYLOAD) }.into(),
				)
				.is_ok(),
				"Validation Failed: {:?} / {:?} / {:?}",
				MockKeyProvider::current_key(),
				MockKeyProvider::current_key_id(),
				<[u8; 4]>::try_from(MockKeyProvider::current_key_id()).unwrap()
			);
		});
	}

	#[test]
	fn reject_invalid_ceremony() {
		new_test_ext().execute_with(|| {
			const PAYLOAD: <MockEthereum as ChainCrypto>::Payload = *b"OHAI";
			assert_eq!(
				Test::validate_unsigned(
					TransactionSource::External,
					&PalletCall::signature_success { ceremony_id: 1234, signature: sign(PAYLOAD) }
						.into()
				)
				.unwrap_err(),
				InvalidTransaction::Stale.into()
			);
		});
	}

	#[test]
	fn reject_invalid_signature() {
		new_test_ext().execute_with(|| {
			const PAYLOAD: <MockEthereum as ChainCrypto>::Payload = *b"OHAI";
			// Initiate request
			let request_id =
				<EthereumThresholdSigner as ThresholdSigner<_>>::request_signature(PAYLOAD);
			let (ceremony_id, _) = LiveCeremonies::<Test, _>::get(request_id).unwrap();
			assert_eq!(
				Test::validate_unsigned(
					TransactionSource::External,
					&PalletCall::signature_success { ceremony_id, signature: INVALID_SIGNATURE }
						.into()
				)
				.unwrap_err(),
				InvalidTransaction::BadProof.into()
			);
		});
	}

	#[test]
	fn reject_invalid_call() {
		new_test_ext().execute_with(|| {
			assert_eq!(
				EthereumThresholdSigner::validate_unsigned(
					TransactionSource::External,
					&PalletCall::report_signature_failed { id: 0, offenders: Default::default() }
				)
				.unwrap_err(),
				InvalidTransaction::Call.into()
			);
		});
	}
}

#[cfg(test)]
mod failure_reporting {
	use super::*;
	use crate::CeremonyContext;
	use cf_chains::ChainCrypto;
	use cf_traits::{mocks::epoch_info::MockEpochInfo, KeyProvider, RetryPolicy};

	fn init_context(
		validator_set: impl IntoIterator<Item = <Test as Chainflip>::ValidatorId> + Copy,
	) -> CeremonyContext<Test, Instance1> {
		const PAYLOAD: <MockEthereum as ChainCrypto>::Payload = *b"OHAI";
		MockEpochInfo::set_authorities(Vec::from_iter(validator_set));
		CeremonyContext::<Test, Instance1> {
			request_context: RequestContext::<Test, Instance1> {
				request_id: 1,
				attempt_count: 0,
				key_id: Some(MockKeyProvider::current_key_id()),
				payload: PAYLOAD,
				retry_policy: RetryPolicy::Always,
			},
			key_id: MockKeyProvider::current_key_id(),
			remaining_respondents: BTreeSet::from_iter(validator_set),
			blame_counts: Default::default(),
			participant_count: 5,
			_phantom: Default::default(),
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
		assert_eq!(ctx.offenders(), vec![4, 5], "Context was {:?}.", ctx);

		// Fourth report, reporting threshold passed.
		report(&mut ctx, 4, vec![1]);

		// Status: 4 responses in, votes: [1:3, 2:1]
		// Vote threshold has not been met for authority `1`, and `5` has not responded.
		// As things stand, [5] would be reported.
		assert_eq!(ctx.offenders(), vec![5], "Context was {:?}.", ctx);

		// Fifth report, reporting threshold passed.
		report(&mut ctx, 5, vec![1, 2]);

		// Status: 5 responses in, votes: [1:4, 2:2]. Only 1 has met the vote threshold.
		assert_eq!(ctx.offenders(), vec![1], "Context was {:?}.", ctx);
	}
}
