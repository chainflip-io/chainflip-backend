// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

use core::cmp::max;

use crate::{
	mock::*, AbortedBroadcasts, AggKey, AwaitingBroadcast, BroadcastBarriers, BroadcastData,
	BroadcastId, ChainBlockNumberFor, Config, DelayedBroadcastRetryQueue, Error,
	Event as BroadcastEvent, Event, FailedBroadcasters, Instance1, PalletConfigUpdate,
	PalletOffence, PendingApiCalls, PendingBroadcasts, RequestFailureCallbacks,
	RequestSuccessCallbacks, Timeouts, TransactionMetadata, TransactionOutIdToBroadcastId,
};
use cf_chains::{
	mocks::{
		ChainChoice, MockAggKey, MockApiCall, MockBroadcastBarriers, MockEthereum,
		MockEthereumChainCrypto, MockEthereumTransactionMetadata, MockThresholdSignature,
		MockTransactionBuilder, ETH_TX_FEE, MOCK_TX_METADATA,
	},
	ChainCrypto, FeeRefundCalculator, ForeignChain,
};
use cf_test_utilities::last_event;
use cf_traits::{
	mocks::{
		block_height_provider::BlockHeightProvider,
		cfe_interface_mock::{MockCfeEvent, MockCfeInterface},
		liability_tracker::MockLiabilityTracker,
		signer_nomination::MockNominator,
		threshold_signer::MockThresholdSigner,
	},
	AsyncResult, Broadcaster as BroadcasterTrait, Chainflip, EpochInfo, GetBlockHeight,
	SetSafeMode, ThresholdSigner,
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
	Rotation { new_key: AggKey<Test, Instance1> },
}

thread_local! {
	pub static TIMEDOUT_ATTEMPTS: std::cell::RefCell<Vec<BroadcastId>> = Default::default();
	pub static ABORTED_BROADCAST: std::cell::RefCell<BroadcastId> = Default::default();
}

type ValidatorId = <Test as Chainflip>::ValidatorId;

fn mock_api_call() -> MockApiCall<MockEthereumChainCrypto> {
	MockApiCall { signer_and_signature: None, payload: Default::default() }
}

const SIG1: <MockEthereumChainCrypto as ChainCrypto>::ThresholdSignature =
	MockThresholdSignature { signing_key: MockAggKey([0xaa; 4]), signed_payload: [0xaa; 4] };

const SIG2: <MockEthereumChainCrypto as ChainCrypto>::ThresholdSignature =
	MockThresholdSignature { signing_key: MockAggKey([0xbb; 4]), signed_payload: [0xbb; 4] };

const SIG3: <MockEthereumChainCrypto as ChainCrypto>::ThresholdSignature =
	MockThresholdSignature { signing_key: MockAggKey([0xcc; 4]), signed_payload: [0xcc; 4] };

const SIG4: <MockEthereumChainCrypto as ChainCrypto>::ThresholdSignature =
	MockThresholdSignature { signing_key: MockAggKey([0xdd; 4]), signed_payload: [0xdd; 4] };

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
	// There should be no transaction out id for this broadcast id
	// Note that there can be multiple transaction out ids for the same broadcast id
	// if re-signing occurs.
	assert!(!TransactionOutIdToBroadcastId::<Test, Instance1>::iter()
		.any(|(_, (b_id, _))| b_id == broadcast_id));
	assert!(FailedBroadcasters::<Test, Instance1>::get(broadcast_id).is_empty());
	assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);
	assert!(PendingApiCalls::<Test, Instance1>::get(broadcast_id).is_none());
	assert!(TransactionMetadata::<Test, Instance1>::get(broadcast_id).is_none());
	assert!(!PendingBroadcasts::<Test, Instance1>::get().contains(&broadcast_id))
}

fn start_mock_broadcast(
	mock_sig: <MockEthereumChainCrypto as ChainCrypto>::ThresholdSignature,
) -> BroadcastId {
	initiate_and_sign_broadcast(&mock_api_call(), mock_sig, TxType::Normal)
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

/// Since there might be multiple entries with the same timeout chainblock number,
/// we collect all of their "values" into a single `BTreeSet`. This improves the
/// readability of a few test cases. Since we don't care about the order of the timeouts,
/// we use a `BTreeSet` instead of a vector.
fn get_timeouts_for(
	chainblock: ChainBlockNumberFor<Test, Instance1>,
) -> BTreeSet<(BroadcastId, ValidatorId)> {
	let mut result = BTreeSet::new();
	for (timeout, broadcast_id, nominee) in Timeouts::<Test, Instance1>::get() {
		if timeout == chainblock {
			result.insert((broadcast_id, nominee));
		}
	}
	result
}
/// Append multiple timeout entries for the same target chain block.
fn append_timeouts_for(
	chainblock: ChainBlockNumberFor<Test, Instance1>,
	timeouts: Vec<(BroadcastId, ValidatorId)>,
) {
	for (broadcast_id, nominee) in timeouts {
		Timeouts::<Test, Instance1>::append((chainblock, broadcast_id, nominee));
	}
}

#[test]
fn transaction_succeeded_results_in_refund_for_signer() {
	new_test_ext().execute_with(|| {
		let api_call = mock_api_call();
		let broadcast_id = initiate_and_sign_broadcast(&api_call, SIG1, TxType::Normal);

		let broadcast_data = AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).unwrap();

		assert_eq!(MockLiabilityTracker::total_liabilities(ForeignChain::Ethereum.gas_asset()), 0);

		witness_broadcast(SIG1);

		let expected_refund = broadcast_data.transaction_payload.return_fee_refund(ETH_TX_FEE);

		assert_eq!(
			MockLiabilityTracker::total_liabilities(ForeignChain::Ethereum.gas_asset()),
			expected_refund
		);

		assert_eq!(
			System::events().get(1).expect("an event").event,
			RuntimeEvent::Broadcaster(Event::TransactionFeeDeficitRecorded {
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
		let broadcast_id = initiate_and_sign_broadcast(&mock_api_call(), SIG1, TxType::Normal);
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
			RuntimeEvent::Broadcaster(Event::BroadcastAborted { broadcast_id })
		);
	});
}

// Helper function: make a broadcast to be aborted upon the next failure.
fn ready_to_abort_broadcast(broadcast_id: BroadcastId) -> u64 {
	// Extract nominee for current broadcast_id from the Timeouts storage.
	// If none can be found the default is 0.
	let mut nominee = 0;
	for (_, id, nom) in Timeouts::<Test, Instance1>::get() {
		if id == broadcast_id {
			nominee = nom;
			break;
		}
	}

	// Mock when all the possible broadcasts have failed another broadcast, and are
	// therefore aborted.
	let mut validators =
		MockEpochInfo::current_authorities().into_iter().collect::<BTreeSet<u64>>();

	// The nominee should be the last one *not* in the `FailedBroadcasters` list
	validators.remove(&nominee);
	FailedBroadcasters::<Test, Instance1>::insert(broadcast_id, validators);
	nominee
}

#[test]
fn broadcasts_aborted_after_all_report_failures() {
	new_test_ext().execute_with(|| {
		let broadcast_id = initiate_and_sign_broadcast(&mock_api_call(), SIG1, TxType::Normal);

		// Make it so the broadcast will be aborted on the next failure.
		let nominee = ready_to_abort_broadcast(broadcast_id);
		assert_ok!(Broadcaster::transaction_failed(
			RawOrigin::Signed(nominee).into(),
			broadcast_id,
		));

		// All validator reported broadcast failure - abort the broadcast.
		System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastAborted {
			broadcast_id,
		}));
	});
}

#[test]
fn test_transaction_signing_failed() {
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);
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
				SIG1,
				Default::default(),
				ETH_TX_FEE,
				MOCK_TX_METADATA,
				0
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
		let api_call = mock_api_call();
		initiate_and_sign_broadcast(&api_call, SIG1, TxType::Normal);

		let mut failed_authorities = vec![];
		let next_block = System::block_number() + 1;
		// The last node succeeds
		for _ in 0..MockEpochInfo::current_authority_count() - 1 {
			// Nominated signer responds that they can't sign the transaction.
			MockCfe::respond(Scenario::BroadcastFailure);
			failed_authorities.push(MockNominator::get_last_nominee().unwrap());

			Broadcaster::on_initialize(next_block);
		}

		witness_broadcast(SIG1);

		MockOffenceReporter::assert_reported(
			PalletOffence::FailedToBroadcastTransaction,
			failed_authorities,
		);
	});
}

#[test]
fn test_signature_request_expiry() {
	let check_end_state = |broadcast_id| {
		// old attempt has expired, but the data still exists
		assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).is_some());
		assert_eq!(TIMEDOUT_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()), broadcast_id,);
		// New attempt is live with same broadcast_id and incremented attempt_count.
		assert_eq!(Broadcaster::attempt_count(broadcast_id), 1);
	};
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);
			let expiry =
				BlockHeightProvider::<MockEthereum>::get_block_height() + BROADCAST_EXPIRY_BLOCKS;
			(broadcast_id, expiry)
		})
		.then_execute_at_next_block(|(broadcast_id, expiry)| {
			MockCfe::respond(Scenario::Timeout);
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);
			(broadcast_id, expiry)
		})
		.then_execute_with(|(broadcast_id, expiry)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(expiry);
			broadcast_id
		})
		.then_execute_at_next_block(|broadcast_id| {
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
	let check_end_state = |broadcast_id| {
		assert_eq!(TIMEDOUT_ATTEMPTS.with(|cell| *cell.borrow().first().unwrap()), broadcast_id,);
		// New attempt is live with same broadcast_id and incremented attempt_count.
		assert_eq!(Broadcaster::attempt_count(broadcast_id), 1);
	};
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);
			MockCfe::respond(Scenario::HappyPath);
			let expiry =
				BlockHeightProvider::<MockEthereum>::get_block_height() + BROADCAST_EXPIRY_BLOCKS;
			(broadcast_id, expiry)
		})
		.then_execute_with(|(broadcast_id, expiry)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(expiry);
			broadcast_id
		})
		.then_execute_at_next_block(|broadcast_id| {
			MockCfe::respond(Scenario::Timeout);
			check_end_state(broadcast_id);
			broadcast_id
		})
		.then_execute_with_keep_context(|_| {
			BlockHeightProvider::<MockEthereum>::increment_block_height()
		})
		.then_execute_at_next_block(|broadcast_id| {
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
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = initiate_and_sign_broadcast(&mock_api_call(), SIG1, TxType::Normal);

			assert_eq!(
				MockThresholdSigner::<MockEthereumChainCrypto, RuntimeCall>::signature_result(0),
				(Default::default(), AsyncResult::Void)
			);
			assert!(AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).is_some());
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);

			MockTransactionBuilder::<MockEthereum, RuntimeCall>::set_requires_refresh();
			let expiry =
				BlockHeightProvider::<MockEthereum>::get_block_height() + BROADCAST_EXPIRY_BLOCKS;
			(broadcast_id, expiry)
		})
		.then_execute_with_keep_context(|(_, expiry)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(*expiry)
		})
		.then_execute_at_next_block(|(broadcast_id, _)| {
			// The transaction has not yet succeeded. Therefore we should still have this tx_out_id
			// in storage.
			assert!(TransactionOutIdToBroadcastId::<Test, Instance1>::get(SIG1).is_some());
			// attempt count incremented for the same broadcast_id
			assert_eq!(Broadcaster::attempt_count(broadcast_id), 1);
		})
		.then_execute_at_next_block(|_| {
			// Verify that we have a new signature request in the pipeline
			assert_eq!(
				MockThresholdSigner::<MockEthereumChainCrypto, RuntimeCall>::signature_result(0),
				(Default::default(), AsyncResult::Pending)
			);
		});
}

#[test]
fn threshold_sign_and_broadcast_with_callback() {
	new_test_ext().execute_with(|| {
		let (broadcast_id, _) =
			Broadcaster::threshold_sign_and_broadcast(mock_api_call(), Some(MockCallback), |_| {
				None
			});

		MockThresholdSigner::<MockEthereumChainCrypto, RuntimeCall>::execute_signature_result_against_last_request(Ok(SIG1));

		assert_eq!(
			RequestSuccessCallbacks::<Test, Instance1>::get(broadcast_id),
			Some(MockCallback)
		);
		assert_ok!(Broadcaster::transaction_succeeded(
			RuntimeOrigin::root(),
			SIG1,
			Default::default(),
			ETH_TX_FEE,
			MOCK_TX_METADATA,
			2
		));
		assert!(RequestSuccessCallbacks::<Test, Instance1>::get(broadcast_id).is_none());
		let mut events = System::events();
		assert_eq!(
			events.pop().expect("an event").event,
			RuntimeEvent::Broadcaster(Event::BroadcastSuccess {
				broadcast_id,
				transaction_out_id: SIG1,
				transaction_ref: 2,
			})
		);
		assert_eq!(
			events.pop().expect("an event").event,
			RuntimeEvent::Broadcaster(Event::BroadcastCallbackExecuted {
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
			start_mock_broadcast(SIG1);
			let start_block_height = BlockHeightProvider::<MockEthereum>::get_block_height();
			assert!(get_timeouts_for(start_block_height + BROADCAST_EXPIRY_BLOCKS).len() == 1);
			start_block_height
		})
		.then_execute_at_next_block(|start_block_height| {
			BlockHeightProvider::<MockEthereum>::set_block_height(
				start_block_height + BROADCAST_EXPIRY_BLOCKS,
			);
			start_block_height
		})
		.then_process_next_block()
		.then_execute_with(|start_block_height| {
			assert!(get_timeouts_for(start_block_height + BROADCAST_EXPIRY_BLOCKS).is_empty());
			assert!(
				get_timeouts_for(
					start_block_height + BROADCAST_EXPIRY_BLOCKS + SAFEMODE_CHAINBLOCK_MARGIN
				)
				.len() == 1
			);
		});
}

#[test]
fn ensure_retries_are_skipped_during_safe_mode() {
	new_test_ext()
		.execute_with(|| {
			start_mock_broadcast(SIG1);
			MockCfe::respond(Scenario::BroadcastFailure);
			start_mock_broadcast(SIG1);
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

		initiate_and_sign_broadcast(&mock_api_call(), SIG1, TxType::Normal);

		assert_eq!(MockLiabilityTracker::total_liabilities(ForeignChain::Ethereum.gas_asset()), 0);

		witness_broadcast(SIG1);

		assert_eq!(
			System::events().get(1).expect("an event").event,
			RuntimeEvent::Broadcaster(Event::TransactionFeeDeficitRefused {
				beneficiary: Default::default(),
			})
		);
	});
}

#[test]
fn callback_is_called_upon_broadcast_failure() {
	new_test_ext().execute_with(|| {
		let (broadcast_id, _) =
			Broadcaster::threshold_sign_and_broadcast(mock_api_call(), None, |_| {
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
		PendingApiCalls::<Test, Instance1>::insert(broadcast_id, mock_api_call());

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
			let broadcast_id = start_mock_broadcast(SIG1);
			(MockNominator::get_last_nominee().unwrap(), broadcast_id, SIG1)
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
							transaction_ref: 0,
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
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);

			let awaiting_broadcast =
				AwaitingBroadcast::<Test, Instance1>::get(broadcast_id).unwrap();

			assert_eq!(Broadcaster::attempt_count(broadcast_id), 0);
			let nominee = MockNominator::get_last_nominee().unwrap();

			let expected_expiry_block =
				BlockHeightProvider::<MockEthereum>::get_block_height() + BROADCAST_EXPIRY_BLOCKS;

			assert_eq!(
				get_timeouts_for(expected_expiry_block).into_iter().next().unwrap().0,
				broadcast_id
			);

			// We want to run test test on the second attempt.
			MockCfe::respond(Scenario::BroadcastFailure);
			(nominee, awaiting_broadcast, expected_expiry_block)
		})
		// on idle runs and the retry is kicked off.
		.then_execute_at_next_block(|p| p)
		.then_execute_at_next_block(|(nominee, broadcast_data, expected_expiry_block)| {
			assert_eq!(Broadcaster::attempt_count(broadcast_data.broadcast_id), 1);
			MockTransactionBuilder::<MockEthereum, RuntimeCall>::set_requires_refresh();
			MockCfe::respond(Scenario::Timeout);
			(nominee, broadcast_data, expected_expiry_block)
		})
		// The broadcast times out
		.then_execute_with_keep_context(|(_, _, expected_expiry_block)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(*expected_expiry_block)
		})
		.then_process_next_block()
		.then_execute_at_next_block(|(nominee, broadcast_data, _)| {
			// Taking the invalid signature code path results in the metadata being removed, so the
			// check for the fee is ignored, however, the transaction_succeeded should still pass.
			assert_ok!(Broadcaster::transaction_succeeded(
				OriginTrait::root(),
				SIG1,
				nominee,
				ETH_TX_FEE,
				MOCK_TX_METADATA,
				0
			));

			assert_eq!(
				MockLiabilityTracker::total_liabilities(ForeignChain::Ethereum.gas_asset()),
				broadcast_data.transaction_payload.return_fee_refund(ETH_TX_FEE)
			);
		});
}

#[test]
fn broadcast_barrier_for_polkadot() {
	new_test_ext()
		.execute_with(|| {
			MockBroadcastBarriers::set(ChainChoice::Polkadot);

			// create and sign 3 txs that are then ready for broadcast
			let broadcast_id_1 =
				initiate_and_sign_broadcast(&mock_api_call(), SIG1, TxType::Normal);
			// tx1 emits broadcast request
			assert_transaction_broadcast_request_event(broadcast_id_1, SIG1);

			let broadcast_id_2 = initiate_and_sign_broadcast(
				&mock_api_call(),
				SIG2,
				TxType::Rotation { new_key: Default::default() },
			);
			// tx2 emits broadcast request and also pauses any further new broadcast requests
			assert_transaction_broadcast_request_event(broadcast_id_2, SIG2);

			let broadcast_id_3 =
				initiate_and_sign_broadcast(&mock_api_call(), SIG3, TxType::Normal);

			// tx3 is ready for broadcast but since there is a broadcast pause, broadcast request is
			// not issued, the broadcast is rescheduled instead.
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastRetryScheduled {
				broadcast_id: broadcast_id_3,
				retry_block: System::block_number() + 1,
			}));

			// report successful broadcast of tx1
			witness_broadcast(SIG1);

			// tx3 should still not be broadcasted because the blocking tx (tx2) has still not
			// succeeded.
			broadcast_id_3
		})
		.then_process_next_block()
		.then_execute_with(|broadcast_id_3| {
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id_3));

			// Now tx2 succeeds which should allow tx3 to be broadcast
			witness_broadcast(SIG2);
			broadcast_id_3
		})
		.then_process_next_block()
		.then_execute_with(|broadcast_id_3| {
			// attempt count is 1 because the previous failure to broadcast because of
			// broadcast pause is considered an attempt
			assert_transaction_broadcast_request_event(broadcast_id_3, SIG3);

			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block).is_empty());

			witness_broadcast(SIG3);
		});
}

#[test]
fn broadcast_barrier_for_bitcoin() {
	new_test_ext().execute_with(|| {
		MockBroadcastBarriers::set(ChainChoice::Bitcoin);

		// create and sign 3 txs that are then ready for broadcast
		let broadcast_id_1 = initiate_and_sign_broadcast(&mock_api_call(), SIG1, TxType::Normal);
		// tx1 emits broadcast request
		assert_transaction_broadcast_request_event(broadcast_id_1, SIG1);

		let broadcast_id_2 = initiate_and_sign_broadcast(
			&mock_api_call(),
			SIG2,
			TxType::Rotation { new_key: Default::default() },
		);
		// tx2 emits broadcast request and does not pause future broadcasts in bitcoin
		assert_transaction_broadcast_request_event(broadcast_id_2, SIG2);

		let broadcast_id_3 = initiate_and_sign_broadcast(&mock_api_call(), SIG3, TxType::Normal);
		// tx3 emits broadcast request
		assert_transaction_broadcast_request_event(broadcast_id_3, SIG3);

		// we successfully witness all txs
		witness_broadcast(SIG1);
		witness_broadcast(SIG2);
		witness_broadcast(SIG3);
	});
}

#[test]
fn broadcast_barrier_for_ethereum() {
	new_test_ext()
		.execute_with(|| {
			MockBroadcastBarriers::set(ChainChoice::Ethereum);

			let broadcast_id_1 =
				initiate_and_sign_broadcast(&mock_api_call(), SIG1, TxType::Normal);
			// tx1 emits broadcast request
			assert_transaction_broadcast_request_event(broadcast_id_1, SIG1);

			let broadcast_id_2 =
				initiate_and_sign_broadcast(&mock_api_call(), SIG2, TxType::Normal);
			// tx2 emits broadcast request
			assert_transaction_broadcast_request_event(broadcast_id_2, SIG2);

			// this will put a broadcast barrier at tx2 and tx3. tx3 wont be broadcasted yet
			let broadcast_id_3 = initiate_and_sign_broadcast(
				&mock_api_call(),
				SIG3,
				TxType::Rotation { new_key: Default::default() },
			);

			// tx3 is ready for broadcast but since there is a broadcast pause, broadcast request is
			// not issued, the broadcast is rescheduled instead.
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastRetryScheduled {
				broadcast_id: broadcast_id_3,
				retry_block: System::block_number() + 1,
			}));

			// tx4 will be created but not broadcasted yet
			let broadcast_id_4 =
				initiate_and_sign_broadcast(&mock_api_call(), SIG4, TxType::Normal);
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastRetryScheduled {
				broadcast_id: broadcast_id_4,
				retry_block: System::block_number() + 1,
			}));

			// report successful broadcast of tx2
			witness_broadcast(SIG2);

			// tx3 and tx4 should still not be broadcasted because not all txs before and including
			// tx2 have been witnessed
			(broadcast_id_3, broadcast_id_4)
		})
		.then_process_next_block()
		.then_execute_with(|(broadcast_id_3, broadcast_id_4)| {
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id_3));
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id_4));

			// Now tx1 succeeds which should allow tx3 to be broadcast but not tx4 since there
			// will be another barrier at tx3
			witness_broadcast(SIG1);
			(broadcast_id_3, broadcast_id_4)
		})
		.then_process_next_block()
		.then_execute_with(|(broadcast_id_3, broadcast_id_4)| {
			// attempt count is 1 because the previous failure to broadcast because of
			// broadcast pause is considered an attempt
			assert_transaction_broadcast_request_event(broadcast_id_3, SIG3);

			// tx4 is still pending
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id_4));

			// witness tx3 which should allow tx4 to be broadcast
			witness_broadcast(SIG3);
			broadcast_id_4
		})
		.then_process_next_block()
		.then_execute_with(|broadcast_id_4| {
			assert_transaction_broadcast_request_event(broadcast_id_4, SIG4);
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block).is_empty());
			witness_broadcast(SIG4);
		});
}

fn assert_transaction_broadcast_request_event(
	broadcast_id: BroadcastId,
	tx_out_id: <MockEthereumChainCrypto as ChainCrypto>::TransactionOutId,
) {
	System::assert_last_event(RuntimeEvent::Broadcaster(Event::TransactionBroadcastRequest {
		transaction_out_id: tx_out_id,
		broadcast_id,
		transaction_payload: Default::default(),
		nominee: MockNominator::get_last_nominee().unwrap(),
	}));
}

fn initiate_and_sign_broadcast(
	api_call: &MockApiCall<MockEthereumChainCrypto>,
	mock_sig: <MockEthereumChainCrypto as ChainCrypto>::ThresholdSignature,
	tx_type: TxType,
) -> BroadcastId {
	let (broadcast_id, _) = match tx_type {
		TxType::Normal => <Broadcaster as BroadcasterTrait<
			<Test as Config<Instance1>>::TargetChain,
		>>::threshold_sign_and_broadcast((*api_call).clone()),
		TxType::Rotation { new_key } => <Broadcaster as BroadcasterTrait<
			<Test as Config<Instance1>>::TargetChain,
		>>::threshold_sign_and_broadcast_rotation_tx(
			(*api_call).clone(), new_key
		),
	};

	MockThresholdSigner::<MockEthereumChainCrypto, RuntimeCall>::execute_signature_result_against_last_request(Ok(mock_sig));

	broadcast_id
}

fn witness_broadcast(tx_out_id: <MockEthereumChainCrypto as ChainCrypto>::TransactionOutId) {
	assert_ok!(Broadcaster::transaction_succeeded(
		RuntimeOrigin::root(),
		tx_out_id,
		Default::default(),
		ETH_TX_FEE,
		MOCK_TX_METADATA,
		0,
	));
}

#[test]
fn timed_out_broadcasters_are_reported() {
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);
			let expiry = BlockHeightProvider::<MockEthereum>::get_block_height()
				.saturating_add(crate::BroadcastTimeout::<Test, Instance1>::get());
			let nominee = AwaitingBroadcast::<Test, Instance1>::get(broadcast_id)
				.unwrap()
				.nominee
				.unwrap();

			assert!(FailedBroadcasters::<Test, Instance1>::get(broadcast_id).is_empty());
			(broadcast_id, nominee, expiry)
		})
		.then_execute_with_keep_context(|(_, _, expiry)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(*expiry)
		})
		.then_execute_at_next_block(|(broadcast_id, nominee, _)| {
			// The nominated broadcaster is added to `FailedBroadcasters` to be reported later.
			assert_eq!(
				FailedBroadcasters::<Test, Instance1>::get(broadcast_id),
				BTreeSet::from([nominee])
			);
		});
}

#[test]
fn broadcast_can_be_aborted_due_to_timeout() {
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);
			let expiry = BlockHeightProvider::<MockEthereum>::get_block_height()
				.saturating_add(crate::BroadcastTimeout::<Test, Instance1>::get());
			ready_to_abort_broadcast(broadcast_id);

			(broadcast_id, expiry)
		})
		.then_execute_at_next_block(|(broadcast_id, expiry)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(expiry);
			broadcast_id
		})
		.then_process_next_block()
		.then_execute_with(|broadcast_id| {
			// Broadcast should be aborted
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastAborted {
				broadcast_id,
			}));
			assert!(AbortedBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));
			assert!(
				FailedBroadcasters::<Test, Instance1>::decode_non_dedup_len(broadcast_id).is_none()
			);
		});
}

#[test]
fn broadcast_timeout_works_when_external_chain_advances_multiple_blocks() {
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);
			ready_to_abort_broadcast(broadcast_id);

			let expiry1 = BlockHeightProvider::<MockEthereum>::get_block_height()
				.saturating_add(crate::BroadcastTimeout::<Test, Instance1>::get());

			(vec![broadcast_id], expiry1)
		})
		// add start a second broadcast during the next external block
		.then_execute_with_keep_context(|_| {
			BlockHeightProvider::<MockEthereum>::increment_block_height()
		})
		.then_execute_with(|(mut broadcast_ids, expiry1)| {
			let broadcast_id = start_mock_broadcast(SIG1);
			broadcast_ids.push(broadcast_id);
			ready_to_abort_broadcast(broadcast_id);
			let expiry2 = BlockHeightProvider::<MockEthereum>::get_block_height()
				.saturating_add(crate::BroadcastTimeout::<Test, Instance1>::get());
			(broadcast_ids, max(expiry1, expiry2))
		})
		.then_process_next_block()
		// move external block height into the future past both timeouts
		.then_execute_with_keep_context(|(_, expiry)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(expiry + 1)
		})
		.then_process_next_block()
		.then_execute_with(|(broadcast_ids, _)| {
			// All broadcast should be aborted
			for broadcast_id in broadcast_ids {
				System::assert_has_event(RuntimeEvent::Broadcaster(Event::BroadcastAborted {
					broadcast_id,
				}));
				assert!(AbortedBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));
				assert!(FailedBroadcasters::<Test, Instance1>::decode_non_dedup_len(broadcast_id)
					.is_none());
			}
		});
}

#[test]
fn aborted_broadcasts_can_still_succeed() {
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);
			let expiry = BlockHeightProvider::<MockEthereum>::get_block_height()
				.saturating_add(crate::BroadcastTimeout::<Test, Instance1>::get());
			ready_to_abort_broadcast(broadcast_id);

			(broadcast_id, SIG1, expiry)
		})
		.then_execute_with_keep_context(|(_, _, expiry)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(*expiry)
		})
		.then_execute_at_next_block(|ctx| ctx)
		.then_execute_with(|(broadcast_id, transaction_out_id, _)| {
			// Broadcast should be aborted
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastAborted {
				broadcast_id,
			}));
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
				2
			));

			// Storage should be cleaned, event emitted.
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastSuccess {
				broadcast_id,
				transaction_out_id,
				transaction_ref: 2,
			}));
			assert_broadcast_storage_cleaned_up(broadcast_id);
		});
}

#[test]
fn broadcast_retry_delay_works() {
	let mut target = 0;
	let delay = 10;
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);

			BroadcastDelay::set(None);
			// With no delay, retries are added to the normal queue, and is retried in the next
			// block.
			let next_block = System::block_number() + 1;
			assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(0u64), broadcast_id));
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id));
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastRetryScheduled {
				broadcast_id,
				retry_block: next_block,
			}));
			broadcast_id
		})
		.then_process_next_block()
		.then_execute_with(|broadcast_id| {
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
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastRetryScheduled {
				broadcast_id,
				retry_block: target,
			}));
			broadcast_id
		})
		.then_process_blocks_until_block(target)
		.then_execute_with(|broadcast_id| {
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::decode_non_dedup_len(
				next_block
			)
			.is_none());
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::decode_non_dedup_len(target)
				.is_none());

			assert_transaction_broadcast_request_event(broadcast_id, SIG1);
		});
}

#[test]
fn broadcast_timeout_delay_works() {
	let mut target = 0;
	let mut external_target = 0;
	let delay = 10;
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);

			BroadcastDelay::set(Some(delay));
			target = System::block_number() + BROADCAST_EXPIRY_BLOCKS;
			external_target =
				BlockHeightProvider::<MockEthereum>::get_block_height() + BROADCAST_EXPIRY_BLOCKS;
			broadcast_id
		})
		.then_process_blocks_until_block(target)
		.then_execute_with_keep_context(|_| {
			BlockHeightProvider::<MockEthereum>::set_block_height(external_target);
		})
		.then_process_next_block()
		.then_execute_with(|broadcast_id| {
			target = System::block_number() + delay;

			assert!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::get(target).contains(&broadcast_id),
			);
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastRetryScheduled {
				broadcast_id,
				retry_block: target,
			}));
		});
}

#[test]
fn aborted_broadcasts_will_not_retry() {
	let mut target = 0;
	let delay = 100;
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);
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
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastAborted {
				broadcast_id,
			}));
			broadcast_id
		})
		.then_process_blocks_until_block(target)
		.then_execute_with(|broadcast_id| {
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
			let broadcast_id = start_mock_broadcast(SIG1);
			BroadcastDelay::set(Some(delay));
			target = System::block_number() + delay;
			assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(0u64), broadcast_id));
			assert!(
				DelayedBroadcastRetryQueue::<Test, Instance1>::get(target).contains(&broadcast_id)
			);

			// Report broadcast as Succeeded
			assert_ok!(Broadcaster::transaction_succeeded(
				RuntimeOrigin::root(),
				SIG1,
				Default::default(),
				ETH_TX_FEE,
				MOCK_TX_METADATA,
				3
			));
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastSuccess {
				broadcast_id,
				transaction_out_id: SIG1,
				transaction_ref: 3,
			}));
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
	let mut target_chainblock: u64 = 0u64;
	let mut target_block: u64 = 0u64;
	new_test_ext()
		.then_execute_at_block(1_000u64, |_| {
			let broadcast_id = start_mock_broadcast(SIG1);
			BroadcastDelay::set(Some(1));
			assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(0u64), broadcast_id));

			// On safe mode next block, storage will be re-added to this target block.
			let next_block = System::block_number() + 1u64;
			target_block = next_block +
				<<Test as crate::Config<Instance1>>::SafeModeBlockMargin as Get<u64>>::get();
			let current_chainblock = BlockHeightProvider::<MockEthereum>::get_block_height();
			target_chainblock = current_chainblock +
				<<Test as crate::Config<Instance1>>::SafeModeChainBlockMargin as Get<u64>>::get();

			// Ensure next block's data is ready to be re-scheduled during safe mode.
			append_timeouts_for(
				current_chainblock,
				vec![(100, 0), (101, 0), (102, 0), (105, 0), (106, 0)],
			);
			append_timeouts_for(
				current_chainblock,
				vec![(100, 0), (101, 0), (102, 0), (105, 0), (106, 0)],
			);
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id,));

			// add mock data to the target block storage.
			append_timeouts_for(
				target_chainblock,
				vec![(100, 0), (101, 0), (102, 0), (103, 0), (104, 0)],
			);
			DelayedBroadcastRetryQueue::<Test, Instance1>::append(target_block, 100);
			DelayedBroadcastRetryQueue::<Test, Instance1>::append(target_block, 101);

			// Activate safe mode code red.
			<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
			broadcast_id
		})
		.then_process_next_block()
		.then_execute_with(|broadcast_id| {
			// Hook should re-schedule the `Timeouts` and Broadcast retries.
			// Entries should be appended to the target block's storage, not replace it.
			assert_eq!(
				get_timeouts_for(target_chainblock),
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
				DelayedBroadcastRetryQueue::<Test, Instance1>::get(target_block),
				BTreeSet::from_iter([100, 101, broadcast_id])
			);
		});
}

#[test]
fn broadcast_is_retried_without_initial_nominee() {
	new_test_ext()
		.then_execute_at_block(1_000u64, |_| {
			// Configure so no nominee can be selected for the very first time.
			MockNominator::set_nominees(Some(Default::default()));

			let broadcast_id = start_mock_broadcast(SIG1);

			// Broadcast should be retried next block
			let next_block = System::block_number() + 1;
			assert!(DelayedBroadcastRetryQueue::<Test, Instance1>::get(next_block)
				.contains(&broadcast_id));

			// Make nominees available
			MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
			broadcast_id
		})
		.then_process_next_block()
		.then_execute_with(|broadcast_id| {
			// Broadcast can now succeed.
			assert_ok!(Broadcaster::transaction_succeeded(
				RuntimeOrigin::root(),
				SIG1,
				Default::default(),
				ETH_TX_FEE,
				MOCK_TX_METADATA,
				5
			));

			// Storage should be cleaned, event emitted.
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastSuccess {
				broadcast_id,
				transaction_out_id: SIG1,
				transaction_ref: 5,
			}));
			assert_broadcast_storage_cleaned_up(broadcast_id);
		});
}

#[test]
fn broadcast_re_signing() {
	new_test_ext()
		.execute_with(|| {
			let broadcast_id = start_mock_broadcast(SIG1);

			// Abort the broadcast
			let nominee = ready_to_abort_broadcast(broadcast_id);
			assert_ok!(Broadcaster::transaction_failed(
				RuntimeOrigin::signed(nominee),
				broadcast_id
			));
			System::assert_last_event(RuntimeEvent::Broadcaster(Event::BroadcastAborted {
				broadcast_id,
			}));

			assert_eq!(TransactionOutIdToBroadcastId::<Test, Instance1>::iter().count(), 1);
			assert_eq!(
				TransactionOutIdToBroadcastId::<Test, Instance1>::get(SIG1).unwrap().0,
				broadcast_id
			);

			broadcast_id
		})
		.then_execute_at_next_block(|broadcast_id| {
			// Check that the broadcast is aborted
			assert!(!PendingBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));
			assert!(AbortedBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));

			// Make sure that only governance can request a re-sign
			assert_noop!(
				crate::Pallet::<Test, Instance1>::re_sign_aborted_broadcasts(
					RuntimeOrigin::signed(100),
					vec![broadcast_id],
					true,
					false,
				),
				sp_runtime::traits::BadOrigin
			);

			// Request a re-sign
			assert_ok!(crate::Pallet::<Test, Instance1>::re_sign_aborted_broadcasts(
				RuntimeOrigin::root(),
				vec![broadcast_id],
				true,
				false,
			));

			// Check that the broadcast is re-scheduled
			assert!(PendingBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));
			assert!(!AbortedBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));
			broadcast_id
		})
		.then_execute_at_next_block(|broadcast_id| {
			// Signed, creating another TransactionOutId for the same broadcast id.
			MockThresholdSigner::<MockEthereumChainCrypto, RuntimeCall>::execute_signature_result_against_last_request(Ok(SIG2));
			broadcast_id
		})
		.then_execute_at_next_block(|broadcast_id| {
			assert_eq!(TransactionOutIdToBroadcastId::<Test, Instance1>::iter().count(), 2);
			assert!(PendingBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));
			assert!(!AbortedBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));

			// Succeed the second, resigned transaction out id.
			assert_ok!(Broadcaster::transaction_succeeded(
				RuntimeOrigin::root(),
				SIG2,
				Default::default(),
				ETH_TX_FEE,
				MOCK_TX_METADATA,
				Default::default(),
			));

			// All transactinon out ids should be cleaned up for this broadcast id.
			assert!(!TransactionOutIdToBroadcastId::<Test, Instance1>::iter().any(|(_, (b_id, _))| b_id == broadcast_id));
		});
}

#[test]
fn threshold_sign_and_refresh_replay_protection() {
	new_test_ext().execute_with(|| {
		MockTransactionBuilder::<MockEthereum, RuntimeCall>::set_refreshed_replay_protection();
		let broadcast_id: u8 = 1;

		PendingApiCalls::<Test, Instance1>::insert(
			broadcast_id as u32,
			mock_api_call(),
		);

		TransactionOutIdToBroadcastId::<Test, Instance1>::insert(
			SIG1,
			(broadcast_id as u32, 0),
		);

		assert_ok!(Broadcaster::re_sign_aborted_broadcasts(
			RuntimeOrigin::root(),
			vec![broadcast_id as u32],
			false,
			true,
		));

		assert!(MockTransactionBuilder::<MockEthereum, RuntimeCall>::get_refreshed_replay_protection_state(), "Refreshed replay protection has not been refreshed!");
	});
}

#[test]
fn should_release_barriers_correctly_in_case_of_rotation_tx_succeeding_first() {
	new_test_ext().execute_with(|| {
		// create a rotation tx and 1 tx before
		let broadcast_id_1 = initiate_and_sign_broadcast(&mock_api_call(), SIG1, TxType::Normal);

		let broadcast_id_2 = initiate_and_sign_broadcast(
			&mock_api_call(),
			SIG2,
			TxType::Rotation { new_key: Default::default() },
		);

		// the rotation tx should create barriers on both txs
		let expected_barriers: BTreeSet<_> = [broadcast_id_1, broadcast_id_2].into_iter().collect();
		assert_eq!(BroadcastBarriers::<Test, Instance1>::get(), expected_barriers);

		// succeed the rotation tx first
		assert_ok!(Broadcaster::transaction_succeeded(
			RuntimeOrigin::root(),
			SIG2,
			Default::default(),
			ETH_TX_FEE,
			MOCK_TX_METADATA,
			Default::default(),
		));

		// This should not release barriers since the tx before rotation tx is pending
		assert_eq!(BroadcastBarriers::<Test, Instance1>::get(), expected_barriers);

		// succeeding the first tx will release both barriers
		assert_ok!(Broadcaster::transaction_succeeded(
			RuntimeOrigin::root(),
			SIG1,
			Default::default(),
			ETH_TX_FEE,
			MOCK_TX_METADATA,
			Default::default(),
		));

		assert_eq!(BroadcastBarriers::<Test, Instance1>::get(), BTreeSet::new());
	});
}

#[test]
fn only_governance_can_stress_test() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Broadcaster::stress_test(RuntimeOrigin::signed(100), 1),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn changing_broadcast_timeout() {
	new_test_ext().execute_with(|| {
		// ensure that timeout is the default value
		assert_eq!(crate::BroadcastTimeout::<Test, _>::get(), crate::mock::BROADCAST_EXPIRY_BLOCKS);

		// new timeout value, ensure that it's different from default
		const NEW_TIMEOUT: u32 = 250;
		assert_ne!(crate::mock::BROADCAST_EXPIRY_BLOCKS, NEW_TIMEOUT as u64);

		// update the timeout
		const UPDATE: PalletConfigUpdate =
			PalletConfigUpdate::BroadcastTimeout { blocks: NEW_TIMEOUT };
		assert_ok!(Broadcaster::update_pallet_config(RuntimeOrigin::root(), UPDATE));

		// check that value was set
		assert_eq!(crate::BroadcastTimeout::<Test, _>::get(), u64::from(NEW_TIMEOUT));

		// check that update event was emitted
		assert_eq!(
			last_event::<Test>(),
			RuntimeEvent::Broadcaster(Event::PalletConfigUpdated { update: UPDATE }),
		);
	});
}

#[test]
fn aborted_broadcast_is_cleaned_up_on_success() {
	new_test_ext().execute_with(|| {
		// Abort a broadcast
		let broadcast_id = start_mock_broadcast(SIG1);
		let nominee = ready_to_abort_broadcast(broadcast_id);
		assert_ok!(Broadcaster::transaction_failed(RuntimeOrigin::signed(nominee), broadcast_id));
		assert!(AbortedBroadcasts::<Test, Instance1>::get().contains(&broadcast_id));

		// Witness a successful broadcast as if it was manually broadcast
		assert_ok!(Broadcaster::transaction_succeeded(
			RuntimeOrigin::root(),
			SIG1,
			Default::default(),
			ETH_TX_FEE,
			MOCK_TX_METADATA,
			Default::default(),
		));

		// Storage should be cleaned, event emitted.
		assert!(AbortedBroadcasts::<Test, Instance1>::get().is_empty());
	});
}
