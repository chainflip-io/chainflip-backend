use std::{
	collections::{BTreeMap, BTreeSet},
	convert::TryFrom,
	iter::{FromIterator, IntoIterator},
};

use crate::{self as pallet_cf_threshold_signature, mock::*, Error};
use cf_traits::Chainflip;
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

impl MockCfe {
	fn process_event(&self, event: Event) {
		match event {
			Event::DogeThresholdSigner(
				pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
					req_id,
					key_id,
					signers,
					_payload,
				),
			) => {
				assert_eq!(key_id, MOCK_KEY_ID);
				assert_eq!(signers, MockNominator::get_nominees().unwrap());

				match &self.behaviour {
					CfeBehaviour::Success => {
						// Wrong request id is a no-op
						assert_noop!(
							DogeThresholdSigner::signature_success(
								Origin::none(),
								req_id + 1,
								VALID_SIGNATURE
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						assert_ok!(DogeThresholdSigner::signature_success(
							Origin::none(),
							req_id,
							VALID_SIGNATURE,
						));
					},
					CfeBehaviour::ReportFailure(bad) => {
						// Invalid ceremony id.
						assert_noop!(
							DogeThresholdSigner::report_signature_failed(
								Origin::signed(self.id),
								req_id * 2,
								bounded_set_from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidCeremonyId
						);

						// Unsolicited responses are rejected.
						assert_noop!(
							DogeThresholdSigner::report_signature_failed(
								Origin::signed(signers.iter().max().unwrap() + 1),
								req_id,
								bounded_set_from_iter(bad.clone()),
							),
							Error::<Test, Instance1>::InvalidRespondent
						);

						assert_ok!(DogeThresholdSigner::report_signature_failed(
							Origin::signed(self.id),
							req_id,
							bounded_set_from_iter(bad.clone()),
						));

						// Can't respond twice.
						assert_noop!(
							DogeThresholdSigner::report_signature_failed(
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
fn happy_path() {
	const NOMINEES: [u64; 2] = [1, 2];
	const VALIDATORS: [u64; 3] = [1, 2, 3];
	ExtBuilder::new()
		.with_validators(VALIDATORS)
		.with_nominees(NOMINEES)
		.with_pending_request("Woof!")
		.build()
		.execute_with(|| {
			let ceremony_id = DogeThresholdSigner::ceremony_id_counter();
			let cfe = MockCfe { id: 1, behaviour: CfeBehaviour::Success };

			tick(&[cfe]);

			// Request is complete
			assert!(DogeThresholdSigner::pending_request(ceremony_id).is_none());

			// Callback has executed.
			assert!(MockCallback::<Doge>::has_executed());
		});
}

#[test]
fn fail_path_with_timeout() {
	const NOMINEES: [u64; 2] = [1, 2];
	const VALIDATORS: [u64; 3] = [1, 2, 3];
	ExtBuilder::new()
		.with_validators(VALIDATORS)
		.with_nominees(NOMINEES)
		.with_pending_request("Woof!")
		.build()
		.execute_with(|| {
			let ceremony_id = DogeThresholdSigner::ceremony_id_counter();
			let cfes = [
				MockCfe { id: 1, behaviour: CfeBehaviour::Timeout },
				MockCfe { id: 2, behaviour: CfeBehaviour::ReportFailure(vec![1]) },
			];

			// CFEs respond
			tick(&cfes[..]);

			// Request is still pending waiting for account 1.
			let request_context = DogeThresholdSigner::pending_request(ceremony_id).unwrap();

			// Account 1 has 1 blame vote against it.
			assert_eq!(request_context.blame_counts, BTreeMap::from_iter([(1, 1)]));

			// We have reach the threshold to start the retry countdown.
			assert!(request_context.countdown_initiation_threshold_reached());

			// Callback has *not* executed but is scheduled for a retry in 10 blocks' time.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + 10;
			assert!(!MockCallback::<Doge>::has_executed());
			assert_eq!(DogeThresholdSigner::retry_queues(retry_block).len(), 1);

			// The offender has not yet been reported.
			assert!(MockOfflineReporter::get_reported().is_empty());

			// Process retries.
			<DogeThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);

			// No longer pending retry.
			assert!(DogeThresholdSigner::retry_queues(retry_block).is_empty());

			// Participant 1 was reported for not responding.
			assert_eq!(MockOfflineReporter::get_reported(), vec![1]);

			// We have a new request pending: New ceremony_id, same request context.
			let pending = DogeThresholdSigner::pending_request(ceremony_id + 1).unwrap();
			assert_eq!(pending.attempt, request_context.attempt + 1);
			assert_eq!(pending.chain_signing_context, request_context.chain_signing_context);
			assert_eq!(
				pending.remaining_respondents,
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
		.with_pending_request("Woof!")
		.build()
		.execute_with(|| {
			let ceremony_id = DogeThresholdSigner::ceremony_id_counter();
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
			let request_context = DogeThresholdSigner::pending_request(ceremony_id).unwrap();
			assert!(request_context.retry_scheduled);

			// Account 1 has 4 blame votes against it.
			assert_eq!(request_context.blame_counts, BTreeMap::from_iter([(1, 4)]));

			// We have reach the threshold to start the retry countdown.
			assert!(request_context.countdown_initiation_threshold_reached());

			// Callback has *not* executed but is scheduled for a retry both in the next block *and*
			// in 10 blocks' time.
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + 1;
			let retry_block_redundant = frame_system::Pallet::<Test>::current_block_number() + 10;
			assert!(!MockCallback::<Doge>::has_executed());
			assert_eq!(DogeThresholdSigner::retry_queues(retry_block).len(), 1);
			assert_eq!(DogeThresholdSigner::retry_queues(retry_block_redundant).len(), 1);

			// The offender has not yet been reported.
			assert!(MockOfflineReporter::get_reported().is_empty());

			// Process retries.
			<DogeThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(retry_block);

			// No longer pending retry.
			assert!(DogeThresholdSigner::retry_queues(retry_block).is_empty());

			// We did reach the reporting threshold, participant 1 was reported.
			assert_eq!(MockOfflineReporter::get_reported(), vec![1]);

			// We have a new request pending: New ceremony_id, same request context.
			let pending = DogeThresholdSigner::pending_request(ceremony_id + 1).unwrap();
			assert_eq!(pending.attempt, request_context.attempt + 1);
			assert_eq!(pending.chain_signing_context, request_context.chain_signing_context);
			assert_eq!(
				pending.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap().into_iter())
			);

			// Processing the redundant retry request has no effect.
			<DogeThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(
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
		.with_pending_request("Woof!")
		.build()
		.execute_with(|| {
			let ceremony_id = DogeThresholdSigner::ceremony_id_counter();
			let request_context = DogeThresholdSigner::pending_request(ceremony_id).unwrap();
			assert!(request_context.retry_scheduled);
			let retry_block = frame_system::Pallet::<Test>::current_block_number() + 1;
			assert_eq!(DogeThresholdSigner::retry_queues(retry_block).len(), 1);
		});
}

#[cfg(test)]
mod unsigned_validation {
	use super::*;
	use crate::Call as DogeCall;
	use frame_support::{pallet_prelude::InvalidTransaction, unsigned::TransactionSource};
	use sp_runtime::traits::ValidateUnsigned;

	#[test]
	fn valid_unsigned_extrinsic() {
		new_test_ext().execute_with(|| {
			// Initiate request
			let request_id = DogeThresholdSigner::request_signature(DogeThresholdSignerContext {
				message: "Woof!".to_string(),
			});
			assert_ok!(Test::validate_unsigned(
				TransactionSource::External,
				&DogeCall::signature_success(request_id, DogeSig::Valid).into()
			));
		});
	}

	#[test]
	fn reject_invalid_ceremony() {
		new_test_ext().execute_with(|| {
			assert_eq!(
				Test::validate_unsigned(
					TransactionSource::External,
					&DogeCall::signature_success(1234, DogeSig::Valid).into()
				)
				.unwrap_err(),
				InvalidTransaction::Stale.into()
			);
		});
	}

	#[test]
	fn reject_invalid_signature() {
		new_test_ext().execute_with(|| {
			// Initiate request
			let request_id = DogeThresholdSigner::request_signature(DogeThresholdSignerContext {
				message: "Woof!".to_string(),
			});
			assert_eq!(
				Test::validate_unsigned(
					TransactionSource::External,
					&DogeCall::signature_success(request_id, DogeSig::Invalid).into()
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
				DogeThresholdSigner::validate_unsigned(
					TransactionSource::External,
					&DogeCall::report_signature_failed(0, Default::default(),)
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
	use crate::RequestContext;
	use cf_traits::mocks::epoch_info::MockEpochInfo;

	fn init_context(
		validator_set: impl IntoIterator<Item = <Test as Chainflip>::ValidatorId> + Copy,
	) -> RequestContext<Test, Instance1> {
		MockEpochInfo::set_validators(Vec::from_iter(validator_set));
		RequestContext::<Test, Instance1> {
			attempt: 0,
			retry_scheduled: false,
			remaining_respondents: BTreeSet::from_iter(validator_set),
			blame_counts: Default::default(),
			participant_count: 5,
			chain_signing_context: Default::default(),
		}
	}

	fn report(context: &mut RequestContext<Test, Instance1>, reporter: u64, blamed: Vec<u64>) {
		for i in blamed {
			*context.blame_counts.entry(i).or_default() += 1;
		}
		context.remaining_respondents.remove(&reporter);
	}

	#[test]
	fn basic_thresholds() {
		let mut ctx = init_context([1, 2, 3, 4, 5]);

		// No reports yet.
		assert!(!ctx.countdown_initiation_threshold_reached());

		// First report, countdown threshold passed.
		report(&mut ctx, 1, vec![2]);
		assert!(ctx.countdown_initiation_threshold_reached());

		// Second report, countdown threshold passed.
		report(&mut ctx, 2, vec![1]);
		assert!(ctx.countdown_initiation_threshold_reached());

		// Third report, countdown threshold passed.
		report(&mut ctx, 3, vec![1]);
		assert!(ctx.countdown_initiation_threshold_reached());

		// Status: 3 responses in, votes: [1:2, 2:1]
		// Vote threshold not met, but two validators have failed to respond - they would be
		// reported.
		assert_eq!(ctx.offenders(), vec![4, 5], "Context was {:?}.", ctx);

		// Fourth report, reporting threshold passed.
		report(&mut ctx, 4, vec![1]);
		assert!(ctx.countdown_initiation_threshold_reached());

		// Status: 4 responses in, votes: [1:3, 2:1]
		// Vote threshold has not been met for validator `1`, and `5` has not responded.
		// As things stand, [5] would be reported.
		assert_eq!(ctx.offenders(), vec![5], "Context was {:?}.", ctx);

		// Fifth report, reporting threshold passed.
		report(&mut ctx, 5, vec![1, 2]);
		assert!(ctx.countdown_initiation_threshold_reached());

		// Status: 5 responses in, votes: [1:4, 2:2]. Only 1 has met the vote threshold.
		assert_eq!(ctx.offenders(), vec![1], "Context was {:?}.", ctx);
	}
}
