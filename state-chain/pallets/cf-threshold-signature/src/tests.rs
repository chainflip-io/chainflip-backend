use std::collections::{BTreeMap, BTreeSet};

use crate::{
	self as pallet_cf_threshold_signature, mock::*, AttemptCount, CeremonyContext, CeremonyId,
	Error, PalletOffence, RequestContext, RequestId,
	THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS_DEFAULT,
};
use cf_chains::mocks::MockEthereum;
use cf_traits::{AsyncResult, Chainflip};
use frame_support::{assert_noop, assert_ok, instances::Instance1, traits::Hooks};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::BlockNumberProvider;

fn get_ceremony_context(
	ceremony_id: CeremonyId,
	expected_request_id: RequestId,
	expected_attempt: AttemptCount,
) -> CeremonyContext<Test, Instance1> {
	let RequestContext { request_id, attempt_count, .. } =
		MockEthereumThresholdSigner::open_requests(ceremony_id).expect("Expected a request_id");
	assert_eq!(request_id, expected_request_id);
	assert_eq!(attempt_count, expected_attempt);
	MockEthereumThresholdSigner::pending_ceremonies(ceremony_id)
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

fn tick(cfes: &[MockCfe]) {
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
			Event::MockEthereumThresholdSigner(
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
							MockEthereumThresholdSigner::signature_success(
								Origin::none(),
								req_id + 1,
								sign(payload)
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						assert_ok!(MockEthereumThresholdSigner::signature_success(
							Origin::none(),
							req_id,
							sign(payload),
						));
					},
					CfeBehaviour::ReportFailure(bad) => {
						// Invalid ceremony id.
						assert_noop!(
							MockEthereumThresholdSigner::report_signature_failed(
								Origin::signed(self.id),
								req_id * 2,
								BTreeSet::from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						// Unsolicited responses are rejected.
						assert_noop!(
							MockEthereumThresholdSigner::report_signature_failed(
								Origin::signed(signers.iter().max().unwrap() + 1),
								req_id,
								BTreeSet::from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidRespondent
						);

						assert_ok!(MockEthereumThresholdSigner::report_signature_failed(
							Origin::signed(self.id),
							req_id,
							BTreeSet::from_iter(bad.clone()),
						));

						// Can't respond twice.
						assert_noop!(
							MockEthereumThresholdSigner::report_signature_failed(
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
			let RequestContext { request_id, .. } =
				MockEthereumThresholdSigner::open_requests(ceremony_id).unwrap();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			tick(&[cfe]);

			// Request is complete
			assert!(MockEthereumThresholdSigner::pending_ceremonies(ceremony_id).is_none());

			// Signature is available
			assert!(matches!(
				MockEthereumThresholdSigner::signatures(request_id),
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
			let RequestContext { request_id, .. } =
				MockEthereumThresholdSigner::open_requests(ceremony_id).unwrap();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			tick(&[cfe]);

			// Request is complete
			assert!(MockEthereumThresholdSigner::pending_ceremonies(ceremony_id).is_none());

			// Callback has triggered.
			assert!(MockCallback::has_executed(request_id));

			// Signature has been consumed.
			assert!(
				matches!(MockEthereumThresholdSigner::signatures(request_id), AsyncResult::Void),
				"Expected Void, got {:?}",
				MockEthereumThresholdSigner::signatures(request_id)
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
			let RequestContext { request_id, attempt_count, .. } =
				MockEthereumThresholdSigner::open_requests(ceremony_id).unwrap();
			let cfes = [
				MockCfe { id: 1, behaviour: CfeBehaviour::Timeout },
				MockCfe { id: 2, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
			];

			// CFEs respond
			tick(&cfes[..]);

			// Request is still pending waiting for account 1.
			let request_context =
				MockEthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();

			// Account 1 has 1 blame vote against it.
			assert_eq!(request_context.blame_counts, BTreeMap::from_iter([(1, 1)]));

			let timeout_delay: <Test as frame_system::Config>::BlockNumber =
				THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS_DEFAULT.into();
			// Callback has *not* executed but is scheduled for a retry after the timeout has
			// elapsed.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + timeout_delay;

			assert!(!MockCallback::has_executed(request_id));
			assert_eq!(MockEthereumThresholdSigner::retry_queues(retry_block).len(), 1);

			// The offender has not yet been reported.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![]);

			// Process retries.
			<MockEthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				retry_block,
			);

			// Expect the retry queue to be empty
			assert!(MockEthereumThresholdSigner::retry_queues(retry_block).is_empty());

			// Participant 1 was reported for not responding.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![1]);

			// We have a new request pending: New ceremony_id, same request context.
			let context = get_ceremony_context(ceremony_id + 1, request_id, attempt_count + 1);
			assert_eq!(
				context.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap().into_iter())
			);
		});
}

#[test]
fn fail_path_no_timeout() {
	const NOMINEES: [u64; 5] = [1, 2, 3, 4, 5];
	const AUTHORITIES: [u64; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
	ExtBuilder::new()
		.with_authorities(AUTHORITIES)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			let ceremony_id = current_ceremony_id();
			let RequestContext { request_id, attempt_count, .. } =
				MockEthereumThresholdSigner::open_requests(ceremony_id).unwrap();
			let cfes = [
				MockCfe { id: 1, behaviour: CfeBehaviour::ReportFailure(vec![]) },
				MockCfe { id: 2, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
				MockCfe { id: 3, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
				MockCfe { id: 4, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
				MockCfe { id: 5, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
			];

			// CFEs respond
			tick(&cfes[..]);

			// Request is still in pending state but scheduled for retry.
			let request_context =
				MockEthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();

			// Account 1 has 4 blame votes against it.
			assert_eq!(request_context.blame_counts, BTreeMap::from_iter([(1, 4)]));

			// Callback has *not* executed but is scheduled for a retry after the CeremonyRetryDelay
			// *and* the threshold timeout.
			let ceremony_retry_delay =
				<Test as crate::Config<Instance1>>::CeremonyRetryDelay::get();
			let init_timeout_delay: <Test as frame_system::Config>::BlockNumber =
				THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS_DEFAULT.into();
			let retry_block =
				frame_system::Pallet::<Test>::current_block_number() + ceremony_retry_delay;
			let retry_block_redundant =
				frame_system::Pallet::<Test>::current_block_number() + init_timeout_delay;

			assert!(!MockCallback::has_executed(request_id));
			assert_eq!(MockEthereumThresholdSigner::retry_queues(retry_block).len(), 1);
			assert_eq!(MockEthereumThresholdSigner::retry_queues(retry_block_redundant).len(), 1);

			// The offender has not yet been reported.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![]);

			// Process retries.
			<MockEthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				retry_block,
			);

			// No longer pending retry.
			assert!(MockEthereumThresholdSigner::retry_queues(retry_block).is_empty());

			// We did reach the reporting threshold, participant 1 was reported.
			MockOffenceReporter::assert_reported(PalletOffence::ParticipateSigningFailed, vec![1]);

			// We have a new request pending: New ceremony_id, same request context.
			let pending = get_ceremony_context(ceremony_id + 1, request_id, attempt_count + 1);
			assert_eq!(
				pending.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap().into_iter())
			);

			// Processing the redundant retry request has no effect.
			<MockEthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				retry_block_redundant,
			);
		});
}

#[test]
fn test_not_enough_signers_for_threshold() {
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
			assert_eq!(MockEthereumThresholdSigner::retry_queues(retry_block).len(), 1);
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
			let participants: Vec<u64> = vec![1, 2, 3, 4, 5, 6];
			let request_id = MockEthereumThresholdSigner::request_signature_with(
				CUSTOM_AGG_KEY.into(),
				participants.clone(),
				PAYLOAD,
				RetryPolicy::Never,
			);
			let (ceremony_id, _) = LiveCeremonies::<Test, _>::get(request_id).unwrap();
			let ceremony = PendingCeremonies::<Test, Instance1>::get(ceremony_id);
			let timeout_delay: <Test as frame_system::Config>::BlockNumber =
				THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS_DEFAULT.into();
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + timeout_delay;
			assert_eq!(ceremony.clone().unwrap().key_id, &CUSTOM_AGG_KEY);
			assert_eq!(ceremony.unwrap().remaining_respondents, BTreeSet::from_iter(participants));
			// Process retries.
			<MockEthereumThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
				retry_block,
			);
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
				<MockEthereumThresholdSigner as ThresholdSigner<_>>::request_signature(PAYLOAD);
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
				<MockEthereumThresholdSigner as ThresholdSigner<_>>::request_signature(PAYLOAD);
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
				MockEthereumThresholdSigner::validate_unsigned(
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
	use cf_traits::{mocks::epoch_info::MockEpochInfo, KeyProvider};

	fn init_context(
		validator_set: impl IntoIterator<Item = <Test as Chainflip>::ValidatorId> + Copy,
	) -> CeremonyContext<Test, Instance1> {
		MockEpochInfo::set_authorities(Vec::from_iter(validator_set));
		CeremonyContext::<Test, Instance1> {
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
