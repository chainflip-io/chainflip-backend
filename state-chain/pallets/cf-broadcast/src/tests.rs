use crate::{
	mock::*, AwaitingBroadcast, AwaitingSignature, BroadcastAttemptId, BroadcastFailure,
	BroadcastId, Error, Event as BroadcastEvent, Instance0, RetryQueue,
};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use frame_system::RawOrigin;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Scenario {
	HappyPath,
	BadSigner,
	BroadcastFailure(BroadcastFailure),
}

thread_local! {
	pub static COMPLETED_BROADCASTS: std::cell::RefCell<Vec<BroadcastId>> = Default::default();
	pub static FAILED_BROADCASTS: std::cell::RefCell<Vec<BroadcastId>> = Default::default();
}

struct MockCfe;

impl MockCfe {
	fn respond(scenario: Scenario) {
		let events = System::events();
		System::reset_events();
		for event_record in events {
			Self::process_event(event_record.event, scenario.clone());
		}
	}

	fn process_event(event: Event, scenario: Scenario) {
		match event {
			Event::pallet_cf_broadcast_Instance0(broadcast_event) => match broadcast_event {
				BroadcastEvent::TransactionSigningRequest(attempt_id, nominee, unsigned_tx) => {
					Self::handle_transaction_signature_request(
						attempt_id,
						nominee,
						unsigned_tx,
						scenario,
					);
				}
				BroadcastEvent::BroadcastRequest(attempt_id, _signed_tx) => {
					Self::handle_broadcast_request(attempt_id, scenario);
				}
				BroadcastEvent::BroadcastComplete(broadcast_id) => {
					COMPLETED_BROADCASTS.with(|cell| cell.borrow_mut().push(broadcast_id));
				}
				BroadcastEvent::RetryScheduled(_, _) => {
					// Informational only. No action required by the CFE.
				}
				BroadcastEvent::BroadcastFailed(broadcast_id, _, _) => {
					FAILED_BROADCASTS.with(|cell| cell.borrow_mut().push(broadcast_id));
				}
				BroadcastEvent::__Ignore(_, _) => unimplemented!(),
			},
			_ => panic!("Unexpected event"),
		};
	}

	// Accepts an unsigned tx, making sure the nominee has been assigned.
	fn handle_transaction_signature_request(
		attempt_id: BroadcastAttemptId,
		nominee: u64,
		_unsigned_tx: MockUnsignedTx,
		scenario: Scenario,
	) {
		assert_eq!(nominee, RANDOM_NOMINEE);
		// Invalid signer refused.
		assert_noop!(
			DogeBroadcast::transaction_ready(
				RawOrigin::Signed(nominee + 1).into(),
				attempt_id,
				MockSignedTx::Valid,
			),
			Error::<Test, Instance0>::InvalidSigner
		);
		// Only the nominee can return the signed tx.
		assert_ok!(DogeBroadcast::transaction_ready(
			RawOrigin::Signed(nominee).into(),
			attempt_id,
			match scenario {
				Scenario::BadSigner => MockSignedTx::Invalid,
				_ => MockSignedTx::Valid,
			},
		));
	}

	// Simulate different outcomes.
	fn handle_broadcast_request(attempt_id: BroadcastAttemptId, scenario: Scenario) {
		assert_ok!(match scenario {
			Scenario::HappyPath =>
				DogeBroadcast::broadcast_success(Origin::root(), attempt_id, [0xcf; 4]),
			Scenario::BroadcastFailure(failure) => {
				DogeBroadcast::broadcast_failure(Origin::root(), attempt_id, failure, [0xcf; 4])
			}
			_ => unimplemented!(),
		});
	}
}

#[test]
fn test_broadcast_happy_path() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;
		const BROADCAST_ATTEMPT_ID: BroadcastAttemptId = 1;

		// Initiate broadcast
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_some());

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_some());

		// CFE responds again with confirmation of a successful broadcast.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());

		// CFE logs the completed broadcast.
		MockCfe::respond(Scenario::HappyPath);
		assert_eq!(
			COMPLETED_BROADCASTS.with(|cell| *cell.borrow().first().unwrap()),
			BROADCAST_ID
		);
	})
}

#[test]
fn test_broadcast_rejected() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ATTEMPT_ID: BroadcastAttemptId = 1;

		// Initiate broadcast
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		assert!(
			AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID)
				.unwrap()
				.attempt_count == 0
		);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_some());

		// CFE responds that the transaction was rejected.
		MockCfe::respond(Scenario::BroadcastFailure(
			BroadcastFailure::TransactionRejected,
		));
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert_eq!(
			RetryQueue::<Test, Instance0>::decode_len().unwrap_or_default(),
			1
		);

		// The `on_initialize` hook is called and triggers a new broadcast attempt.
		DogeBroadcast::on_initialize(0);
		assert_eq!(
			RetryQueue::<Test, Instance0>::decode_len().unwrap_or_default(),
			0
		);
		assert!(
			AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID + 1)
				.unwrap()
				.attempt_count == 1
		);

		// The nominee was not reported.
		assert_eq!(MockOfflineReporter::get_reported(), vec![RANDOM_NOMINEE]);
	})
}

#[test]
fn test_broadcast_failed() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;
		const BROADCAST_ATTEMPT_ID: BroadcastAttemptId = 1;

		// Initiate broadcast
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		assert!(
			AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID)
				.unwrap()
				.attempt_count == 0
		);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_some());

		// CFE responds that the transaction failed.
		MockCfe::respond(Scenario::BroadcastFailure(
			BroadcastFailure::TransactionFailed,
		));
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());

		// We don't retry.
		assert_eq!(
			RetryQueue::<Test, Instance0>::decode_len().unwrap_or_default(),
			0
		);
		// The broadcast has failed.
		MockCfe::respond(Scenario::BroadcastFailure(
			BroadcastFailure::TransactionFailed,
		));
		assert_eq!(
			FAILED_BROADCASTS.with(|cell| *cell.borrow().first().unwrap()),
			BROADCAST_ID
		);
	})
}

#[test]
fn test_bad_signature() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ATTEMPT_ID: BroadcastAttemptId = 1;

		// Initiate broadcast
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		assert!(
			AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID)
				.unwrap()
				.attempt_count == 0
		);

		// CFE responds with an invalid transaction.
		MockCfe::respond(Scenario::BadSigner);

		// Broadcast is removed and scheduled for retry.
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		assert_eq!(
			RetryQueue::<Test, Instance0>::decode_len().unwrap_or_default(),
			1
		);

		// The nominee was reported.
		assert_eq!(MockOfflineReporter::get_reported(), vec![RANDOM_NOMINEE]);
	})
}

#[test]
fn test_invalid_id_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			DogeBroadcast::transaction_ready(RawOrigin::Signed(0).into(), 0, MockSignedTx::Valid),
			Error::<Test, Instance0>::InvalidBroadcastId
		);
		assert_noop!(
			DogeBroadcast::broadcast_success(Origin::root(), 0, [0u8; 4]),
			Error::<Test, Instance0>::InvalidBroadcastId
		);
		assert_noop!(
			DogeBroadcast::broadcast_failure(
				Origin::root(),
				0,
				BroadcastFailure::TransactionFailed,
				[0u8; 4]
			),
			Error::<Test, Instance0>::InvalidBroadcastId
		);
	})
}

#[test]
fn test_signature_request_expiry() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;
		const BROADCAST_ATTEMPT_ID: BroadcastAttemptId = 1;

		// Initiate broadcast
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		assert!(
			AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID)
				.unwrap()
				.attempt_count == 0
		);

		// Simulate the expiry hook for the next block.
		let current_block = System::block_number();
		DogeBroadcast::on_initialize(current_block + 1);

		// Nothing should have changed
		assert!(
			AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID)
				.unwrap()
				.attempt_count == 0
		);

		// Simulate the expiry hook for the expected expiry block.
		let expected_expiry_block = current_block + SIGNING_EXPIRY_BLOCKS;
		DogeBroadcast::on_initialize(expected_expiry_block);

		// Old attempt has expired.
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		// New attempt is live with same broadcast_id and incremented attempt_count.
		assert!({
			let new_attempt =
				AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID + 1).unwrap();
			new_attempt.attempt_count == 1 && new_attempt.broadcast_id == BROADCAST_ID
		});

		// Subsequent calls to the hook have no further effect.
		DogeBroadcast::on_initialize(expected_expiry_block + 1);

		// Old attempt has expired.
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		// New attempt is live with same broadcast_id and incremented attempt_count.
		assert!({
			let new_attempt =
				AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID + 1).unwrap();
			new_attempt.attempt_count == 1 && new_attempt.broadcast_id == BROADCAST_ID
		});
	})
}

#[test]
fn test_broadcast_request_expiry() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;
		const BROADCAST_ATTEMPT_ID: BroadcastAttemptId = 1;

		// Initiate broadcast and pass the signing stage;
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		MockCfe::respond(Scenario::HappyPath);

		// Simulate the expiry hook for the next block.
		let current_block = System::block_number();
		DogeBroadcast::on_initialize(current_block + 1);

		// Nothing should have changed
		assert!(
			AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID)
				.unwrap()
				.attempt_count == 0
		);

		// Simulate the expiry hook for the expected expiry block.
		let expected_expiry_block = current_block + BROADCAST_EXPIRY_BLOCKS;
		DogeBroadcast::on_initialize(expected_expiry_block);

		// Old attempt has expired.
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		// New attempt is live with same broadcast_id and incremented attempt_count.
		assert!({
			let new_attempt =
				AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID + 1).unwrap();
			new_attempt.attempt_count == 1 && new_attempt.broadcast_id == BROADCAST_ID
		});

		// Subsequent calls to the hook have no further effect.
		DogeBroadcast::on_initialize(expected_expiry_block + 1);

		// Old attempt has expired.
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID).is_none());
		// New attempt is live with same broadcast_id and incremented attempt_count.
		assert!({
			let new_attempt =
				AwaitingSignature::<Test, Instance0>::get(BROADCAST_ATTEMPT_ID + 1).unwrap();
			new_attempt.attempt_count == 1 && new_attempt.broadcast_id == BROADCAST_ID
		});
	})
}
