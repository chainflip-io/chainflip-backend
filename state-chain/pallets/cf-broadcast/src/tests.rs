use crate::{
	mock::*, AwaitingBroadcast, BroadcastAttemptCount, BroadcastAttemptId, BroadcastId,
	BroadcastRetryQueue, Error, Event as BroadcastEvent, FailedBroadcasters, Instance1,
	PalletOffence, RefundSignerId, SignatureToBroadcastIdLookup, ThresholdSignatureData, Timeouts,
	TransactionFeeDeficit, TransactionHashWhitelist, WeightInfo,
};

use sp_std::collections::btree_set::BTreeSet;

use cf_chains::{
	mocks::{
		MockApiCall, MockEthereum, MockThresholdSignature, MockUnsignedTransaction, Validity,
		ETH_TX_HASH,
	},
	ChainAbi,
};
use cf_traits::{
	mocks::{
		epoch_info::MockEpochInfo, signer_nomination::MockNominator,
		threshold_signer::MockThresholdSigner,
	},
	AsyncResult, EpochInfo, ThresholdSigner,
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
	pub static TIMEDOUT_ATTEMPTS: std::cell::RefCell<Vec<BroadcastAttemptId>> = Default::default();
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
			Event::Broadcaster(broadcast_event) => match broadcast_event {
				BroadcastEvent::TransactionBroadcastRequest {
					broadcast_attempt_id,
					nominee,
					unsigned_tx,
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
									RawOrigin::Signed(nominee + 1).into(),
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
							assert_noop!(
								Broadcaster::whitelist_transaction_for_refund(
									RawOrigin::Signed(nominee + 1).into(),
									broadcast_attempt_id,
									unsigned_tx.clone().signed(Validity::Valid),
									Validity::Valid
								),
								Error::<Test, Instance1>::InvalidSigner
							);
							// Only the nominee can return the signed tx.
							assert_ok!(Broadcaster::whitelist_transaction_for_refund(
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
				BroadcastEvent::RefundSignerIdUpdated { .. } => {
					// Information only. No action required by the CFE.
				},
				BroadcastEvent::ThresholdSignatureInvalid { .. } => {},
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
	assert!(FailedBroadcasters::<Test, Instance1>::get(broadcast_id).is_none());
	assert_eq!(BroadcastAttemptCount::<Test, Instance1>::get(broadcast_id), 0);
	assert!(ThresholdSignatureData::<Test, Instance1>::get(broadcast_id).is_none());
}

#[test]
fn test_broadcast_happy_path() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_some());

		// CFE responds with a signed transaction to whitelist for the refund.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_some());

		assert_ok!(Broadcaster::signature_accepted(
			Origin::root(),
			MockThresholdSignature::default(),
			Default::default(),
			ETH_TX_HASH,
		));
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_none());

		assert_broadcast_storage_cleaned_up(broadcast_attempt_id.broadcast_id);
	})
}

#[test]
fn test_abort_after_number_of_attempts_is_equal_to_the_number_of_authorities() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();

		let mut broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);

		for _ in 0..MockEpochInfo::current_authority_count() {
			// Nominated signer responds that they can't sign the transaction.
			MockCfe::respond(Scenario::SigningFailure);

			// retry should kick off at end of block if sufficient block space is free.
			Broadcaster::on_idle(0, LARGE_EXCESS_WEIGHT);
			Broadcaster::on_initialize(0);

			broadcast_attempt_id = broadcast_attempt_id.next_attempt();
		}

		assert_eq!(
			System::events().pop().expect("an event").event,
			Event::Broadcaster(crate::Event::BroadcastAborted {
				broadcast_id: broadcast_attempt_id.broadcast_id
			})
		);

		assert_broadcast_storage_cleaned_up(broadcast_attempt_id.broadcast_id);
	})
}

#[test]
fn on_idle_caps_broadcasts_when_not_enough_weight() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);

		MockCfe::respond(Scenario::SigningFailure);

		let broadcast_attempt_id_2 = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);

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
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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
fn test_bad_signature_when_whitelisting() {
	new_test_ext().execute_with(|| {
		// Set two nominees so we can check later that the failing id was excluded from
		// the nomination of the retry.
		MockNominator::set_nominees(Some(BTreeSet::from([1, 2])));

		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		let broadcast_request =
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).unwrap();
		assert_eq!(broadcast_request.broadcast_attempt.broadcast_attempt_id.attempt_count, 0);

		// CFE responds with an invalid transaction.
		MockCfe::respond(Scenario::BadSigner);

		// Broadcast is removed and scheduled for retry.
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_none());
		assert_eq!(BroadcastRetryQueue::<Test, Instance1>::decode_len().unwrap_or_default(), 1);

		// process retries
		Broadcaster::on_idle(0, 10_000_000_000);

		let next_broadcast_request =
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id.next_attempt()).unwrap();

		assert_eq!(next_broadcast_request.broadcast_attempt.broadcast_attempt_id.attempt_count, 1);
		assert!(broadcast_request.nominee != next_broadcast_request.nominee);
	})
}

#[test]
fn test_invalid_id_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Broadcaster::whitelist_transaction_for_refund(
				RawOrigin::Signed(0).into(),
				BroadcastAttemptId::default(),
				<<MockEthereum as ChainAbi>::UnsignedTransaction>::default()
					.signed(Validity::Valid),
				Validity::Valid
			),
			Error::<Test, Instance1>::InvalidBroadcastAttemptId
		);
		assert_noop!(
			Broadcaster::transaction_signing_failure(
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
			Broadcaster::signature_accepted(
				RawOrigin::Signed(0).into(),
				MockThresholdSignature::default(),
				0,
				[0u8; 4],
			),
			Error::<Test, Instance1>::InvalidPayload
		);
	})
}

// This is so someone can't get refunded if they were slow to submit their tx.
#[test]
fn cannot_whitelist_call_after_expired() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		assert!(
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id)
				.unwrap()
				.broadcast_attempt
				.broadcast_attempt_id
				.attempt_count == 0
		);
		let current_block = System::block_number();
		// we should have no Timeouts at this point, but in expiry blocks we should
		assert_eq!(Timeouts::<Test, Instance1>::get(current_block), vec![]);
		let expiry_block = current_block + BROADCAST_EXPIRY_BLOCKS;
		assert_eq!(Timeouts::<Test, Instance1>::get(expiry_block), vec![broadcast_attempt_id]);

		// Simulate the expiry hook for the expected expiry block.
		Broadcaster::on_initialize(expiry_block);

		// The first one reached expiry so we start a retry
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_none());
		let next_broadcast_attempt_id = broadcast_attempt_id.next_attempt();
		let broadcast_request =
			AwaitingBroadcast::<Test, Instance1>::get(next_broadcast_attempt_id).unwrap();
		assert_eq!(broadcast_request.broadcast_attempt.broadcast_attempt_id.attempt_count, 1);

		// This is a little confusing. Because we don't progress in blocks. i.e.
		// System::block_number() does not change
		// so when we retry the expired transaction, the *new* expiry block for the retry is
		// actually the same block since the current block number is unchanged
		// the current block number + BROADCAST_EXPIRY_BLOCKS is also unchanged
		// but, the retry has the incremented attempt_count of course
		assert_eq!(Timeouts::<Test, Instance1>::get(expiry_block), vec![next_broadcast_attempt_id]);

		// The first attempt has expired, which has triggered a second attempt.
		// We now whitelist the first call.
		assert_noop!(
			Broadcaster::whitelist_transaction_for_refund(
				RawOrigin::Signed(broadcast_request.nominee).into(),
				broadcast_attempt_id,
				broadcast_request.broadcast_attempt.unsigned_tx.clone().signed(Validity::Valid),
				Validity::Valid,
			),
			Error::<Test, Instance1>::InvalidBroadcastAttemptId
		);
		// We still shouldn't have a valid signer in the deficit map yet
		// or any of the related maps
		assert!(TransactionFeeDeficit::<Test, Instance1>::get(broadcast_request.nominee).is_none());
		assert!(TransactionHashWhitelist::<Test, Instance1>::get(ETH_TX_HASH).is_none());
		assert!(RefundSignerId::<Test, Instance1>::get(broadcast_request.nominee).is_none());
	});
}

// This is possible if someone tries to mess with us. They could just submit the tx to ETH without
// submitting the whitelist we check this to ensure there are no negative effects of this.
#[test]
fn signature_success_no_whitelisted_call_no_side_effects() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);

		const FEE_PAID: u128 = 200;
		assert_ok!(Broadcaster::signature_accepted(
			Origin::root(),
			MockThresholdSignature::default(),
			FEE_PAID,
			ETH_TX_HASH,
		));

		assert_broadcast_storage_cleaned_up(broadcast_attempt_id.broadcast_id);
	});
}

#[test]
fn signature_accepted_of_whitelisted_tx_hash_results_in_refund_for_whitelister() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		let tx_sig_request =
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).unwrap();

		let signed_tx = tx_sig_request.broadcast_attempt.unsigned_tx.signed(Validity::Valid);
		assert_ok!(Broadcaster::whitelist_transaction_for_refund(
			RawOrigin::Signed(tx_sig_request.nominee).into(),
			broadcast_attempt_id,
			signed_tx,
			Validity::Valid,
		));

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
		assert_ok!(Broadcaster::signature_accepted(
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

// the nodes who failed to broadcast should be report if we succeed, since success
// indicates the failed nodes could have succeeded themselves.
#[test]
fn signature_accepted_after_timeout_reports_failed_nodes() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);

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

		assert_ok!(Broadcaster::signature_accepted(
			Origin::root(),
			MockThresholdSignature::default(),
			Default::default(),
			ETH_TX_HASH,
		));

		MockOffenceReporter::assert_reported(
			PalletOffence::FailedToBroadcastTransaction,
			failed_authorities,
		);
	});
}

#[test]
fn signature_accepted_of_non_whitelisted_tx_hash_results_in_no_refund() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		let tx_sig_request =
			AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).unwrap();

		let signed_tx = tx_sig_request.broadcast_attempt.unsigned_tx.signed(Validity::Valid);
		assert_ok!(Broadcaster::whitelist_transaction_for_refund(
			RawOrigin::Signed(tx_sig_request.nominee).into(),
			broadcast_attempt_id,
			signed_tx,
			Validity::Valid,
		));

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
		// when they submitted `whitelist_transaction_for_refund`
		let mut bad_eth_tx_hash = ETH_TX_HASH;
		bad_eth_tx_hash[0] = ETH_TX_HASH[0] + 1;

		const FEE_PAID: u128 = 200;
		assert_ok!(Broadcaster::signature_accepted(
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
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
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

#[test]
fn re_request_threshold_signature() {
	new_test_ext().execute_with(|| {
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		let broadcast_attempt_id = Broadcaster::start_broadcast(
			&MockThresholdSignature::default(),
			MockUnsignedTransaction,
			MockApiCall::default(),
		);
		// Expect the threshold signature pipeline to be empty
		assert_eq!(
			MockThresholdSigner::<MockEthereum, Call>::signature_result(0),
			AsyncResult::Void
		);
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_some());
		assert_eq!(
			BroadcastAttemptCount::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id),
			0
		);
		// Simualte a key rotation to invalidate the signature
		MockKeyProvider::set_valid(false);
		Broadcaster::on_initialize(BROADCAST_EXPIRY_BLOCKS + 1);
		// Expect the broadcast to be deleted
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_attempt_id).is_none());
		// Verify storage has been deleted
		assert!(SignatureToBroadcastIdLookup::<Test, Instance1>::get(
			MockThresholdSignature::default()
		)
		.is_none());
		assert_eq!(
			BroadcastAttemptCount::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id),
			0
		);
		assert!(ThresholdSignatureData::<Test, Instance1>::get(broadcast_attempt_id.broadcast_id)
			.is_none());
		// Verify that we have a new signature request in the pipeline
		assert_eq!(
			MockThresholdSigner::<MockEthereum, Call>::signature_result(0),
			AsyncResult::Pending
		);
	});
}
