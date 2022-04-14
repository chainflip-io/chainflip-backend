use std::{
	collections::{BTreeMap, BTreeSet},
	convert::TryFrom,
	iter::{FromIterator, IntoIterator},
};

use crate::{
	self as pallet_cf_threshold_signature, mock::*, AttemptCount, CeremonyContext, CeremonyId,
	Error, PalletOffence, RequestId,
};
use cf_chains::mocks::MockEthereum;
use cf_traits::{AsyncResult, Chainflip};
use frame_support::{
	assert_noop, assert_ok,
	instances::Instance1,
	storage::bounded_btree_set::BoundedBTreeSet,
	traits::{Get, Hooks},
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::BlockNumberProvider;

fn bounded_set_from_iter<T: Ord, S: Get<u32>>(
	members: impl IntoIterator<Item = T>,
) -> BoundedBTreeSet<T, S> {
	BoundedBTreeSet::try_from(BTreeSet::from_iter(members)).unwrap()
}

fn get_ceremony_context(
	ceremony_id: CeremonyId,
	expected_request_id: RequestId,
	expected_attempt: AttemptCount,
) -> CeremonyContext<Test, Instance1> {
	let (request_id, attempt, _) =
		MockEthereumThresholdSigner::open_requests(ceremony_id).expect("Expected a request_id");
	assert_eq!(request_id, expected_request_id);
	assert_eq!(attempt, expected_attempt);
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
				assert_eq!(key_id, MOCK_KEY_ID);
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
								bounded_set_from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						// Unsolicited responses are rejected.
						assert_noop!(
							MockEthereumThresholdSigner::report_signature_failed(
								Origin::signed(signers.iter().max().unwrap() + 1),
								req_id,
								bounded_set_from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidRespondent
						);

						assert_ok!(MockEthereumThresholdSigner::report_signature_failed(
							Origin::signed(self.id),
							req_id,
							bounded_set_from_iter(bad.clone()),
						));

						// Can't respond twice.
						assert_noop!(
							MockEthereumThresholdSigner::report_signature_failed(
								Origin::signed(self.id),
								req_id,
								bounded_set_from_iter(bad.clone()),
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
	const VALIDATORS: [u64; 3] = [1, 2, 3];
	ExtBuilder::new()
		.with_validators(VALIDATORS)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			let ceremony_id = current_ceremony_id();
			let (request_id, ..) = MockEthereumThresholdSigner::open_requests(ceremony_id).unwrap();
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
	const VALIDATORS: [u64; 3] = [1, 2, 3];
	ExtBuilder::new()
		.with_validators(VALIDATORS)
		.with_nominees(NOMINEES)
		.with_request_and_callback(b"OHAI", MockCallback::new)
		.build()
		.execute_with(|| {
			let ceremony_id = current_ceremony_id();
			let (request_id, ..) = MockEthereumThresholdSigner::open_requests(ceremony_id).unwrap();
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
	const VALIDATORS: [u64; 3] = [1, 2, 3];
	ExtBuilder::new()
		.with_validators(VALIDATORS)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			let ceremony_id = current_ceremony_id();
			let (request_id, attempt, _) =
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

			// Callback has *not* executed but is scheduled for a retry in 10 blocks' time.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + 10;
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
			let context = get_ceremony_context(ceremony_id + 1, request_id, attempt + 1);
			assert_eq!(
				context.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap().into_iter())
			);
		});
}

#[test]
fn fail_path_no_timeout() {
	const NOMINEES: [u64; 5] = [1, 2, 3, 4, 5];
	const VALIDATORS: [u64; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
	ExtBuilder::new()
		.with_validators(VALIDATORS)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			let ceremony_id = current_ceremony_id();
			let (request_id, attempt, _) =
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

			// Callback has *not* executed but is scheduled for a retry both in the next block *and*
			// in 10 blocks' time.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + 1;
			let retry_block_redundant = frame_system::Pallet::<Test>::current_block_number() + 10;
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
			let pending = get_ceremony_context(ceremony_id + 1, request_id, attempt + 1);
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
	const VALIDATORS: [u64; 5] = [1, 2, 3, 4, 5];
	ExtBuilder::new()
		.with_validators(VALIDATORS)
		.with_nominees(NOMINEES)
		.with_request(b"OHAI")
		.build()
		.execute_with(|| {
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + 1;
			assert_eq!(MockEthereumThresholdSigner::retry_queues(retry_block).len(), 1);
		});
}

#[cfg(test)]
mod unsigned_validation {
	use super::*;
	use crate::Call as PalletCall;
	use cf_chains::ChainCrypto;
	use frame_support::{pallet_prelude::InvalidTransaction, unsigned::TransactionSource};
	use sp_runtime::traits::ValidateUnsigned;

	#[test]
	fn valid_unsigned_extrinsic() {
		new_test_ext().execute_with(|| {
			const PAYLOAD: <MockEthereum as ChainCrypto>::Payload = *b"OHAI";
			// Initiate request
			let (_, ceremony_id) = MockEthereumThresholdSigner::request_signature(PAYLOAD);
			assert_ok!(Test::validate_unsigned(
				TransactionSource::External,
				&PalletCall::signature_success(ceremony_id, sign(PAYLOAD)).into()
			));
		});
	}

	#[test]
	fn reject_invalid_ceremony() {
		new_test_ext().execute_with(|| {
			const PAYLOAD: <MockEthereum as ChainCrypto>::Payload = *b"OHAI";
			assert_eq!(
				Test::validate_unsigned(
					TransactionSource::External,
					&PalletCall::signature_success(1234, sign(PAYLOAD)).into()
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
			let (_, ceremony_id) = MockEthereumThresholdSigner::request_signature(PAYLOAD);
			assert_eq!(
				Test::validate_unsigned(
					TransactionSource::External,
					&PalletCall::signature_success(ceremony_id, INVALID_SIGNATURE).into()
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
					&PalletCall::report_signature_failed(0, Default::default(),)
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
	use cf_traits::mocks::epoch_info::MockEpochInfo;

	fn init_context(
		validator_set: impl IntoIterator<Item = <Test as Chainflip>::ValidatorId> + Copy,
	) -> CeremonyContext<Test, Instance1> {
		MockEpochInfo::set_validators(Vec::from_iter(validator_set));
		CeremonyContext::<Test, Instance1> {
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
		// Vote threshold has not been met for validator `1`, and `5` has not responded.
		// As things stand, [5] would be reported.
		assert_eq!(ctx.offenders(), vec![5], "Context was {:?}.", ctx);

		// Fifth report, reporting threshold passed.
		report(&mut ctx, 5, vec![1, 2]);

		// Status: 5 responses in, votes: [1:4, 2:2]. Only 1 has met the vote threshold.
		assert_eq!(ctx.offenders(), vec![1], "Context was {:?}.", ctx);
	}
}
