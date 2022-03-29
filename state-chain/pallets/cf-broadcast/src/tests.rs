use crate::{
	mock::*, AwaitingTransactionSignature, AwaitingTransmission, BroadcastAttemptId, BroadcastId,
	BroadcastRetryQueue, BroadcastStage, Error, Event as BroadcastEvent, Instance1,
	SignatureToBroadcastIdLookup, TransmissionFailure,
};
use cf_chains::{
	mocks::{MockEthereum, MockThresholdSignature, MockUnsignedTransaction, Validity},
	ChainAbi,
};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use frame_system::RawOrigin;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Scenario {
	HappyPath,
	BadSigner,
	TransmissionFailure(TransmissionFailure),
	Timeout,
	SignatureAccepted,
}

thread_local! {
	pub static COMPLETED_BROADCASTS: std::cell::RefCell<Vec<BroadcastId>> = Default::default();
	pub static FAILED_BROADCASTS: std::cell::RefCell<Vec<BroadcastId>> = Default::default();
	pub static EXPIRED_ATTEMPTS: std::cell::RefCell<Vec<(BroadcastAttemptId, BroadcastStage)>> = Default::default();
	pub static ABORTED_BROADCAST: std::cell::RefCell<BroadcastId> = Default::default();
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
			Event::MockBroadcast(broadcast_event) => match broadcast_event {
				BroadcastEvent::TransactionSigningRequest(attempt_id, nominee, unsigned_tx) => {
					if let Scenario::Timeout = scenario {
						// Ignore the request.
						return
					}
					Self::handle_transaction_signature_request(
						attempt_id,
						nominee,
						unsigned_tx,
						scenario,
					);
				},
				BroadcastEvent::TransmissionRequest(attempt_id, _signed_tx) => {
					if let Scenario::Timeout = scenario {
						// Ignore the request.
						return
					}
					Self::handle_broadcast_request(attempt_id.broadcast_id, scenario);
				},
				BroadcastEvent::BroadcastComplete(broadcast_id) => {
					COMPLETED_BROADCASTS.with(|cell| cell.borrow_mut().push(broadcast_id));
				},
				BroadcastEvent::BroadcastRetryScheduled(_) => {
					// Informational only. No action required by the CFE.
				},
				BroadcastEvent::BroadcastFailed(broadcast_attempt_id, _) => {
					FAILED_BROADCASTS
						.with(|cell| cell.borrow_mut().push(broadcast_attempt_id.broadcast_id));
				},
				BroadcastEvent::BroadcastAttemptExpired(broadcast_attempt_id, stage) =>
					EXPIRED_ATTEMPTS
						.with(|cell| cell.borrow_mut().push((broadcast_attempt_id, stage))),
				BroadcastEvent::BroadcastAborted(_) => {
					// Informational only. No action required by the CFE.
				},
				BroadcastEvent::__Ignore(_, _) => unreachable!(),
			},
			_ => panic!("Unexpected event"),
		};
	}

	// Accepts an unsigned tx, making sure the nominee has been assigned.
	fn handle_transaction_signature_request(
		attempt_id: BroadcastAttemptId,
		nominee: u64,
		unsigned_tx: MockUnsignedTransaction,
		scenario: Scenario,
	) {
		assert_eq!(nominee, RANDOM_NOMINEE);
		// Only the nominee can return the signed tx.
		assert_noop!(
			MockBroadcast::transaction_ready_for_transmission(
				RawOrigin::Signed(nominee + 1).into(),
				attempt_id,
				unsigned_tx.clone().signed(Validity::Valid),
				Validity::Valid
			),
			Error::<Test, Instance1>::InvalidSigner
		);
		// Only the nominee can return the signed tx.
		assert_ok!(MockBroadcast::transaction_ready_for_transmission(
			RawOrigin::Signed(nominee).into(),
			attempt_id,
			unsigned_tx.signed(Validity::Valid),
			match scenario {
				Scenario::BadSigner => Validity::Invalid,
				_ => Validity::Valid,
			}
		));
	}

	// Simulate different outcomes.
	fn handle_broadcast_request(broadcast_id: BroadcastId, scenario: Scenario) {
		assert_ok!(match scenario {
			Scenario::HappyPath =>
				MockBroadcast::transmission_success(Origin::root(), broadcast_id, [0xcf; 4]),
			Scenario::TransmissionFailure(failure) => {
				MockBroadcast::transmission_failure(
					Origin::root(),
					broadcast_id,
					failure,
					[0xcf; 4],
				)
			},
			Scenario::SignatureAccepted => {
				MockBroadcast::signature_accepted(Origin::root(), MockThresholdSignature::default())
			},
			_ => unimplemented!(),
		});
	}
}

#[test]
fn test_broadcast_happy_path() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_some());

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_some());

		// CFE responds again with confirmation of a successful broadcast.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_none());

		// CFE logs the completed broadcast.
		MockCfe::respond(Scenario::HappyPath);
		assert_eq!(COMPLETED_BROADCASTS.with(|cell| *cell.borrow().first().unwrap()), BROADCAST_ID);

		// Check if the storage was cleaned up successfully
		println!(
			"Here's the broadcast attempt id: {:?}",
			SignatureToBroadcastIdLookup::<Test, Instance1>::get(MockThresholdSignature::default())
		);
		assert!(SignatureToBroadcastIdLookup::<Test, Instance1>::get(
			MockThresholdSignature::default()
		)
		.is_none());
	})
}

#[test]
fn test_broadcast_rejected() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: u32 = 1;

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 1
		);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_some());

		// CFE responds that the transaction was rejected.
		MockCfe::respond(Scenario::TransmissionFailure(TransmissionFailure::TransactionRejected));
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::decode_len().unwrap_or_default(), 1);

		// The `on_initialize` hook is called and triggers a new broadcast attempt.
		MockBroadcast::on_initialize(0);
		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::decode_len().unwrap_or_default(), 0);

		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 2
		);

		// The nominee was not reported.
		assert_eq!(MockOffenceReporter::get_reported(), vec![RANDOM_NOMINEE]);
	})
}

#[test]
fn test_abort_after_max_attempt_reached() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		// A series of failed attempts.  We would expect MAXIMUM_BROADCAST_ATTEMPTS to continue
		// retrying until the request to retry is aborted with an event emitted
		for _ in 0..MAXIMUM_BROADCAST_ATTEMPTS {
			// CFE responds with a signed transaction. This moves us to the broadcast stage.
			MockCfe::respond(Scenario::HappyPath);
			// CFE responds that the transaction was rejected.
			MockCfe::respond(Scenario::TransmissionFailure(
				TransmissionFailure::TransactionRejected,
			));
			// The `on_initialize` hook is called and triggers a new broadcast attempt.
			MockBroadcast::on_initialize(0);
		}

		assert_eq!(
			System::events().pop().expect("an event").event,
			Event::MockBroadcast(crate::Event::BroadcastAborted(1))
		);
	})
}

#[test]
fn test_broadcast_failed() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: u32 = 1;

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 1
		);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_some());

		// CFE responds that the transaction failed.
		MockCfe::respond(Scenario::TransmissionFailure(TransmissionFailure::TransactionFailed));
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_none());

		// We don't retry.
		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::decode_len().unwrap_or_default(), 0);
		// The broadcast has failed.
		MockCfe::respond(Scenario::TransmissionFailure(TransmissionFailure::TransactionFailed));
		assert_eq!(FAILED_BROADCASTS.with(|cell| *cell.borrow().first().unwrap()), BROADCAST_ID);
	})
}

#[test]
fn test_bad_signature() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: u32 = 1;

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 1
		);

		// CFE responds with an invalid transaction.
		MockCfe::respond(Scenario::BadSigner);

		// Broadcast is removed and scheduled for retry.
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::decode_len().unwrap_or_default(), 1);

		// The nominee was reported.
		assert_eq!(MockOffenceReporter::get_reported(), vec![RANDOM_NOMINEE]);
	})
}

#[test]
fn test_invalid_id_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			MockBroadcast::transaction_ready_for_transmission(
				RawOrigin::Signed(0).into(),
				BroadcastAttemptId::default(),
				<<MockEthereum as ChainAbi>::UnsignedTransaction>::default()
					.signed(Validity::Valid),
				Validity::Valid
			),
			Error::<Test, Instance1>::InvalidBroadcastAttemptId
		);
		assert_noop!(
			MockBroadcast::transmission_success(Origin::root(), 0, [0u8; 4]),
			Error::<Test, Instance1>::InvalidBroadcastAttemptId
		);
		assert_noop!(
			MockBroadcast::transmission_failure(
				Origin::root(),
				0,
				TransmissionFailure::TransactionFailed,
				[0u8; 4]
			),
			Error::<Test, Instance1>::InvalidBroadcastAttemptId
		);
	})
}

#[test]
fn test_signature_request_expiry() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;
		let broadcast_attempt_id =
			BroadcastAttemptId { broadcast_id: BROADCAST_ID, attempt_count: 1 };

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 1
		);

		// Simulate the expiry hook for the next block.
		let current_block = System::block_number();
		MockBroadcast::on_initialize(current_block + 1);
		MockCfe::respond(Scenario::Timeout);

		// Nothing should have changed
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 1
		);

		// Simulate the expiry hook for the expected expiry block.
		let expected_expiry_block = current_block + SIGNING_EXPIRY_BLOCKS;
		MockBroadcast::on_initialize(expected_expiry_block);
		MockCfe::respond(Scenario::Timeout);

		let check_end_state = || {
			// old attempt has expired, but the data still exists
			assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_some());

			assert_eq!(
				EXPIRED_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()),
				(broadcast_attempt_id, BroadcastStage::TransactionSigning),
			);

			// New attempt is live with same broadcast_id and incremented attempt_count.
			assert!({
				let new_attempt =
					AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).unwrap();
				new_attempt.broadcast_attempt.broadcast_attempt_id.attempt_count == 2 &&
					new_attempt.broadcast_attempt.broadcast_attempt_id.broadcast_id ==
						BROADCAST_ID
			});
		};

		check_end_state();

		// Subsequent calls to the hook have no further effect.
		MockBroadcast::on_initialize(expected_expiry_block + 1);
		MockCfe::respond(Scenario::Timeout);

		check_end_state();
	})
}

#[test]
fn test_transmission_request_expiry() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;
		let broadcast_attempt_id =
			BroadcastAttemptId { broadcast_id: BROADCAST_ID, attempt_count: 1 };

		// Initiate broadcast and pass the signing stage;
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		MockCfe::respond(Scenario::HappyPath);

		// Simulate the expiry hook for the next block.
		let current_block = System::block_number();
		MockBroadcast::on_initialize(current_block + 1);
		MockCfe::respond(Scenario::Timeout);

		// Nothing should have changed
		assert!(
			AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 1
		);

		// Simulate the expiry hook for the expected expiry block.
		let expected_expiry_block = current_block + TRANSMISSION_EXPIRY_BLOCKS;
		MockBroadcast::on_initialize(expected_expiry_block);
		MockCfe::respond(Scenario::Timeout);

		let check_end_state = || {
			// Old attempt has expired.
			assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_none());
			assert_eq!(
				EXPIRED_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()),
				(broadcast_attempt_id, BroadcastStage::Transmission),
			);
			// New attempt is live with same broadcast_id and incremented attempt_count.
			assert!({
				let new_attempt =
					AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).unwrap();
				new_attempt.broadcast_attempt.broadcast_attempt_id.attempt_count == 2 &&
					new_attempt.broadcast_attempt.broadcast_attempt_id.broadcast_id ==
						BROADCAST_ID
			});
		};

		check_end_state();

		// Subsequent calls to the hook have no further effect.
		MockBroadcast::on_initialize(expected_expiry_block + 1);
		MockCfe::respond(Scenario::Timeout);

		check_end_state();
	})
}

#[test]
fn no_validators_available() {
	new_test_ext().execute_with(|| {
		// Simulate that no validator is currently online
		NOMINATION.with(|cell| *cell.borrow_mut() = None);
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		// Check the retry queue
		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::decode_len().unwrap_or_default(), 1);
	});
}

// In this scenario the transmission of the transaction is not able to get through. We try
// several times but without success. The system remains in this state until CFE witness the
// successful emit of the SignatureAccepted event on the target chain. The broadcast of the
// transaction gest finalized with this.
#[test]
fn missing_transaction_transmission() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: u32 = 1;
		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_some());

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_some());

		// First retry
		MockCfe::respond(Scenario::TransmissionFailure(TransmissionFailure::TransactionRejected));
		MockBroadcast::on_initialize(0_u64);

		MockBroadcast::on_initialize(1_u64);

		// Resign the transaction and move it again to the transmission stage
		MockCfe::respond(Scenario::HappyPath);
		MockBroadcast::on_initialize(2_u64);

		// Expect the transaction back on the transmission state
		// this would be the next attempt_id
		assert_eq!(
			AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count,
			2
		);

		// Finalize the broadcast by witnessing the external SignatureAccepted event from the
		// target chain
		MockCfe::respond(Scenario::SignatureAccepted);
		// Check if the event was emitted
		assert_eq!(
			System::events().pop().expect("an event").event,
			Event::MockBroadcast(crate::Event::BroadcastComplete(BROADCAST_ID))
		);
		MockBroadcast::on_initialize(3_u64);
		MockCfe::respond(Scenario::HappyPath);

		// Proof that the broadcast was successfully finalized
		assert!(AwaitingTransmission::<Test, Instance1>::get(BROADCAST_ID).is_none());
		assert_eq!(COMPLETED_BROADCASTS.with(|cell| *cell.borrow().first().unwrap()), 1);

		// Check if the storage was cleaned up successfully
		assert!(SignatureToBroadcastIdLookup::<Test, Instance1>::get(
			MockThresholdSignature::default()
		)
		.is_none());
	});
}
