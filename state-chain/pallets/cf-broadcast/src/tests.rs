#![cfg(test)]

use crate::{
	mock::*, AbortedBroadcasts, AwaitingBroadcast, BroadcastData, BroadcastId, Config,
	DelayedBroadcastRetryQueue, Error, Event as BroadcastEvent, FailedBroadcasters, Instance1,
	PalletOffence, PendingBroadcasts, RequestFailureCallbacks, RequestSuccessCallbacks,
	ThresholdSignatureData, Timeouts, TransactionFeeDeficit, TransactionMetadata,
	TransactionOutIdToBroadcastId,
};
use cf_chains::{
	evm::SchnorrVerificationComponents,
	mocks::{
		ChainChoice, MockAggKey, MockApiCall, MockBroadcastBarriers, MockEthereum,
		MockEthereumChainCrypto, MockEthereumTransactionMetadata, MockThresholdSignature,
		MockTransactionBuilder, ETH_TX_FEE, MOCK_TRANSACTION_OUT_ID, MOCK_TX_METADATA,
	},
	ChainCrypto, FeeRefundCalculator,
};
use cf_traits::{
	mocks::{
		cfe_interface_mock::{MockCfeEvent, MockCfeInterface},
		signer_nomination::MockNominator,
		threshold_signer::MockThresholdSigner,
	},
	AsyncResult, Broadcaster as BroadcasterTrait, Chainflip, EpochInfo, SetSafeMode,
	ThresholdSigner,
};
use cfe_events::TxBroadcastRequest;
use frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{Get, Hooks, OriginTrait},
};
use frame_system::RawOrigin;
use sp_std::collections::btree_set::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
	HappyPath,
	BroadcastFailure,
	Timeout,
}

enum TxType {
	Normal,
	Rotation,
}

thread_local! {
	pub static TIMEDOUT_ATTEMPTS: std::cell::RefCell<Vec<BroadcastId>> = Default::default();
	pub static ABORTED_BROADCAST: std::cell::RefCell<BroadcastId> = Default::default();
}

type ValidatorId = <Test as Chainflip>::ValidatorId;

struct MockCfe;

impl MockCfe {
	fn respond(scenario: Scenario) {
		// Process non-cfe events (move this out of MockCfe?)
		let events = System::events();
		System::reset_events();
		for event_record in events {
			Self::process_event(event_record.event);
		}

		// Process cfe events
		let events = MockCfeInterface::take_events();
		for event in events {
			Self::process_cfe_event(event, scenario);
		}
	}

	fn process_cfe_event(event: MockCfeEvent<ValidatorId>, scenario: Scenario) {
		match event {
			MockCfeEvent::EthTxBroadcastRequest(TxBroadcastRequest {
				broadcast_id,
				nominee,
				payload: _,
			}) => {
				match scenario {
					Scenario::BroadcastFailure => {
						assert_ok!(Broadcaster::transaction_failed(
							RawOrigin::Signed(nominee).into(),
							broadcast_id,
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
			_ => {
				// No other events used in these tests
			},
		}
	}

	fn process_event(event: RuntimeEvent) {
		match event {
			RuntimeEvent::Broadcaster(broadcast_event) => match broadcast_event {
				BroadcastEvent::BroadcastTimeout { broadcast_id } =>
					TIMEDOUT_ATTEMPTS.with(|cell| cell.borrow_mut().push(broadcast_id)),
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
	assert!(FailedBroadcasters::<Test, Instance1>::get(broadcast_id).is_empty());
	assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);
	assert!(ThresholdSignatureData::<Test, Instance1>::get(broadcast_id).is_none());
	assert!(TransactionMetadata::<Test, Instance1>::get(broadcast_id).is_none());
	assert!(!PendingBroadcasts::<Test, Instance1>::get().contains(&broadcast_id))
}

fn start_mock_broadcast_tx_out_id(
	i: u8,
) -> (BroadcastId, <MockEthereumChainCrypto as ChainCrypto>::TransactionOutId) {
	let (tx_out_id, api_call) = api_call(i);
	let broadcast_id = initiate_and_sign_broadcast(&api_call, TxType::Normal);
	(broadcast_id, tx_out_id)
}

fn start_mock_broadcast() -> (BroadcastId, crate::TransactionOutIdFor<Test, Instance1>) {
	start_mock_broadcast_tx_out_id(Default::default())
}

fn new_mock_broadcast_attempt(
	broadcast_id: BroadcastId,
	nominee: u64,
) -> BroadcastData<Test, Instance1> {
	BroadcastData::<Test, Instance1> {
		broadcast_id,
		transaction_payload: Default::default(),
		threshold_signature_payload: Default::default(),
		transaction_out_id: Default::default(),
		nominee: Some(nominee),
	}
}

#[test]
fn transaction_succeeded_results_in_refund_for_signer() {
	new_test_ext().execute_with(|| {
		let (tx_out_id, api_call) = api_call(1);
		let broadcast_id = initiate_and_sign_broadcast(&api_call, TxType::Normal);

		let broadcast_data = AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).unwrap();

		let nominee = MockNominator::get_last_nominee().unwrap();

		assert_eq!(TransactionFeeDeficit::<Test, Instance1>::get(nominee), 0);

		witness_broadcast(tx_out_id);

		let expected_refund = broadcast_data.transaction_payload.return_fee_refund(ETH_TX_FEE);

		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).is_none());

		assert_eq!(TransactionFeeDeficit::<Test, Instance1>::get(nominee), expected_refund);

		assert_eq!(
			System::events().get(1).expect("an event").event,
			RuntimeEvent::Broadcaster(crate::Event::TransactionFeeDeficitRecorded {
				beneficiary: Default::default(),
				amount: expected_refund
			})
		);

		assert_broadcast_storage_cleaned_up(broadcast_id);
	});
}

#[test]
fn test_abort_after_number_of_attempts_is_equal_to_the_number_of_authorities() {
	new_test_ext().execute_with(|| {
		let (_tx_out_id, api_call) = api_call(1);
		let broadcast_id = initiate_and_sign_broadcast(&api_call, TxType::Normal);
		let next_block = System::block_number() + 1;
		for i in 0..MockEpochInfo::current_authority_count() {
			// Nominated signer responds that they can't sign the transaction.
			// retry should kick off at end of block if sufficient block space is free.
			assert_eq!(
				Broadcaster::attempt_count(broadcast_id),
				i,
				"Failed for {broadcast_id:?} at iteration {i}"
			);
			MockCfe::respond(Scenario::BroadcastFailure);
			Broadcaster::on_initialize(next_block);
		}

		assert_eq!(
			System::events().pop().expect("an event").event,
			RuntimeEvent::Broadcaster(crate::Event::BroadcastAborted { broadcast_id })
		);
	});
}

// Helper function: make a broadcast to be aborted upon the next failure.
fn ready_to_abort_broadcast(broadcast_id: BroadcastId) -> u64 {
	// Mock when all the possible broadcasts have failed another broadcast, and are
	// therefore aborted.
	let mut validators = MockEpochInfo::current_authorities();
	let nominee = validators.pop_first().unwrap();
	FailedBroadcasters::<Test, Instance1>::insert(broadcast_id, validators);
	nominee
}

#[test]
fn broadcasts_aborted_after_all_report_failures() {
	let mut broadcast_id = 0;
	new_test_ext().execute_with(|| {
		let (_tx_out_id1, api_call1) = api_call(1);

		broadcast_id = initiate_and_sign_broadcast(&api_call1, TxType::Normal);

		// Make it so the broadcast will be aborted on the next failure.
		let nominee = ready_to_abort_broadcast(broadcast_id);
		assert_ok!(Broadcaster::transaction_failed(
			RawOrigin::Signed(nominee).into(),
			broadcast_id,
		));

		// All validator reported broadcast failure - abort the broadcast.
		System::assert_last_event(RuntimeEvent::Broadcaster(crate::Event::BroadcastAborted {
			broadcast_id,
		}));
	});
}

#[test]
fn test_transaction_signing_failed() {
	new_test_ext()
		.execute_with(|| {
			let (broadcast_id, _) = start_mock_broadcast();
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);

			// CFE responds with a signed transaction. This moves us to the broadcast stage.
			MockCfe::respond(Scenario::BroadcastFailure);
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id));
			broadcast_id
		})
		.then_execute_at_next_block(|broadcast_id| {
			// Failed broadcasts are retried in the next block.
			assert!(AwaitingBroadcast::<Test, Instance1>::contains_key(broadcast_id));
		});
}

#[test]
fn test_invalid_id_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Broadcaster::transaction_failed(
				RawOrigin::Signed(
					*<Test as Chainflip>::EpochInfo::current_authorities().first().unwrap()
				)
				.into(),
				BroadcastId::default(),
			),
			Error::<Test, Instance1>::InvalidBroadcastId
		);
	});
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
				MOCK_TX_METADATA,
			),
			Error::<Test, Instance1>::InvalidPayload
		);
	});
}

// the nodes who failed to broadcast should be report if we succeed, since success
// indicates the failed nodes could have succeeded themselves.
#[test]
fn transaction_succeeded_after_timeout_reports_failed_nodes() {
	new_test_ext().execute_with(|| {
		let (tx_out_id, api_call) = api_call(1);
		initiate_and_sign_broadcast(&api_call, TxType::Normal);

		let mut failed_authorities = vec![];
		let next_block = System::block_number() + 1;
		// The last node succeeds
		for _ in 0..MockEpochInfo::current_authority_count() - 1 {
			// Nominated signer responds that they can't sign the transaction.
			MockCfe::respond(Scenario::BroadcastFailure);
			failed_authorities.push(MockNominator::get_last_nominee().unwrap());

			Broadcaster::on_initialize(next_block);
		}

		witness_broadcast(tx_out_id);

		MockOffenceReporter::assert_reported(
			PalletOffence::FailedToBroadcastTransaction,
			failed_authorities,
		);
	});
}

#[test]
fn test_signature_request_expiry() {
	let mut expiry = 0;
	let check_end_state = |broadcast_id| {
		// old attempt has expired, but the data still exists
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).is_some());
		assert_eq!(TIMEDOUT_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()), broadcast_id,);
		// New attempt is live with same broadcast_id and incremented attempt_count.
		assert_eq!(Broadcaster::attempt_count(broadcast_id), 1);
	};
	new_test_ext()
		.execute_with(|| {
			let (broadcast_id, _) = start_mock_broadcast();
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);
			expiry = System::block_number() + BROADCAST_EXPIRY_BLOCKS;
			broadcast_id
		})
		.then_execute_at_next_block(|broadcast_id| {
			MockCfe::respond(Scenario::Timeout);
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);
			broadcast_id
		})
		.then_execute_at_block(expiry, |broadcast_id| {
			MockCfe::respond(Scenario::Timeout);
			check_end_state(broadcast_id);
			broadcast_id
		})
		.then_execute_at_next_block(|broadcast_id| {
			// Subsequent calls to the hook have no further effect.
			MockCfe::respond(Scenario::Timeout);
			check_end_state(broadcast_id);
		});
}

#[test]
fn test_transmission_request_expiry() {
	let mut expiry = 0;
	let check_end_state = |broadcast_id| {
		assert_eq!(TIMEDOUT_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()), broadcast_id,);
		// New attempt is live with same broadcast_id and incremented attempt_count.
		assert_eq!(Broadcaster::attempt_count(broadcast_id), 1);
	};
	new_test_ext()
		.execute_with(|| {
			let (broadcast_id, _) = start_mock_broadcast();
			MockCfe::respond(Scenario::HappyPath);
			expiry = System::block_number() + BROADCAST_EXPIRY_BLOCKS;
			broadcast_id
		})
		.then_execute_at_block(expiry, |broadcast_id| {
			MockCfe::respond(Scenario::Timeout);
			check_end_state(broadcast_id);
			broadcast_id
		})
		.then_execute_at_block(expiry + 1, |broadcast_id| {
			// Subsequent calls to the hook have no further effect.
			MockCfe::respond(Scenario::Timeout);
			check_end_state(broadcast_id);
		});
}

// One particular case where this occurs is if the Polkadot Runtime upgrade occurs after we've
// already signed a tx. In this case we know it will continue to fail if we keep rebroadcasting so
// we should stop and re-threshold sign using the new runtime version.
#[test]
fn re_request_threshold_signature_on_invalid_tx_params() {
	let mut expiry = 0;
	new_test_ext()
		.execute_with(|| {
			let (_, api_call) = api_call(1);
			let broadcast_id = initiate_and_sign_broadcast(&api_call, TxType::Normal);

			assert_eq!(
				MockThresholdSigner::<MockEthereumChainCrypto, RuntimeCall>::signature_result(0),
				AsyncResult::Void
			);
			assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).is_some());
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);

			MockTransactionBuilder::<MockEthereum, RuntimeCall>::set_requires_refresh();
			expiry = System::block_number() + BROADCAST_EXPIRY_BLOCKS;
			broadcast_id
		})
		.then_execute_at_block(expiry, |broadcast_id| {
			// Verify storage has been deleted
			assert!(TransactionOutIdToBroadcastId::<Test, Instance1>::get(MOCK_TRANSACTION_OUT_ID)
				.is_none());
			// attempt count incremented for the same broadcast_id
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 1);
		})
		.then_execute_at_next_block(|_| {
			// Verify that we have a new signature request in the pipeline
			assert_eq!(
				MockThresholdSigner::<MockEthereumChainCrypto, RuntimeCall>::signature_result(0),
				AsyncResult::Pending
			);
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

		let broadcast_id =
			Broadcaster::threshold_sign_and_broadcast(api_call.clone(), Some(MockCallback), |_| {
				None
			});

		EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));

		assert_eq!(
			RequestSuccessCallbacks::<Test, Instance1>::get(broadcast_id),
			Some(MockCallback)
		);
		assert_ok!(Broadcaster::transaction_succeeded(
			RuntimeOrigin::root(),
			MOCK_TRANSACTION_OUT_ID,
			Default::default(),
			ETH_TX_FEE,
			MOCK_TX_METADATA,
		));
		assert!(RequestSuccessCallbacks::<Test, Instance1>::get(broadcast_id).is_none());
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

#[test]
fn ensure_safe_mode_is_moving_timeouts() {
	new_test_ext()
		.execute_with(|| {
			<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
			let _ = start_mock_broadcast();
			assert!(Timeouts::<Test, Instance1>::get(5u64).len() == 1);
		})
		.then_execute_at_block(5u64, |_| {})
		.then_execute_with(|_| {
			assert!(Timeouts::<Test, Instance1>::get(5u64).is_empty());
			assert!(Timeouts::<Test, Instance1>::get(15u64).len() == 1);
		});
}

#[test]
fn ensure_retries_are_skipped_during_safe_mode() {
	new_test_ext()
		.execute_with(|| {
			let _ = start_mock_broadcast();
			MockCfe::respond(Scenario::BroadcastFailure);
			let _ = start_mock_broadcast();
			MockCfe::respond(Scenario::BroadcastFailure);
			let next_block = System::block_number() + 1;
			assert_eq!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::decode_non_dedup_len(next_block),
				Some(2)
			);
			<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		})
		.then_execute_at_next_block(|()| {})
		.then_execute_with(|_| {
			let target = System::block_number() +
				<<Test as crate::Config<Instance1>>::SafeModeBlockMargin as Get<u64>>::get();
			assert_eq!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::decode_non_dedup_len(target),
				Some(2)
			);
		});
}

#[test]
fn transaction_succeeded_results_in_refund_refuse_for_signer() {
	new_test_ext().execute_with(|| {
		MockEthereumTransactionMetadata::set_validity(false);

		let (tx_out_id, api_call) = api_call(1);
		initiate_and_sign_broadcast(&api_call, TxType::Normal);

		let nominee = MockNominator::get_last_nominee().unwrap();

		assert_eq!(TransactionFeeDeficit::<Test, Instance1>::get(nominee), 0);

		witness_broadcast(tx_out_id);

		assert_eq!(
			System::events().get(1).expect("an event").event,
			RuntimeEvent::Broadcaster(crate::Event::TransactionFeeDeficitRefused {
				beneficiary: Default::default(),
			})
		);
	});
}

#[test]
fn callback_is_called_upon_broadcast_failure() {
	new_test_ext().execute_with(|| {
		let api_call = MockApiCall {
			payload: Default::default(),
			sig: Default::default(),
			tx_out_id: MOCK_TRANSACTION_OUT_ID,
		};
		let broadcast_id =
			Broadcaster::threshold_sign_and_broadcast(api_call.clone(), None, |_| {
				Some(MockCallback)
			});

		assert_eq!(
			RequestFailureCallbacks::<Test, Instance1>::get(broadcast_id),
			Some(MockCallback)
		);
		assert!(!MockCallback::was_called());

		AwaitingBroadcast::<Test, Instance1>::insert(
			broadcast_id,
			new_mock_broadcast_attempt(broadcast_id, 0u64),
		);
		ThresholdSignatureData::<Test, Instance1>::insert(
			broadcast_id,
			(
				api_call,
				MockThresholdSignature {
					signing_key: MockAggKey([0u8; 4]),
					signed_payload: [0u8; 4],
				},
			),
		);

		// Broadcast fails when no broadcaster can be nominated.
		let nominee = ready_to_abort_broadcast(broadcast_id);

		assert_ok!(Broadcaster::transaction_failed(
			RawOrigin::Signed(nominee).into(),
			broadcast_id,
		));

		// This should trigger the failed callback
		assert!(MockCallback::was_called());
	});
}

#[test]
fn retry_and_success_in_same_block() {
	new_test_ext()
		.execute_with(|| {
			// Setup
			let (broadcast_id, transaction_out_id) = start_mock_broadcast();
			(MockNominator::get_last_nominee().unwrap(), broadcast_id, transaction_out_id)
		})
		.then_apply_extrinsics(|(nominee, broadcast_id, transaction_out_id)| {
			[
				(
					OriginTrait::signed(*nominee),
					RuntimeCall::Broadcaster(crate::Call::transaction_failed {
						broadcast_id: *broadcast_id,
					}),
					Ok(()),
				),
				(
					OriginTrait::root(),
					RuntimeCall::Broadcaster(
						crate::Call::<Test, Instance1>::transaction_succeeded {
							tx_out_id: *transaction_out_id,
							signer_id: Default::default(),
							tx_fee: cf_chains::evm::TransactionFee {
								effective_gas_price: Default::default(),
								gas_used: Default::default(),
							},
							tx_metadata: Default::default(),
						},
					),
					Ok(()),
				),
			]
		});
}

// When we retry threshold signing, we want to make sure that the storage remains valid such that if
// there is transaction_succeeded witnessed late due to some delay, the success still goes through.
#[test]
fn retry_with_threshold_signing_still_allows_late_success_witness_second_attempt() {
	let mut expected_expiry_block = 0;
	const MOCK_TRANSACTION_OUT_ID: [u8; 4] = [0xbc; 4];
	new_test_ext()
		.execute_with(|| {
			let (broadcast_id, _) = start_mock_broadcast_tx_out_id(0xbc);

			let awaiting_broadcast =
				AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).unwrap();

			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);
			let nominee = MockNominator::get_last_nominee().unwrap();

			let current_block = frame_system::Pallet::<Test>::block_number();

			expected_expiry_block = current_block + BROADCAST_EXPIRY_BLOCKS;

			assert_eq!(
				Timeouts::<Test, Instance1>::get(expected_expiry_block)
					.into_iter()
					.next()
					.unwrap()
					.0,
				broadcast_id
			);

			// We want to run test test on the second attempt.
			MockCfe::respond(Scenario::BroadcastFailure);
			(nominee, awaiting_broadcast)
		})
		// on idle runs and the retry is kicked off.
		.then_execute_at_next_block(|p| p)
		.then_execute_at_next_block(|(nominee, broadcast_data)| {
			assert_eq!(Broadcaster::attempt_count(broadcast_data.broadcast_id), 1);
			MockTransactionBuilder::<MockEthereum, RuntimeCall>::set_requires_refresh();
			MockCfe::respond(Scenario::Timeout);
			(nominee, broadcast_data)
		})
		// The broadcast times out
		.then_execute_at_block(expected_expiry_block, |p| p)
		.then_execute_at_next_block(|(nominee, broadcast_data)| {
			// Taking the invalid signature code path results in the metadata being removed, so the
			// check for the fee is ignored, however, the transaction_succeeded should still pass.
			assert_ok!(Broadcaster::transaction_succeeded(
				OriginTrait::root(),
				MOCK_TRANSACTION_OUT_ID,
				nominee,
				ETH_TX_FEE,
				MOCK_TX_METADATA,
			));

			assert_eq!(
				TransactionFeeDeficit::<Test, Instance1>::get(nominee),
				broadcast_data.transaction_payload.return_fee_refund(ETH_TX_FEE)
			);
		});
}

#[test]
fn broadcast_barrier_for_polkadot() {
	new_test_ext()
		.execute_with(|| {
			MockBroadcastBarriers::set(ChainChoice::Polkadot);

			let (tx_out_id1, api_call1) = api_call(1);
			let (tx_out_id2, api_call2) = api_call(2);
			let (tx_out_id3, api_call3) = api_call(3);

			// create and sign 3 txs that are then ready for broadcast
			let broadcast_id_1 = initiate_and_sign_broadcast(&api_call1, TxType::Normal);
			// tx1 emits broadcast request
			assert_transaction_broadcast_request_event(broadcast_id_1, tx_out_id1);

			let broadcast_id_2 = initiate_and_sign_broadcast(&api_call2, TxType::Rotation);
			// tx2 emits broadcast request and also pauses any further new broadcast requests
			assert_transaction_broadcast_request_event(broadcast_id_2, tx_out_id2);

			let broadcast_id_3 = initiate_and_sign_broadcast(&api_call3, TxType::Normal);

			// tx3 is ready for broadcast but since there is a broadcast pause, broadcast request is
			// not issued, the broadcast is rescheduled instead.
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastRetryScheduled {
					broadcast_id: broadcast_id_3,
					retry_block: System::block_number() + 1,
				},
			));

			// report successful broadcast of tx1
			witness_broadcast(tx_out_id1);

			// tx3 should still not be broadcasted because the blocking tx (tx2) has still not
			// succeeded.
			(tx_out_id2, tx_out_id3, broadcast_id_3)
		})
		.then_execute_at_next_block(|(tx_out_id2, tx_out_id3, broadcast_id_3)| {
			(tx_out_id2, tx_out_id3, broadcast_id_3)
		})
		.then_execute_with(|(tx_out_id2, tx_out_id3, broadcast_id_3)| {
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id_3));

			// Now tx2 succeeds which should allow tx3 to be broadcast
			witness_broadcast(tx_out_id2);
			(tx_out_id3, broadcast_id_3)
		})
		.then_execute_at_next_block(|(tx_out_id3, broadcast_id_3)| (tx_out_id3, broadcast_id_3))
		.then_execute_with(|(tx_out_id3, broadcast_id_3)| {
			// attempt count is 1 because the previous failure to broadcast because of
			// broadcast pause is considered an attempt
			assert_transaction_broadcast_request_event(broadcast_id_3, tx_out_id3);

			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block).is_empty());

			witness_broadcast(tx_out_id3);
		});
}

#[test]
fn broadcast_barrier_for_bitcoin() {
	new_test_ext().execute_with(|| {
		MockBroadcastBarriers::set(ChainChoice::Bitcoin);

		let (tx_out_id1, api_call1) = api_call(1);
		let (tx_out_id2, api_call2) = api_call(2);
		let (tx_out_id3, api_call3) = api_call(3);

		// create and sign 3 txs that are then ready for broadcast
		let broadcast_id_1 = initiate_and_sign_broadcast(&api_call1, TxType::Normal);
		// tx1 emits broadcast request
		assert_transaction_broadcast_request_event(broadcast_id_1, tx_out_id1);

		let broadcast_id_2 = initiate_and_sign_broadcast(&api_call2, TxType::Rotation);
		// tx2 emits broadcast request and does not pause future broadcasts in bitcoin
		assert_transaction_broadcast_request_event(broadcast_id_2, tx_out_id2);

		let broadcast_id_3 = initiate_and_sign_broadcast(&api_call3, TxType::Normal);
		// tx3 emits broadcast request
		assert_transaction_broadcast_request_event(broadcast_id_3, tx_out_id3);

		// we successfully witness all txs
		witness_broadcast(tx_out_id1);
		witness_broadcast(tx_out_id2);
		witness_broadcast(tx_out_id3);
	});
}

#[test]
fn broadcast_barrier_for_ethereum() {
	new_test_ext()
		.execute_with(|| {
			MockBroadcastBarriers::set(ChainChoice::Ethereum);

			let (tx_out_id1, api_call1) = api_call(1);
			let (tx_out_id2, api_call2) = api_call(2);
			let (tx_out_id3, api_call3) = api_call(3);
			let (tx_out_id4, api_call4) = api_call(4);

			let broadcast_id_1 = initiate_and_sign_broadcast(&api_call1, TxType::Normal);
			// tx1 emits broadcast request
			assert_transaction_broadcast_request_event(broadcast_id_1, tx_out_id1);

			let broadcast_id_2 = initiate_and_sign_broadcast(&api_call2, TxType::Normal);
			// tx2 emits broadcast request
			assert_transaction_broadcast_request_event(broadcast_id_2, tx_out_id2);

			// this will put a broadcast barrier at tx2 and tx3. tx3 wont be broadcasted yet
			let broadcast_id_3 = initiate_and_sign_broadcast(&api_call3, TxType::Rotation);

			// tx3 is ready for broadcast but since there is a broadcast pause, broadcast request is
			// not issued, the broadcast is rescheduled instead.
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastRetryScheduled {
					broadcast_id: broadcast_id_3,
					retry_block: System::block_number() + 1,
				},
			));

			// tx4 will be created but not broadcasted yet
			let broadcast_id_4 = initiate_and_sign_broadcast(&api_call4, TxType::Normal);
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastRetryScheduled {
					broadcast_id: broadcast_id_4,
					retry_block: System::block_number() + 1,
				},
			));

			// report successful broadcast of tx2
			witness_broadcast(tx_out_id2);

			// tx3 and tx4 should still not be broadcasted because not all txs before and including
			// tx2 have been witnessed
			(tx_out_id1, tx_out_id3, broadcast_id_3, tx_out_id4, broadcast_id_4)
		})
		.then_execute_at_next_block(
			|(tx_out_id1, tx_out_id3, broadcast_id_3, tx_out_id4, broadcast_id_4)| {
				(tx_out_id1, tx_out_id3, broadcast_id_3, tx_out_id4, broadcast_id_4)
			},
		)
		.then_execute_with(
			|(tx_out_id1, tx_out_id3, broadcast_id_3, tx_out_id4, broadcast_id_4)| {
				let next_block = System::block_number() + 1;
				assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
					.contains(&broadcast_id_3));
				assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
					.contains(&broadcast_id_4));

				// Now tx1 succeeds which should allow tx3 to be broadcast but not tx4 since there
				// will be another barrier at tx3
				witness_broadcast(tx_out_id1);
				(tx_out_id3, broadcast_id_3, tx_out_id4, broadcast_id_4)
			},
		)
		.then_execute_at_next_block(|(tx_out_id3, broadcast_id_3, tx_out_id4, broadcast_id_4)| {
			(tx_out_id3, broadcast_id_3, tx_out_id4, broadcast_id_4)
		})
		.then_execute_with(|(tx_out_id3, broadcast_id_3, tx_out_id4, broadcast_id_4)| {
			// attempt count is 1 because the previous failure to broadcast because of
			// broadcast pause is considered an attempt
			assert_transaction_broadcast_request_event(broadcast_id_3, tx_out_id3);

			// tx4 is still pending
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id_4));

			// witness tx3 which should allow tx4 to be broadcast
			witness_broadcast(tx_out_id3);
			(tx_out_id4, broadcast_id_4)
		})
		.then_execute_at_next_block(|(tx_out_id4, broadcast_id_4)| (tx_out_id4, broadcast_id_4))
		.then_execute_with(|(tx_out_id4, broadcast_id_4)| {
			assert_transaction_broadcast_request_event(broadcast_id_4, tx_out_id4);
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block).is_empty());
			witness_broadcast(tx_out_id4);
		});
}

fn api_call(i: u8) -> ([u8; 4], MockApiCall<MockEthereumChainCrypto>) {
	let tx_out_id = [i; 4];
	(tx_out_id, MockApiCall { tx_out_id, sig: Default::default(), payload: Default::default() })
}

fn assert_transaction_broadcast_request_event(broadcast_id: BroadcastId, tx_out_id: [u8; 4]) {
	System::assert_last_event(RuntimeEvent::Broadcaster(
		crate::Event::<Test, Instance1>::TransactionBroadcastRequest {
			transaction_out_id: tx_out_id,
			broadcast_id,
			transaction_payload: Default::default(),
			nominee: MockNominator::get_last_nominee().unwrap(),
		},
	));
}

fn initiate_and_sign_broadcast(
	api_call: &MockApiCall<MockEthereumChainCrypto>,
	tx_type: TxType,
) -> BroadcastId {
	let broadcast_id = match tx_type {
		TxType::Normal => <Broadcaster as BroadcasterTrait<
			<Test as Config<Instance1>>::TargetChain,
		>>::threshold_sign_and_broadcast((*api_call).clone()),
		TxType::Rotation => <Broadcaster as BroadcasterTrait<
			<Test as Config<Instance1>>::TargetChain,
		>>::threshold_sign_and_broadcast_rotation_tx((*api_call).clone()),
	};

	EthMockThresholdSigner::execute_signature_result_against_last_request(Ok(ETH_DUMMY_SIG));

	broadcast_id
}

fn witness_broadcast(tx_out_id: [u8; 4]) {
	assert_ok!(Broadcaster::transaction_succeeded(
		RuntimeOrigin::root(),
		tx_out_id,
		Default::default(),
		ETH_TX_FEE,
		MOCK_TX_METADATA,
	));
}

#[test]
fn timed_out_broadcaster_are_reported() {
	let mut expiry = 0u64;
	new_test_ext()
		.execute_with(|| {
			let (broadcast_id, _) = start_mock_broadcast();
			expiry = System::block_number()
				.saturating_add(<Test as crate::Config<Instance1>>::BroadcastTimeout::get());
			let nominee = AwaitingBroadcast::<Test, Instance1>::get(broadcast_id)
				.unwrap()
				.nominee
				.unwrap();

			assert!(FailedBroadcasters::<Test, Instance1>::get(broadcast_id).is_empty());
			(broadcast_id, nominee)
		})
		.then_execute_at_block(expiry, |(broadcast_id, nominee)| {
			// The nominated broadcaster is added to `FailedBroadcasters` to be reported later.
			assert_eq!(
				FailedBroadcasters::<Test, Instance1>::get(broadcast_id),
				BTreeSet::from([nominee])
			);
		});
}

#[test]
fn broadcast_can_be_aborted_due_to_time_out() {
	let mut expiry = 0;
	new_test_ext()
		.execute_with(|| {
			let (broadcast_id, _) = start_mock_broadcast();
			expiry = System::block_number()
				.saturating_add(<Test as crate::Config<Instance1>>::BroadcastTimeout::get());
			ready_to_abort_broadcast(broadcast_id);

			broadcast_id
		})
		.then_execute_at_block(expiry, |broadcast_id| broadcast_id)
		.then_execute_with(|broadcast_id| {
			// Broadcast should be aborted
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastAborted { broadcast_id },
			));
			assert!(AbortedBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));
			assert!(
				FailedBroadcasters::<Test, Instance1>::decode_non_dedup_len(broadcast_id).is_none()
			);
		});
}

#[test]
fn aborted_broadcasts_can_still_succeed() {
	let mut expiry = 0;
	new_test_ext()
		.execute_with(|| {
			let (broadcast_id, transaction_out_id) = start_mock_broadcast();
			expiry = System::block_number()
				.saturating_add(<Test as crate::Config<Instance1>>::BroadcastTimeout::get());
			ready_to_abort_broadcast(broadcast_id);

			(broadcast_id, transaction_out_id)
		})
		.then_execute_at_block(expiry, |(broadcast_id, transaction_out_id)| {
			(broadcast_id, transaction_out_id)
		})
		.then_execute_with(|(broadcast_id, transaction_out_id)| {
			// Broadcast should be aborted
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastAborted { broadcast_id },
			));
			assert!(
				FailedBroadcasters::<Test, Instance1>::decode_non_dedup_len(broadcast_id).is_none()
			);
			assert!(AbortedBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));

			// Broadcast can still be reported as successful
			assert_ok!(Broadcaster::transaction_succeeded(
				RuntimeOrigin::root(),
				transaction_out_id,
				Default::default(),
				ETH_TX_FEE,
				MOCK_TX_METADATA,
			));

			// Storage should be cleaned, event emitted.
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastSuccess {
					broadcast_id,
					transaction_out_id,
				},
			));
			assert_broadcast_storage_cleaned_up(broadcast_id);
		});
}

#[test]
fn broadcast_retry_delay_works() {
	let mut target = 0;
	let mut broadcast_id = 0;
	let mut tx_out_id = Default::default();
	let delay = 10;
	new_test_ext()
		.execute_with(|| {
			(broadcast_id, tx_out_id) = start_mock_broadcast();

			BroadcastDelay::set(None);
			// With no delay, retries are added to the normal queue, and is retried in the next
			// block.
			let next_block = System::block_number() + 1;
			assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(0u64), broadcast_id));
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id));
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastRetryScheduled {
					broadcast_id,
					retry_block: next_block,
				},
			));
		})
		.then_execute_at_next_block(|_| {})
		.then_execute_with(|_| {
			BroadcastDelay::set(Some(delay));
			// Set delay - retries will be added to the Delayed queue.
			assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(1u64), broadcast_id));
			target = System::block_number() + delay;

			let next_block = System::block_number() + 1;
			assert_eq!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::decode_non_dedup_len(next_block),
				None
			);
			assert!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::get(target).contains(&broadcast_id),
			);
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastRetryScheduled {
					broadcast_id,
					retry_block: target,
				},
			));
		})
		.then_execute_at_block(target, |_| {})
		.then_execute_with(|_| {
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::decode_non_dedup_len(
				next_block
			)
			.is_none());
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::decode_non_dedup_len(target)
				.is_none());

			assert_transaction_broadcast_request_event(broadcast_id, tx_out_id);
		});
}

#[test]
fn broadcast_timeout_delay_works() {
	let mut target = 0;
	let mut broadcast_id = 0;
	let delay = 10;
	new_test_ext()
		.execute_with(|| {
			(broadcast_id, _) = start_mock_broadcast();

			BroadcastDelay::set(Some(delay));
			target = System::block_number() + BROADCAST_EXPIRY_BLOCKS;
		})
		.then_execute_at_block(target, |_| {})
		.then_execute_with(|_| {
			target = System::block_number() + delay;

			assert!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::get(target).contains(&broadcast_id),
			);
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastRetryScheduled {
					broadcast_id,
					retry_block: target,
				},
			));
		});
}

#[test]
fn aborted_broadcasts_will_not_retry() {
	let mut target = 0;
	let mut broadcast_id = 0;
	let delay = 100;
	new_test_ext()
		.execute_with(|| {
			(broadcast_id, _) = start_mock_broadcast();
			BroadcastDelay::set(Some(delay));
			target = System::block_number() + delay;
			assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(0u64), broadcast_id));
			assert!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::get(target).contains(&broadcast_id,)
			);

			// Abort the broadcast
			let nominee = ready_to_abort_broadcast(broadcast_id);
			assert_ok!(Broadcaster::transaction_failed(
				RuntimeOrigin::signed(nominee),
				broadcast_id
			));
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastAborted { broadcast_id },
			));
		})
		.then_execute_at_block(target, |_| {})
		.then_execute_with(|_| {
			// assert no retry happened
			assert!(FailedBroadcasters::<Test, Instance1>::get(broadcast_id).is_empty());
			assert!(!PendingBroadcasts::<Test, Instance1>::get().contains(&broadcast_id))
		});
}

#[test]
fn succeeded_broadcasts_will_not_retry() {
	let mut target = 0;
	let delay = 100;
	new_test_ext()
		.execute_with(|| {
			let (broadcast_id, transaction_out_id) = start_mock_broadcast();
			BroadcastDelay::set(Some(delay));
			target = System::block_number() + delay;
			assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(0u64), broadcast_id));
			assert!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::get(target).contains(&broadcast_id)
			);

			// Report broadcast as Succeeded
			assert_ok!(Broadcaster::transaction_succeeded(
				RuntimeOrigin::root(),
				transaction_out_id,
				Default::default(),
				ETH_TX_FEE,
				MOCK_TX_METADATA,
			));
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastSuccess {
					broadcast_id,
					transaction_out_id,
				},
			));
			broadcast_id
		})
		.then_execute_at_block(target, |broadcast_id| broadcast_id)
		.then_execute_with(|broadcast_id| {
			// assert no retry happened
			assert_broadcast_storage_cleaned_up(broadcast_id);

			// Further reports of failure will be ignored
			assert_err!(
				Broadcaster::transaction_failed(RawOrigin::Signed(1).into(), broadcast_id,),
				Error::<Test, Instance1>::InvalidBroadcastId
			);

			assert_broadcast_storage_cleaned_up(broadcast_id);
		});
}

#[test]
fn broadcast_retries_will_not_be_overwritten_during_safe_mode() {
	let mut target: u64 = 0u64;
	let mut broadcast_id = 0;
	new_test_ext()
		.then_execute_at_block(1_000u64, |_| {
			(broadcast_id, _) = start_mock_broadcast();
			BroadcastDelay::set(Some(1));
			assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(0u64), broadcast_id));

			// On safe mode next block, storage will be re-added to this target block.
			let next_block = System::block_number() + 1u64;
			target = next_block +
				<<Test as crate::Config<Instance1>>::SafeModeBlockMargin as Get<u64>>::get();

			// Ensure next block's data is ready to be re-scheduled during safe mode.
			Timeouts::<Test, Instance1>::insert(
				next_block,
				BTreeSet::from_iter(vec![(100, 0), (101, 0), (102, 0), (105, 0), (106, 0)]),
			);
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id,));

			// add mock data to the target block storage.
			Timeouts::<Test, Instance1>::insert(
				target,
				BTreeSet::from_iter(vec![(100, 0), (101, 0), (102, 0), (103, 0), (104, 0)]),
			);
			DelayedBroadcastRetryQueue::<Test, Instance1>::append(target, 100);
			DelayedBroadcastRetryQueue::<Test, Instance1>::append(target, 101);

			// Activate safe mode code red.
			<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		})
		.then_execute_at_next_block(|_| {})
		.then_execute_with(|_| {
			// Hook should re-schedule the `Timeouts` and Broadcast retries.
			// Entries should be appended to the target block's storage, not replace it.
			assert_eq!(
				Timeouts::<Test, Instance1>::get(target),
				// 105 and 106 are added from the next_block's storage.
				BTreeSet::from_iter(vec![
					(100, 0),
					(101, 0),
					(102, 0),
					(103, 0),
					(104, 0),
					(105, 0),
					(106, 0)
				])
			);
			assert_eq!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::get(target),
				BTreeSet::from_iter([100, 101, broadcast_id])
			);
		});
}

#[test]
fn broadcast_is_retried_without_initial_nominee() {
	let mut broadcast_id = 0;
	let mut transaction_out_id = Default::default();
	new_test_ext()
		.then_execute_at_block(1_000u64, |_| {
			// Configure so no nominee can be selected for the very first time.
			MockNominator::set_nominees(Some(Default::default()));

			(broadcast_id, transaction_out_id) = start_mock_broadcast();

			// Broadcast should be retried next block
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id));

			// Make nominees available
			MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		})
		.then_execute_at_next_block(|_| {})
		.then_execute_with(|_| {
			// Broadcast can now succeed.
			assert_ok!(Broadcaster::transaction_succeeded(
				RuntimeOrigin::root(),
				transaction_out_id,
				Default::default(),
				ETH_TX_FEE,
				MOCK_TX_METADATA,
			));

			// Storage should be cleaned, event emitted.
			System::assert_last_event(RuntimeEvent::Broadcaster(
				crate::Event::<Test, Instance1>::BroadcastSuccess {
					broadcast_id,
					transaction_out_id,
				},
			));
			assert_broadcast_storage_cleaned_up(broadcast_id);
		});
}
