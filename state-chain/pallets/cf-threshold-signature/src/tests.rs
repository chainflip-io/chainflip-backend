use std::{
	collections::BTreeSet,
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Scenario {
	HappyPath,
	RetryPath,
	InvalidThresholdSignaturePath,
}

struct MockCfe;

fn bounded_set_from<T: Ord, S: Get<u32>>(v: impl IntoIterator<Item = T>) -> BoundedBTreeSet<T, S> {
	BoundedBTreeSet::try_from(BTreeSet::from_iter(v)).unwrap()
}

impl MockCfe {
	fn respond(scenario: Scenario) {
		let events = System::events();
		System::reset_events();
		for event_record in events {
			Self::process_event(event_record.event, scenario);
		}
	}

	fn process_event(event: Event, scenario: Scenario) {
		match event {
			Event::DogeThresholdSigner(
				pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
					req_id,
					key_id,
					signers,
					payload,
				),
			) => {
				assert_eq!(key_id, MOCK_KEY_ID);
				assert_eq!(signers, vec![RANDOM_NOMINEE]);
				assert_eq!(payload, DOGE_PAYLOAD);

				match scenario {
					Scenario::HappyPath => {
						assert_ok!(DogeThresholdSigner::signature_success(
							Origin::root(),
							req_id,
							VALID_SIGNATURE.to_string(),
						));
					},
					Scenario::RetryPath => {
						assert_ok!(DogeThresholdSigner::report_signature_failed(
							Origin::signed(1),
							req_id,
							bounded_set_from([RANDOM_NOMINEE]),
						));
					},
					Scenario::InvalidThresholdSignaturePath => {
						assert_noop!(
							DogeThresholdSigner::signature_success(
								Origin::root(),
								req_id,
								INVALID_SIGNATURE.to_string(),
							),
							Error::<Test, Instance1>::InvalidThresholdSignature
						);
					},
				};
			},
			_ => panic!("Unexpected event"),
		};
	}
}

#[test]
fn happy_path() {
	new_test_ext().execute_with(|| {
		// Initiate request
		let request_id = DogeThresholdSigner::request_signature(DogeThresholdSignerContext {
			message: "Amazing!".to_string(),
		});
		let pending = DogeThresholdSigner::pending_request(request_id).unwrap();
		assert_eq!(pending.attempt, 0);
		assert_eq!(pending.remaining_respondents, BTreeSet::from_iter([RANDOM_NOMINEE]));

		// Wrong request id is a no-op
		assert_noop!(
			DogeThresholdSigner::signature_success(
				Origin::root(),
				request_id + 1,
				"MaliciousSignature".to_string()
			),
			Error::<Test, Instance1>::InvalidCeremonyId
		);

		// CFE responds
		MockCfe::respond(Scenario::HappyPath);

		// Request is complete
		assert!(DogeThresholdSigner::pending_request(request_id).is_none());

		// Call back has executed.
		assert_eq!(
			MockCallback::<Doge>::get_stored_callback(),
			Some("So Amazing! Such Wow!".to_string())
		);
	});
}

#[test]
fn retry_path() {
	new_test_ext().execute_with(|| {
		// Initiate request
		let request_id = DogeThresholdSigner::request_signature(DogeThresholdSignerContext {
			message: "Amazing!".to_string(),
		});
		let pending = DogeThresholdSigner::pending_request(request_id).unwrap();
		assert_eq!(pending.attempt, 0);
		assert_eq!(pending.remaining_respondents, BTreeSet::from_iter([RANDOM_NOMINEE]));

		// CFE responds
		MockCfe::respond(Scenario::RetryPath);

		// Request is complete
		assert!(DogeThresholdSigner::pending_request(request_id).is_none());

		// Call back has *not* executed.
		assert_eq!(MockCallback::<Doge>::get_stored_callback(), None);

		// The offender has been reported.
		assert_eq!(MockOfflineReporter::get_reported(), vec![RANDOM_NOMINEE]);

		// Scheduled for retry.
		assert_eq!(DogeThresholdSigner::retry_queue().len(), 1);

		// Process retries.
		<DogeThresholdSigner as Hooks<BlockNumberFor<Test>>>::on_initialize(0);

		// No longer pending retry.
		assert!(DogeThresholdSigner::retry_queue().is_empty());

		// We have a new request pending.
		let pending = DogeThresholdSigner::pending_request(request_id + 1).unwrap();
		assert_eq!(pending.attempt, 1);
		assert_eq!(pending.remaining_respondents, BTreeSet::from_iter([RANDOM_NOMINEE]));
	});
}

#[test]
fn invalid_threshold_signature_path() {
	new_test_ext().execute_with(|| {
		// Initiate request
		let _request_id = DogeThresholdSigner::request_signature(DogeThresholdSignerContext {
			message: "So threshold!".to_string(),
		});

		// CFE responds
		MockCfe::respond(Scenario::InvalidThresholdSignaturePath);

		// TODO: Define what behaviour we expect from here.
	});
}

#[cfg(test)]
mod failure_reporting {
	use cf_traits::mocks::epoch_info::MockEpochInfo;
	use crate::RequestContext;
	use super::*;

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
	fn basic_thresholds()
	{
		let mut ctx = init_context([1, 2, 3, 4, 5]);

		// No reports yet.
		assert!(!ctx.countdown_threshold_reached());

		// First report, not enough to trigger countdown.
		report(&mut ctx, 1, vec![2]);
		assert!(!ctx.countdown_threshold_reached());

		// Second report, countdown threshold passed.
		report(&mut ctx, 2, vec![1]);
		assert!(ctx.countdown_threshold_reached());
		
		// Third report, countdown threshold passed.
		report(&mut ctx, 3, vec![1]);
		assert!(ctx.countdown_threshold_reached());

		// Status: 3 responses in, votes: [1:2, 2:1]
		// Vote threshold not met, but two validators have failed to respond - they would be reported.
		assert_eq!(ctx.offenders(), vec![4, 5], "Context was {:?}.", ctx);

		// Fourth report, reporting threshold passed.
		report(&mut ctx, 4, vec![1]);
		assert!(ctx.countdown_threshold_reached());

		// Status: 4 responses in, votes: [1:3, 2:1]
		// Vote threshold has been met for validator `1`, and `5` has not responded. Both should be reported.
		assert_eq!(ctx.offenders(), vec![1, 5], "Context was {:?}.", ctx);

		// Fifth report, reporting threshold passed.
		report(&mut ctx, 5, vec![1, 2]);
		assert!(ctx.countdown_threshold_reached());

		// Status: 5 responses in, votes: [1:4, 2:2]. Only 1 has met the vote threshold.
		assert_eq!(ctx.offenders(), vec![1], "Context was {:?}.", ctx);
	}
}
