#![cfg(test)]

use crate::{
	mock::*, AwaitingBroadcast, BroadcastAttemptCount, BroadcastAttemptId, BroadcastId,
	BroadcastRetryQueue, Error, Event as BroadcastEvent, FailedBroadcasters, Instance1,
	PalletOffence, RequestCallbacks, ThresholdSignatureData, TransactionFeeDeficit,
	TransactionOutIdToBroadcastId, WeightInfo,
};
use cf_chains::{
	eth::SchnorrVerificationComponents,
	mocks::{
		MockApiCall, MockEthereum, MockThresholdSignature, MockTransaction, MockTransactionBuilder,
		ETH_TX_FEE,
	},
	ChainCrypto, FeeRefundCalculator,
};
use cf_traits::{
	mocks::{signer_nomination::MockNominator, threshold_signer::MockThresholdSigner},
	AsyncResult, Chainflip, EpochInfo, ThresholdSigner,
};
use frame_support::{assert_noop, assert_ok, dispatch::Weight, traits::Hooks};
use frame_system::RawOrigin;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Scenario {
	HappyPath,
	SigningFailure,
	Timeout,
}

thread_local! {
	pub static TIMEDOUT_ATTEMPTS: std::cell::RefCell<Vec<BroadcastAttemptId>> = Default::default();
	pub static ABORTED_BROADCAST: std::cell::RefCell<BroadcastId> = Default::default();
}

// When calling on_idle, we should broadcast everything with this excess weight.
const LARGE_EXCESS_WEIGHT: Weight = Weight::from_ref_time(20_000_000_000);

const MOCK_TRANSACTION_OUT_ID: [u8; 4] = [0xbc; 4];

struct MockCfe;

impl MockCfe {
	fn respond(scenario: Scenario) {
		let events = System::events();
		System::reset_events();
		for event_record in events {
			Self::process_event(event_record.event, scenario.clone());
		}
	}

	fn process_event(event: RuntimeEvent, scenario: Scenario) {
		match event {
			RuntimeEvent::Broadcaster(broadcast_event) => match broadcast_event {
				BroadcastEvent::TransactionBroadcastRequest {
					broadcast_attempt_id,
					nominee,
					transaction_payload: _,
					transaction_out_id: _,
				} => {
					match scenario {
						Scenario::SigningFailure => {
							// only nominee can return the signed tx
							assert_eq!(
								nominee,
								MockNominator::get_last_nominee().unwrap(),
								"CFE using wrong nomination"
							);
							assert_noop!(
								Broadcaster::transaction_signing_failure(
									RawOrigin::Signed((nominee + 1) % 3).into(),
									broadcast_attempt_id
								),
								Error::<Test, Instance1>::InvalidSigner
							);
							assert_ok!(Broadcaster::transaction_signing_failure(
								RawOrigin::Signed(nominee).into(),
								broadcast_attempt_id,
							));
						},
						Scenario::Timeout => {
							// Ignore the request.
						},
						_ => {
							// only nominee can return the signed tx
							assert_eq!(
								nominee,
								MockNominator::get_last_nominee().unwrap(),
								"CFE using wrong nomination"
							);
						},
					}
				},
				BroadcastEvent::BroadcastSuccess { .. } => {
					// Informational only. no action required by the CFE.
				},
				BroadcastEvent::BroadcastRetryScheduled { .. } => {
					// Informational only. No action required by the CFE.
				},
				BroadcastEvent::BroadcastAttemptTimeout { broadcast_attempt_id } =>
					TIMEDOUT_ATTEMPTS.with(|cell| cell.borrow_mut().push(broadcast_attempt_id)),
				BroadcastEvent::BroadcastAborted { .. } => {
					// Informational only. No action required by the CFE.
				},
				BroadcastEvent::__Ignore(_, _) => unreachable!(),
				_ => {},
			},
			_ => panic!("Unexpected event"),
		};
	}
}

fn assert_broadcast_storage_cleaned_up(broadcast_id: BroadcastId) {
	assert!(
		TransactionOutIdToBroadcastId::<Test, Instance1>::get(MOCK_TRANSACTION_OUT_ID).is_none()
	);
	assert!(FailedBroadcasters::<Test, Instance1>::get(broadcast_id).is_none());
	assert_eq!(BroadcastAttemptCount::<Test, Instance1>::get(broadcast_id), 0);
	assert!(ThresholdSignatureData::<Test, Instance1>::get(broadcast_id).is_none());
}

fn start_mock_broadcast_tx_out_id(
	tx_out_id: <MockEthereum as ChainCrypto>::TransactionOutId,
) -> BroadcastAttemptId {
	Broadcaster::start_broadcast(
		&MockThresholdSignature::default(),
		MockTransaction,
		MockApiCall { tx_out_id, ..Default::default() },
		MockApiCall::<MockEthereum>::default().payload,
		1,
	)
}

fn start_mock_broadcast() -> BroadcastAttemptId {
	start_mock_broadcast_tx_out_id(Default::default())
}

// The happy path :)
#[test]
fn transaction_succeeded_results_in_refund_for_signer() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = start_mock_broadcast_tx_out_id(MOCK_TRANSACTION_OUT_ID);
		let tx_sig_request =
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).unwrap();

		let nominee = MockNominator::get_last_nominee().unwrap();

		assert_eq!(TransactionFeeDeficit::<Test, Instance1>::get(nominee), 0);

		assert_ok!(Broadcaster::transaction_succeeded(
			RuntimeOrigin::root(),
			MOCK_TRANSACTION_OUT_ID,
			nominee,
			ETH_TX_FEE,
		));

		let expected_refund = tx_sig_request
			.broadcast_attempt
			.transaction_payload
			.return_fee_refund(ETH_TX_FEE);

		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_none());

		assert_eq!(TransactionFeeDeficit::<Test, Instance1>::get(nominee), expected_refund);

		assert_broadcast_storage_cleaned_up(broadcast_attempt_id.broadcast_id);
	});
}

#[test]
fn test_abort_after_number_of_attempts_is_equal_to_the_number_of_authorities() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = start_mock_broadcast();

		for i in 0..MockEpochInfo::current_authority_count() {
			// Nominated signer responds that they can't sign the transaction.
			// retry should kick off at end of block if sufficient block space is free.
			assert_eq!(
				BroadcastAttemptCount::<Test, _>::get(broadcast_attempt_id.broadcast_id),
				broadcast_attempt_id.attempt_count + i,
				"Failed for {broadcast_attempt_id:?} at iteration {i}"
			);
			MockCfe::respond(Scenario::SigningFailure);
			Broadcaster::on_idle(0, LARGE_EXCESS_WEIGHT);
		}

		assert_eq!(
			System::events().pop().expect("an event").event,
			RuntimeEvent::Broadcaster(crate::Event::BroadcastAborted {
				broadcast_id: broadcast_attempt_id.broadcast_id
			})
		);
	})
}

#[test]
fn on_idle_caps_broadcasts_when_not_enough_weight() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = start_mock_broadcast();

		MockCfe::respond(Scenario::SigningFailure);

		let broadcast_attempt_id_2 = start_mock_broadcast();

		MockCfe::respond(Scenario::SigningFailure);

		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::get().len(), 2);

		let start_next_broadcast_weight: Weight =
			<() as WeightInfo>::start_next_broadcast_attempt();

		// only a single retry will fit in the block since we use the exact weight of the call
		Broadcaster::on_idle(0, start_next_broadcast_weight);
		Broadcaster::on_initialize(0);

		// only the first one should have retried, incremented attempt count
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id.next_attempt())
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
		let broadcast_attempt_id = start_mock_broadcast();
		assert!(
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::SigningFailure);
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_none());
		assert_eq!(
			BroadcastRetryQueue::<Test, Instance1>::get()
				.into_iter()
				.next()
				.unwrap()
				.broadcast_attempt_id,
			broadcast_attempt_id
		);

		// retry should kick off at end of block
		Broadcaster::on_idle(0, LARGE_EXCESS_WEIGHT);
		Broadcaster::on_initialize(0);

		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id.next_attempt())
			.is_some());
	})
}

#[test]
fn test_invalid_id_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Broadcaster::transaction_signing_failure(
				RawOrigin::Signed(
					*<Test as Chainflip>::EpochInfo::current_authorities().first().unwrap()
				)
				.into(),
				BroadcastAttemptId::default(),
			),
			Error::<Test, Instance1>::InvalidBroadcastAttemptId
		);
	})
}

#[test]
fn test_sigdata_with_no_match_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Broadcaster::transaction_succeeded(
				RawOrigin::Signed(
					*<Test as Chainflip>::EpochInfo::current_authorities().first().unwrap()
				)
				.into(),
				MOCK_TRANSACTION_OUT_ID,
				Default::default(),
				ETH_TX_FEE,
			),
			Error::<Test, Instance1>::InvalidPayload
		);
	})
}

// the nodes who failed to broadcast should be report if we succeed, since success
// indicates the failed nodes could have succeeded themselves.
#[test]
fn transaction_succeeded_after_timeout_reports_failed_nodes() {
	new_test_ext().execute_with(|| {
		start_mock_broadcast_tx_out_id(MOCK_TRANSACTION_OUT_ID);

		let mut failed_authorities = vec![];
		// The last node succeeds
		for _ in 0..MockEpochInfo::current_authority_count() - 1 {
			// Nominated signer responds that they can't sign the transaction.
			MockCfe::respond(Scenario::SigningFailure);
			failed_authorities.push(MockNominator::get_last_nominee().unwrap());

			// retry should kick off at end of block if sufficient block space is free.
			Broadcaster::on_idle(0, LARGE_EXCESS_WEIGHT);
			Broadcaster::on_initialize(0);
		}

		assert_ok!(Broadcaster::transaction_succeeded(
			RuntimeOrigin::root(),
			MOCK_TRANSACTION_OUT_ID,
			Default::default(),
			ETH_TX_FEE,
		));

		MockOffenceReporter::assert_reported(
			PalletOffence::FailedToBroadcastTransaction,
			failed_authorities,
		);
	});
}

#[test]
fn test_signature_request_expiry() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = start_mock_broadcast();
		let first_broadcast_id = broadcast_attempt_id.broadcast_id;
		assert!(
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);

		// Simulate the expiry hook for the next block.
		let current_block = System::block_number();
		Broadcaster::on_initialize(current_block + 1);
		MockCfe::respond(Scenario::Timeout);

		assert!(
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);

		// Simulate the expiry hook for the expected expiry block.
		let expected_expiry_block = current_block + BROADCAST_EXPIRY_BLOCKS;
		Broadcaster::on_initialize(expected_expiry_block);
		MockCfe::respond(Scenario::Timeout);

		let check_end_state = || {
			// old attempt has expired, but the data still exists
			assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_none());

			assert_eq!(
				TIMEDOUT_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()),
				broadcast_attempt_id,
			);

			// New attempt is live with same broadcast_id and incremented attempt_count.
			assert!({
				let new_attempt =
					AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id.next_attempt())
						.unwrap();
				new_attempt.broadcast_attempt.broadcast_attempt_id.attempt_count == 1 &&
					new_attempt.broadcast_attempt.broadcast_attempt_id.broadcast_id ==
						first_broadcast_id
			});
		};

		check_end_state();

		// Subsequent calls to the hook have no further effect.
		Broadcaster::on_initialize(expected_expiry_block + 1);
		MockCfe::respond(Scenario::Timeout);

		check_end_state();
	})
}

#[test]
fn test_transmission_request_expiry() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = start_mock_broadcast();
		let first_broadcast_id = broadcast_attempt_id.broadcast_id;
		MockCfe::respond(Scenario::HappyPath);

		// Simulate the expiry hook for the next block.
		let current_block = System::block_number();
		Broadcaster::on_initialize(current_block + 1);
		MockCfe::respond(Scenario::Timeout);

		// Simulate the expiry hook for the expected expiry block.
		let expected_expiry_block = current_block + BROADCAST_EXPIRY_BLOCKS;
		Broadcaster::on_initialize(expected_expiry_block);
		MockCfe::respond(Scenario::Timeout);

		let check_end_state = || {
			assert_eq!(
				TIMEDOUT_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()),
				broadcast_attempt_id,
			);
			// New attempt is live with same broadcast_id and incremented attempt_count.
			assert!({
				let new_attempt =
					AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id.next_attempt())
						.unwrap();
				new_attempt.broadcast_attempt.broadcast_attempt_id.attempt_count == 1 &&
					new_attempt.broadcast_attempt.broadcast_attempt_id.broadcast_id ==
						first_broadcast_id
			});
		};

		check_end_state();

		// Subsequent calls to the hook have no further effect.
		Broadcaster::on_initialize(expected_expiry_block + 1);
		MockCfe::respond(Scenario::Timeout);

		check_end_state();
	})
}

fn threshold_signature_rerequested(broadcast_attempt_id: BroadcastAttemptId) {
	// Expect the original broadcast to be deleted
	assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_none());
	// Verify storage has been deleted
	assert!(
		TransactionOutIdToBroadcastId::<Test, Instance1>::get(MOCK_TRANSACTION_OUT_ID).is_none()
	);
	assert_eq!(BroadcastAttemptCount::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id), 0);
	assert!(
		ThresholdSignatureData::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id).is_none()
	);
	// Verify that we have a new signature request in the pipeline
	assert_eq!(
		MockThresholdSigner::<MockEthereum, RuntimeCall>::signature_result(0),
		AsyncResult::Pending
	);
}

// One particular case where this occurs is if the Polkadot Runtime upgrade occurs after we've
// already signed a tx. In this case we know it will continue to fail if we keep rebroadcasting so
// we should stop and rethreshold sign using the new runtime version.
#[test]
fn re_request_threshold_signature_on_invalid_tx_params() {
	new_test_ext().execute_with(|| {
		let broadcast_attempt_id = start_mock_broadcast();

		assert_eq!(
			MockThresholdSigner::<MockEthereum, RuntimeCall>::signature_result(0),
			AsyncResult::Void
		);
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_some());
		assert_eq!(
			BroadcastAttemptCount::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id),
			0
		);

		MockTransactionBuilder::<MockEthereum, RuntimeCall>::set_invalid_for_rebroadcast();

		// If invalid on retry then we should re-threshold sign
		Broadcaster::on_initialize(BROADCAST_EXPIRY_BLOCKS + 1);
		threshold_signature_rerequested(broadcast_attempt_id);
	});
}

pub const ETH_DUMMY_SIG: SchnorrVerificationComponents =
	SchnorrVerificationComponents { s: [0xcf; 32], k_times_g_address: [0xcf; 20] };

#[test]
fn threshold_sign_and_broadcast_with_callback() {
	new_test_ext().execute_with(|| {
		let api_call = MockApiCall {
			payload: Default::default(),
			sig: Default::default(),
			tx_out_id: MOCK_TRANSACTION_OUT_ID,
		};

		let (broadcast_id, _threshold_request_id) =
			Broadcaster::threshold_sign_and_broadcast(api_call.clone(), Some(MockCallback));

		EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));

		assert_eq!(RequestCallbacks::<Test, Instance1>::get(broadcast_id), Some(MockCallback));
		assert_ok!(Broadcaster::transaction_succeeded(
			RuntimeOrigin::root(),
			MOCK_TRANSACTION_OUT_ID,
			Default::default(),
			ETH_TX_FEE,
		));
		assert!(RequestCallbacks::<Test, Instance1>::get(broadcast_id).is_none());
		let mut events = System::events();
		assert_eq!(
			events.pop().expect("an event").event,
			RuntimeEvent::Broadcaster(crate::Event::BroadcastSuccess {
				broadcast_id,
				transaction_out_id: api_call.tx_out_id
			})
		);
		assert_eq!(
			events.pop().expect("an event").event,
			RuntimeEvent::Broadcaster(crate::Event::BroadcastCallbackExecuted {
				broadcast_id,
				result: Ok(())
			})
		);
	});
}
