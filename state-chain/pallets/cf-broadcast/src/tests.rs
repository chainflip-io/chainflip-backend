use crate::{
	mock::*, AwaitingTransactionSignature, AwaitingTransmission, BroadcastAttemptId, BroadcastId,
	BroadcastIdToAttemptNumbers, BroadcastRetryQueue, BroadcastStage, Error,
	Event as BroadcastEvent, Expiries, Instance1, PalletOffence, RefundSignerId,
	SignatureToBroadcastIdLookup, SignerIdToAccountId, TransactionFeeDeficit, TransmissionFailure,
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
	SigningFailure,
	Timeout,
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
				BroadcastEvent::TransactionSigningRequest(
					broadcast_attempt_id,
					nominee,
					unsigned_tx,
				) => {
					if let Scenario::Timeout = scenario {}
					match scenario {
						Scenario::SigningFailure => {
							// only nominee can return the signed tx
							assert_noop!(
								MockBroadcast::transaction_signing_failure(
									RawOrigin::Signed(nominee + 1).into(),
									broadcast_attempt_id
								),
								Error::<Test, Instance1>::InvalidSigner
							);
							assert_ok!(MockBroadcast::transaction_signing_failure(
								RawOrigin::Signed(nominee).into(),
								broadcast_attempt_id,
							));
						},
						Scenario::Timeout => {
							// Ignore the request.
							return
						},
						_ => {
							assert_eq!(nominee, RANDOM_NOMINEE);
							// Only the nominee can return the signed tx.
							assert_noop!(
								MockBroadcast::transaction_ready_for_transmission(
									RawOrigin::Signed(nominee + 1).into(),
									broadcast_attempt_id,
									unsigned_tx.clone().signed(Validity::Valid),
									Validity::Valid
								),
								Error::<Test, Instance1>::InvalidSigner
							);
							// Only the nominee can return the signed tx.
							assert_ok!(MockBroadcast::transaction_ready_for_transmission(
								RawOrigin::Signed(nominee).into(),
								broadcast_attempt_id,
								unsigned_tx.signed(Validity::Valid),
								match scenario {
									Scenario::BadSigner => Validity::Invalid,
									_ => Validity::Valid,
								}
							));
						},
					}
				},
				BroadcastEvent::TransmissionRequest(_, _signed_tx) => {
					match scenario {
						Scenario::Timeout => return,

						// NB: This is ok for the sake of testing, but conceptually it's slightly
						// different to the real version, as we submit signature_accepted after
						// *witnessing* the transaction on ETH NOT when transmit the transaction.
						Scenario::HappyPath => {
							assert_ok!(MockBroadcast::signature_accepted(
								Origin::root(),
								MockThresholdSignature::default(),
								Validity::Valid,
                                0,
								10,
								[0xcf; 4],
								200,
							));
						},
						_ => unimplemented!(),
					};
				},
				BroadcastEvent::BroadcastComplete(broadcast_attempt_id) => {
					COMPLETED_BROADCASTS
						.with(|cell| cell.borrow_mut().push(broadcast_attempt_id.broadcast_id));
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
				BroadcastEvent::RefundSignerIdUpdated(_, _) => {
					// Information only. No action required by the CFE.
				},
			},
			_ => panic!("Unexpected event"),
		};
	}
}

#[test]
fn test_broadcast_happy_path() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id: 1, attempt_count: 0 };
		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).is_some()
		);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).is_none()
		);
		assert!(AwaitingTransmission::<Test, Instance1>::get(broadcast_attempt_id).is_some());

		// CFE responds again with confirmation of a successful broadcast.
		MockCfe::respond(Scenario::HappyPath);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).is_none()
		);
		assert!(AwaitingTransmission::<Test, Instance1>::get(broadcast_attempt_id).is_none());

		// CFE logs the completed broadcast.
		MockCfe::respond(Scenario::HappyPath);
		assert_eq!(
			COMPLETED_BROADCASTS.with(|cell| *cell.borrow().first().unwrap()),
			broadcast_attempt_id.broadcast_id
		);

		// Check if the storage was cleaned up successfully
		assert!(SignatureToBroadcastIdLookup::<Test, Instance1>::get(
			MockThresholdSignature::default()
		)
		.is_none());
	})
}

#[test]
fn test_abort_after_max_attempt_reached() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		// A series of failed attempts.  We would expect MAXIMUM_BROADCAST_ATTEMPTS to continue
		// retrying until the request to retry is aborted with an event emitted
		for _ in 0..MAXIMUM_BROADCAST_ATTEMPTS + 1 {
			// Nominated signer responds that they can't sign the transaction.
			MockCfe::respond(Scenario::SigningFailure);
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
fn test_transaction_signing_failed() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id: 1, attempt_count: 0 };

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::SigningFailure);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).is_none()
		);
		assert!(AwaitingTransmission::<Test, Instance1>::get(broadcast_attempt_id).is_none());
		assert_eq!(
			BroadcastRetryQueue::<Test, Instance1>::get()
				.into_iter()
				.next()
				.unwrap()
				.broadcast_attempt_id,
			broadcast_attempt_id
		);

		// initialising a new block should kick off the retry
		MockBroadcast::on_initialize(0);
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(
			broadcast_attempt_id.next_attempt()
		)
		.is_some());
	})
}

#[test]
fn test_bad_signature() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id: 1, attempt_count: 0 };

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);

		// CFE responds with an invalid transaction.
		MockCfe::respond(Scenario::BadSigner);

		// Broadcast is removed and scheduled for retry.
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).is_none()
		);
		assert!(AwaitingTransmission::<Test, Instance1>::get(broadcast_attempt_id).is_none());
		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::decode_len().unwrap_or_default(), 1);

		// The nominee was reported.
		MockOffenceReporter::assert_reported(
			PalletOffence::InvalidTransactionAuthored,
			vec![RANDOM_NOMINEE],
		);
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
			MockBroadcast::transaction_signing_failure(
				RawOrigin::Signed(0).into(),
				BroadcastAttemptId::default(),
			),
			Error::<Test, Instance1>::InvalidBroadcastAttemptId
		);
	})
}

#[test]
fn test_invalid_sigdata_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			MockBroadcast::signature_accepted(
				RawOrigin::Signed(0).into(),
				MockThresholdSignature::default(),
				Validity::Valid,
				0,
				10,
				[0u8; 4],
			),
			Error::<Test, Instance1>::InvalidPayload
		);
	})
}

#[test]
fn cfe_responds_signature_success_already_expired_transaction_sig_broadcast_attempt_id_is_noop() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id: 1, attempt_count: 0 };

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);
		let current_block = System::block_number();
		// we should have no expiries at this point, but in expiry blocks we should
		assert_eq!(Expiries::<Test, Instance1>::get(current_block), vec![]);
		let expiry_block = current_block + SIGNING_EXPIRY_BLOCKS;
		assert_eq!(
			Expiries::<Test, Instance1>::get(expiry_block),
			vec![(BroadcastStage::TransactionSigning, broadcast_attempt_id)]
		);

		// Simulate the expiry hook for the expected expiry block.
		MockBroadcast::on_initialize(expiry_block);

		// We expired the first one
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).is_none()
		);
		let tx_sig_request = AwaitingTransactionSignature::<Test, Instance1>::get(
			broadcast_attempt_id.next_attempt(),
		)
		.unwrap();
		assert_eq!(tx_sig_request.broadcast_attempt.broadcast_attempt_id.attempt_count, 1);

		// This is a little confusing. Because we don't progress in blocks. i.e.
		// System::block_number() does not change
		// so when we retry the expired transaction, the *new* expiry block for the retry is
		// actually the same block since the current block number is unchanged
		// the current block number + SIGNING_EXPIRY_BLOCKS is also unchanged
		// but, the retry has the incremented attempt_count of course
		assert_eq!(
			Expiries::<Test, Instance1>::get(expiry_block),
			vec![(BroadcastStage::TransactionSigning, broadcast_attempt_id.next_attempt())]
		);

		// The first attempt is already expired, but we're going to say it's ready for transmission
		assert_noop!(
			MockBroadcast::transaction_ready_for_transmission(
				RawOrigin::Signed(tx_sig_request.nominee).into(),
				broadcast_attempt_id,
				tx_sig_request.broadcast_attempt.unsigned_tx.clone().signed(Validity::Valid),
				Validity::Valid,
			),
			Error::<Test, Instance1>::InvalidBroadcastAttemptId
		);

		// We should have removed the earlier mapping, as that retry is invalid now
		// and still have the latest retry attempt count
		assert_eq!(
			BroadcastIdToAttemptNumbers::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id)
				.unwrap(),
			vec![1]
		);

		// We still shouldn't have a valid signer in the deficit map yet
		// or any of the related maps
		assert!(TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee).is_none());
		assert!(SignerIdToAccountId::<Test, Instance1>::get(Validity::Valid).is_none());
		assert!(RefundSignerId::<Test, Instance1>::get(tx_sig_request.nominee).is_none());

		// TODO: should we move this testing below into a separate test

		// We now succeed on submitting the second one
		assert_ok!(MockBroadcast::transaction_ready_for_transmission(
			RawOrigin::Signed(tx_sig_request.nominee).into(),
			tx_sig_request.broadcast_attempt.broadcast_attempt_id,
			tx_sig_request.broadcast_attempt.unsigned_tx.signed(Validity::Valid),
			Validity::Valid,
		));

		// only the latest attempt is valid
		assert_eq!(
			BroadcastIdToAttemptNumbers::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id)
				.unwrap(),
			vec![1]
		);

		// We should have the valid signer in the list with no deficit ath this point
		assert_eq!(
			TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			0
		);
		// .. and related identity mappings
		assert_eq!(
			SignerIdToAccountId::<Test, Instance1>::get(Validity::Valid).unwrap(),
			tx_sig_request.nominee
		);
		assert_eq!(
			RefundSignerId::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			Validity::Valid
		);

		// We shouldn't have any other signers with 0 values
		const WRONG_XT_SUBMITTER: u64 = 666;
		assert!(TransactionFeeDeficit::<Test, Instance1>::get(WRONG_XT_SUBMITTER).is_none());

		// we should not have a transmission attempt for the old attempt id that did not succeed
		assert!(AwaitingTransmission::<Test, Instance1>::get(broadcast_attempt_id).is_none());

		// We should have a transmission attempt for the new attempt that did succeed
		assert!(AwaitingTransmission::<Test, Instance1>::get(
			tx_sig_request.broadcast_attempt.broadcast_attempt_id
		)
		.is_some());

		let transmission_expiry_block = current_block + TRANSMISSION_EXPIRY_BLOCKS;
		assert_eq!(
			Expiries::<Test, Instance1>::get(transmission_expiry_block),
			vec![(
				BroadcastStage::Transmission,
				tx_sig_request.broadcast_attempt.broadcast_attempt_id
			)]
		);

		// expire the transmission attempt, success not reached yet
		MockBroadcast::on_initialize(transmission_expiry_block);

		// We should still have the transmission
		assert!(AwaitingTransmission::<Test, Instance1>::get(
			tx_sig_request.broadcast_attempt.broadcast_attempt_id
		)
		.is_some());

		// We now have a valid attempt count (1) for the awaiting transmission
		// and a valid attempt count (2) for the awaiting transaction signature
		assert_eq!(
			BroadcastIdToAttemptNumbers::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id)
				.unwrap(),
			vec![1, 2]
		);

		const FEE_PAID: u128 = 200;
		// We submit that the signature was accepted
		assert_ok!(MockBroadcast::signature_accepted(
			Origin::root(),
			MockThresholdSignature::default(),
			Validity::Valid,
			FEE_PAID,
			10,
			[0xcf; 4],
		));

		// Attempt numbers, signature requests and transmission should be cleaned up
		assert!(BroadcastIdToAttemptNumbers::<Test, Instance1>::get(
			broadcast_attempt_id.broadcast_id
		)
		.is_none());

		// We should now have a deficit for the valid signer
		assert_eq!(
			TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			FEE_PAID
		);
		assert!(AwaitingTransmission::<Test, Instance1>::get(
			tx_sig_request.broadcast_attempt.broadcast_attempt_id
		)
		.is_none());
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(
			tx_sig_request.broadcast_attempt.broadcast_attempt_id.next_attempt()
		)
		.is_none())
	});
}

#[test]
fn signature_accepted_signed_by_non_whitelisted_signer_id_does_not_increase_deficit() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id: 1, attempt_count: 0 };
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		let tx_sig_request =
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).unwrap();

		let signed_tx = tx_sig_request.broadcast_attempt.unsigned_tx.signed(Validity::Valid);
		let _ = MockBroadcast::transaction_ready_for_transmission(
			RawOrigin::Signed(tx_sig_request.nominee).into(),
			broadcast_attempt_id,
			signed_tx,
			Validity::Valid,
		);

		// We have whitelisted their address, 0 deficit
		assert_eq!(
			TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			0
		);
		// The mapping from SignerId to account id should be updated
		assert_eq!(
			SignerIdToAccountId::<Test, Instance1>::get(Validity::Valid).unwrap(),
			tx_sig_request.nominee
		);

		// The mapping from account id to signer id should be updated
		assert_eq!(
			RefundSignerId::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			Validity::Valid
		);

		// now we respond with signature accepted from the invalid signer since they weren't
		// whitelisted
		assert_ok!(MockBroadcast::signature_accepted(
			Origin::root(),
			MockThresholdSignature::default(),
			Validity::Invalid,
			200,
			10,
			[0xcf; 4],
		));

		assert_eq!(
			TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			0
		);
	});
}

#[test]
fn test_signature_request_expiry() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;
		let broadcast_attempt_id =
			BroadcastAttemptId { broadcast_id: BROADCAST_ID, attempt_count: 0 };

		// Initiate broadcast
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);

		// Simulate the expiry hook for the next block.
		let current_block = System::block_number();
		MockBroadcast::on_initialize(current_block + 1);
		MockCfe::respond(Scenario::Timeout);

		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);

		// Simulate the expiry hook for the expected expiry block.
		let expected_expiry_block = current_block + SIGNING_EXPIRY_BLOCKS;
		MockBroadcast::on_initialize(expected_expiry_block);
		MockCfe::respond(Scenario::Timeout);

		let check_end_state = || {
			// old attempt has expired, but the data still exists
			assert!(AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id)
				.is_none());

			assert_eq!(
				EXPIRED_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()),
				(broadcast_attempt_id, BroadcastStage::TransactionSigning),
			);

			// New attempt is live with same broadcast_id and incremented attempt_count.
			assert!({
				let new_attempt = AwaitingTransactionSignature::<Test, Instance1>::get(
					broadcast_attempt_id.next_attempt(),
				)
				.unwrap();
				new_attempt.broadcast_attempt.broadcast_attempt_id.attempt_count == 1 &&
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
		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id: 1, attempt_count: 0 };

		// Initiate broadcast and pass the signing stage;
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		MockCfe::respond(Scenario::HappyPath);

		// Simulate the expiry hook for the next block.
		let current_block = System::block_number();
		MockBroadcast::on_initialize(current_block + 1);
		MockCfe::respond(Scenario::Timeout);

		// Nothing should have changed
		assert!(
			AwaitingTransmission::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);

		// Simulate the expiry hook for the expected expiry block.
		let expected_expiry_block = current_block + TRANSMISSION_EXPIRY_BLOCKS;
		MockBroadcast::on_initialize(expected_expiry_block);
		MockCfe::respond(Scenario::Timeout);

		let check_end_state = || {
			// We still allow nodes to submit transmission successes for retried broadcasts
			assert!(AwaitingTransmission::<Test, Instance1>::get(broadcast_attempt_id).is_some());
			assert_eq!(
				EXPIRED_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()),
				(broadcast_attempt_id, BroadcastStage::Transmission),
			);
			// New attempt is live with same broadcast_id and incremented attempt_count.
			assert!({
				let new_attempt = AwaitingTransactionSignature::<Test, Instance1>::get(
					broadcast_attempt_id.next_attempt(),
				)
				.unwrap();
				new_attempt.broadcast_attempt.broadcast_attempt_id.attempt_count == 1 &&
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
fn no_authorities_available() {
	new_test_ext().execute_with(|| {
		// Simulate that no authority is currently online
		NOMINATION.with(|cell| *cell.borrow_mut() = None);
		MockBroadcast::start_broadcast(&MockThresholdSignature::default(), MockUnsignedTransaction);
		// Check the retry queue
		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::decode_len().unwrap_or_default(), 1);
	});
}
