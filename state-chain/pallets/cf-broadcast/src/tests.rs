use crate::{
	mock::*, AwaitingTransactionSignature, AwaitingTransmission, BroadcastAttemptId, BroadcastId,
	BroadcastIdToAttemptNumbers, BroadcastRetryQueue, BroadcastStage, Error,
	Event as BroadcastEvent, Expiries, FailedTransactionSigners, Instance1, PalletOffence,
	RefundSignerId, SignatureToBroadcastIdLookup, ThresholdSignatureData, TransactionFeeDeficit,
	TransactionHashWhitelist, WeightInfo,
};
use cf_chains::{
	mocks::{
		MockApiCall, MockEthereum, MockThresholdSignature, MockUnsignedTransaction, Validity,
		ETH_TX_HASH,
	},
	ChainAbi,
};
use cf_traits::{
	mocks::{epoch_info::MockEpochInfo, threshold_signer::MockThresholdSigner},
	AsyncResult, ThresholdSigner,
};
use frame_support::{assert_noop, assert_ok, dispatch::Weight, traits::Hooks};
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
	pub static EXPIRED_ATTEMPTS: std::cell::RefCell<Vec<(BroadcastAttemptId, BroadcastStage)>> = Default::default();
	pub static ABORTED_BROADCAST: std::cell::RefCell<BroadcastId> = Default::default();
}

// When calling on_idle, we should broadcast everything with this excess weight.
const LARGE_EXCESS_WEIGHT: Weight = 20_000_000_000;

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
							assert_eq!(
								nominee,
								MockNominator::get_nominee().unwrap(),
								"CFE using wrong nomination"
							);
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
						},
						_ => {
							assert_eq!(nominee, MockNominator::get_nominee().unwrap());
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
						Scenario::Timeout => {
							// Ignore the request.
						},

						// NB: This is ok for the sake of testing, but conceptually it's slightly
						// different to the real version, as we submit signature_accepted after
						// *witnessing* the transaction on ETH NOT when transmit the transaction.
						Scenario::HappyPath => {
							assert_ok!(MockBroadcast::signature_accepted(
								Origin::root(),
								MockThresholdSignature::default(),
								200,
								[0xcf; 4],
							));
						},
						_ => unimplemented!(),
					};
				},
				BroadcastEvent::BroadcastSuccess(broadcast_id) => {
					COMPLETED_BROADCASTS.with(|cell| cell.borrow_mut().push(broadcast_id));
				},
				BroadcastEvent::BroadcastRetryScheduled(_) => {
					// Informational only. No action required by the CFE.
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
				BroadcastEvent::ThresholdSignatureInvalid(_) => {},
			},
			_ => panic!("Unexpected event"),
		};
	}
}

fn assert_broadcast_storage_cleaned_up(broadcast_id: BroadcastId) {
	assert!(
		SignatureToBroadcastIdLookup::<Test, Instance1>::get(MockThresholdSignature::default())
			.is_none()
	);
	assert!(FailedTransactionSigners::<Test, Instance1>::get(broadcast_id).is_none());
	assert!(BroadcastIdToAttemptNumbers::<Test, Instance1>::get(broadcast_id).is_none());
	assert!(ThresholdSignatureData::<Test, Instance1>::get(broadcast_id).is_none());
}

#[test]
fn test_broadcast_happy_path() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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

		assert_broadcast_storage_cleaned_up(broadcast_attempt_id.broadcast_id);
	})
}

#[test]
fn test_abort_after_max_attempt_reached() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast

		let starting_nomination = MockNominator::get_nominee().unwrap();

		let mut broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		// A series of failed attempts.  We would expect MAXIMUM_BROADCAST_ATTEMPTS to continue
		// retrying until the request to retry is aborted with an event emitted
		for _ in 0..=MAXIMUM_BROADCAST_ATTEMPTS {
			// Nominated signer responds that they can't sign the transaction.
			MockCfe::respond(Scenario::SigningFailure);

			// make the nomination unique, so we can test that all the authorities
			// so we can test all the failed authorities reported
			MockNominator::increment_nominee();

			// retry should kick off at end of block if sufficient block space is free.
			MockBroadcast::on_idle(0, LARGE_EXCESS_WEIGHT);
			MockBroadcast::on_initialize(0);

			broadcast_attempt_id = broadcast_attempt_id.next_attempt();
		}

		assert_eq!(
			System::events().pop().expect("an event").event,
			Event::MockBroadcast(crate::Event::BroadcastAborted(1))
		);

		MockOffenceReporter::assert_reported(
			PalletOffence::FailedToSignTransaction,
			(starting_nomination..=(starting_nomination + (MAXIMUM_BROADCAST_ATTEMPTS as u64)))
				.collect::<Vec<_>>(),
		);

		assert_broadcast_storage_cleaned_up(broadcast_attempt_id.broadcast_id);
	})
}

#[test]
fn on_idle_caps_broadcasts_when_not_enough_weight() {
	new_test_ext().execute_with(|| {
		// kick off two broadcasts
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		let broadcast_attempt_id_2 = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);

		// respond failure to both
		MockCfe::respond(Scenario::SigningFailure);

		let start_next_broadcast_weight: Weight =
			<() as WeightInfo>::start_next_broadcast_attempt();

		// only a single retry will fit in the block since we use the exact weight of the call
		MockBroadcast::on_idle(0, start_next_broadcast_weight);
		MockBroadcast::on_initialize(0);

		// only the first one should have retried, incremented attempt count
		assert!(AwaitingTransactionSignature::<Test, Instance1>::get(
			broadcast_attempt_id.next_attempt()
		)
		.is_some());
		// the other should be still in the retry queue
		let retry_queue = BroadcastRetryQueue::<Test, Instance1>::get();
		assert_eq!(retry_queue.len(), 1);
		assert_eq!(retry_queue.first().unwrap().broadcast_attempt_id, broadcast_attempt_id_2);
	})
}

#[test]
fn test_transaction_signing_failed() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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

		// retry should kick off at end of block
		MockBroadcast::on_idle(0, LARGE_EXCESS_WEIGHT);
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
		// Initiate broadcast
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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
			vec![MockNominator::get_nominee().unwrap()],
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
				0,
				[0u8; 4],
			),
			Error::<Test, Instance1>::InvalidPayload
		);
	})
}

#[test]
fn cfe_responds_signature_success_already_expired_transaction_sig_broadcast_attempt_id_is_noop() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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
		assert!(TransactionHashWhitelist::<Test, Instance1>::get(ETH_TX_HASH).is_none());
		assert!(RefundSignerId::<Test, Instance1>::get(tx_sig_request.nominee).is_none());

		// TODO: should we move this testing below into a separate test

		// Tx hash of the unsigned ethereum transaction
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
			TransactionHashWhitelist::<Test, Instance1>::get(ETH_TX_HASH).unwrap(),
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
		assert_ok!(MockBroadcast::signature_accepted(
			Origin::root(),
			MockThresholdSignature::default(),
			FEE_PAID,
			ETH_TX_HASH,
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
fn signature_accepted_of_whitelisted_tx_hash_results_in_refund_for_whitelister() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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
		// The mapping from TransactionHash to account id should be updated
		assert_eq!(
			TransactionHashWhitelist::<Test, Instance1>::get(ETH_TX_HASH).unwrap(),
			tx_sig_request.nominee
		);

		// The mapping from account id to signer id should be updated
		assert_eq!(
			RefundSignerId::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			Validity::Valid
		);

		const FEE_PAID: u128 = 200;
		// now we respond with signature accepted from the invalid signer since they weren't
		// whitelisted
		assert_ok!(MockBroadcast::signature_accepted(
			Origin::root(),
			MockThresholdSignature::default(),
			FEE_PAID,
			ETH_TX_HASH,
		));

		assert_eq!(
			TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			FEE_PAID
		);
		assert!(TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee + 1).is_none());
	});
}

#[test]
fn signature_accepted_of_non_whitelisted_tx_hash_results_in_no_refund() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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
		// The mapping from TransactionHash to account id should be updated
		assert_eq!(
			TransactionHashWhitelist::<Test, Instance1>::get(ETH_TX_HASH).unwrap(),
			tx_sig_request.nominee
		);

		// The refund mapping from account id to signer id should be updated
		assert_eq!(
			RefundSignerId::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			Validity::Valid
		);

		// simulates a node submitting a diff tx than the one they committed to
		// when they submitted `transaction_ready_for_transmission`
		let mut bad_eth_tx_hash = ETH_TX_HASH;
		bad_eth_tx_hash[0] = ETH_TX_HASH[0] + 1;

		const FEE_PAID: u128 = 200;
		assert_ok!(MockBroadcast::signature_accepted(
			Origin::root(),
			MockThresholdSignature::default(),
			FEE_PAID,
			bad_eth_tx_hash,
		));

		assert_eq!(
			TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee).unwrap(),
			0
		);
		assert!(TransactionFeeDeficit::<Test, Instance1>::get(tx_sig_request.nominee + 1).is_none());
	});
}

#[test]
fn test_signature_request_expiry() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		let first_broadcast_id = broadcast_attempt_id.broadcast_id;
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
						first_broadcast_id
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
		// Initiate broadcast and pass the signing stage;
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		let first_broadcast_id = broadcast_attempt_id.broadcast_id;
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
						first_broadcast_id
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
fn re_request_threshold_signature() {
	new_test_ext().execute_with(|| {
		// Initiate broadcast
		let broadcast_attempt_id = MockBroadcast::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		// Expect the threshold signature pipeline to be empty
		assert_eq!(
			MockThresholdSigner::<MockEthereum, Call>::signature_result(0),
			AsyncResult::Void
		);
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).is_some()
		);
		assert_eq!(
			BroadcastIdToAttemptNumbers::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id)
				.unwrap(),
			vec![0]
		);
		// Simualte a key rotation to invalidate the signature
		MockKeyProvider::set_valid(false);
		MockBroadcast::on_initialize(SIGNING_EXPIRY_BLOCKS + 1);
		// Expect the broadcast to be deleted
		assert!(
			AwaitingTransactionSignature::<Test, Instance1>::get(broadcast_attempt_id).is_none()
		);
		// Verify storage has been deleted
		assert!(SignatureToBroadcastIdLookup::<Test, Instance1>::get(
			MockThresholdSignature::default()
		)
		.is_none());
		assert!(BroadcastIdToAttemptNumbers::<Test, Instance1>::get(1).is_none());
		assert!(ThresholdSignatureData::<Test, Instance1>::get(1).is_none());
		// Verify that we have a new signature request in the pipeline
		assert_eq!(
			MockThresholdSigner::<MockEthereum, Call>::signature_result(0),
			AsyncResult::Pending
		);
	});
}
