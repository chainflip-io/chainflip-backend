use super::*;

const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;

fn fok_swap(refund_params: Option<TestRefundParams>) -> TestSwapParams {
	TestSwapParams::new(None, refund_params, false)
}

fn fok_swap_ccm(refund_params: Option<TestRefundParams>) -> TestSwapParams {
	TestSwapParams::new(None, refund_params, true)
}

#[track_caller]
fn assert_swaps_scheduled_for_block(swap_ids: &[SwapId], expected_block_number: u64) {
	for expected_swap_id in swap_ids {
		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::SwapScheduled { swap_id, execute_at: block_number, .. }) if swap_id == expected_swap_id && block_number == &expected_block_number,
		);
	}
}

#[test]
fn both_fok_and_regular_swaps_succeed_first_try() {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const REGULAR_SWAP_ID: u64 = 1;
	const FOK_SWAP_ID: u64 = 2;

	const REGULAR_REQUEST_ID: u64 = 1;
	const FOK_REQUEST_ID: u64 = 2;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			const REFUND_PARAMS: TestRefundParams = TestRefundParams {
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
				min_output: (INPUT_AMOUNT - BROKER_FEE) * DEFAULT_SWAP_RATE,
			};

			let refund_parameters_encoded =
				REFUND_PARAMS.into_channel_params(INPUT_AMOUNT).map_address(|refund_address| {
					MockAddressConverter::to_encoded_address(refund_address)
				});

			insert_swaps(&vec![fok_swap(None), fok_swap(Some(REFUND_PARAMS))]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: FOK_REQUEST_ID,
					refund_parameters,
					..
				}) if refund_parameters.as_ref() == Some(&refund_parameters_encoded),
			);

			assert_swaps_scheduled_for_block(
				&[REGULAR_SWAP_ID, FOK_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_process_blocks_until_block(SWAPS_SCHEDULED_FOR_BLOCK)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: REGULAR_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: REGULAR_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: REGULAR_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: FOK_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_REQUEST_ID
				}),
			);
		});
}

#[test]
fn price_limit_is_respected_in_fok_swap() {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 =
		SWAPS_SCHEDULED_FOR_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;

	const EXPECTED_OUTPUT: AssetAmount = (INPUT_AMOUNT - BROKER_FEE) * DEFAULT_SWAP_RATE;
	const HIGH_OUTPUT: AssetAmount = EXPECTED_OUTPUT + 1;

	const REGULAR_SWAP_ID: u64 = 1;
	const FOK_SWAP_1_ID: u64 = 2;
	const FOK_SWAP_2_ID: u64 = 3;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&vec![
				fok_swap(None),
				fok_swap(Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: HIGH_OUTPUT,
				})),
				fok_swap(Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: EXPECTED_OUTPUT,
				})),
			]);

			assert_swaps_scheduled_for_block(
				&[REGULAR_SWAP_ID, FOK_SWAP_1_ID, FOK_SWAP_2_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_process_blocks_until_block(3u64)
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), SWAPS_SCHEDULED_FOR_BLOCK);
			// Swap 2 should fail due to price limit and rescheduled for block
			// `SWAPS_SCHEDULED_FOR_BLOCK + SWAP_RETRY_DELAY_BLOCKS`, but swaps 1 and 3 should be
			// successful:
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_1_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: REGULAR_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 1 }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_2_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 3 }),
			);

			assert_eq!(SwapQueue::<Test>::get(SWAP_RETRIED_AT_BLOCK).len(), 1);
		})
		.then_execute_at_block(SWAP_RETRIED_AT_BLOCK, |_| {
			// Changing the swap rate to allow the FoK swap to be executed
			SwapRate::set(HIGH_OUTPUT as f64 / (INPUT_AMOUNT - BROKER_FEE) as f64);
		})
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_1_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 2 }),
			);

			assert_eq!(SwapQueue::<Test>::get(SWAP_RETRIED_AT_BLOCK).len(), 0);
		});
}

#[test]
fn fok_swap_gets_refunded_due_to_price_limit() {
	const FOK_SWAP_REQUEST_ID: u64 = 1;
	const OTHER_SWAP_REQUEST_ID: u64 = 2;

	const FOK_SWAP_ID: u64 = 1;
	const OTHER_SWAP_ID: u64 = 2;

	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 =
		SWAPS_SCHEDULED_FOR_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			// Min output for swap 1 is too high to be executed:
			const MIN_OUTPUT: AssetAmount = (INPUT_AMOUNT - BROKER_FEE) * DEFAULT_SWAP_RATE + 1;
			insert_swaps(&[fok_swap(Some(TestRefundParams {
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
				min_output: MIN_OUTPUT,
			}))]);
			// However, swap 2 is non-FoK and should still be executed:
			insert_swaps(&[fok_swap(None)]);

			assert_swaps_scheduled_for_block(
				&[FOK_SWAP_ID, OTHER_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_process_blocks_until_block(SWAPS_SCHEDULED_FOR_BLOCK)
		.then_execute_with(|_| {
			// Swap 1 should fail here and rescheduled for a later block,
			// but swap 2 (without FoK parameters) should still be successful:
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: OTHER_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: OTHER_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: OTHER_SWAP_REQUEST_ID
				}),
			);
		})
		.then_process_blocks_until_block(SWAP_RETRIED_AT_BLOCK)
		.then_execute_with(|_| {
			// Swap request should be removed in case of refund
			assert_eq!(SwapRequests::<Test>::get(FOK_SWAP_REQUEST_ID), None);
			// Swap should fail here (due to price limit) and be refunded due
			// to reaching expiry block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: FOK_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_SWAP_REQUEST_ID
				}),
			);
		});
}

#[test]
fn fok_swap_gets_refunded_due_to_price_impact_protection() {
	const FOK_SWAP_REQUEST_ID: u64 = 1;
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 =
		SWAPS_SCHEDULED_FOR_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	const FOK_SWAP_ID: u64 = 1;
	const REGULAR_SWAP_ID: u64 = 2;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			// FoK swap 1 should fail and will eventually be refunded
			insert_swaps(&[fok_swap(Some(TestRefundParams {
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
				min_output: INPUT_AMOUNT,
			}))]);

			// Non-FoK swap 2 will fail together with swap 1, but should be retried indefinitely
			insert_swaps(&[fok_swap(None)]);

			assert_swaps_scheduled_for_block(
				&[FOK_SWAP_ID, REGULAR_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {
			// This simulates not having enough liquidity/triggering price impact protection
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_execute_with(|_| {
			// Both swaps should fail here and be rescheduled for a later block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
				}),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: REGULAR_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
				})
			);
		})
		.then_process_blocks_until_block(SWAP_RETRIED_AT_BLOCK)
		.then_execute_with(|_| {
			// Swap request should be removed in case of refund
			assert_eq!(SwapRequests::<Test>::get(FOK_SWAP_REQUEST_ID), None);
			// Swap should fail here (due to price impact protection) and be refunded due
			// to reaching expiry block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: FOK_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_SWAP_REQUEST_ID
				}),
				// Non-fok swap will continue to be retried:
				RuntimeEvent::Swapping(Event::SwapRescheduled { swap_id: REGULAR_SWAP_ID, .. }),
			);
		});
}

#[test]
fn fok_test_zero_refund_duration() {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	new_test_ext()
		.execute_with(|| {
			// A swap with 0 retry duration should be tried exactly 1 time
			insert_swaps(&[fok_swap(Some(TestRefundParams {
				retry_duration: 0,
				min_output: INPUT_AMOUNT,
			}))]);

			assert_swaps_scheduled_for_block(&[1], SWAPS_SCHEDULED_FOR_BLOCK);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {
			// This simulates not having enough liquidity/triggering price impact protection
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_execute_with(|_| {
			// The swap should fail and be refunded immediately instead of being retried
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled { swap_request_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 1, .. }),
			);
		});
}

#[test]
fn fok_ccm_happy_path() {
	const PRINCIPAL_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const GAS_BLOCK: u64 = PRINCIPAL_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const REQUEST_ID: u64 = 1;
	const PRINCIPAL_SWAP_ID: u64 = 1;
	const GAS_SWAP_ID: u64 = 2;

	const EXPECTED_OUTPUT: AssetAmount = (INPUT_AMOUNT - BROKER_FEE) * DEFAULT_SWAP_RATE;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[fok_swap_ccm(Some(TestRefundParams {
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
				min_output: EXPECTED_OUTPUT,
			}))]);

			assert_swaps_scheduled_for_block(&[PRINCIPAL_SWAP_ID], PRINCIPAL_BLOCK);
		})
		.then_process_blocks_until_block(PRINCIPAL_BLOCK)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: PRINCIPAL_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: GAS_SWAP_ID,
					swap_type: SwapType::CcmGas,
					..
				}),
			);
		})
		.then_process_blocks_until_block(GAS_BLOCK)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: GAS_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: REQUEST_ID }),
			);
		});
}

#[test]
fn fok_ccm_refunded() {
	const PRINCIPAL_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const REQUEST_ID: u64 = 1;
	const PRINCIPAL_SWAP_ID: u64 = 1;

	const PRINCIPAL_AMOUNT: AssetAmount = INPUT_AMOUNT - GAS_BUDGET;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[fok_swap_ccm(Some(TestRefundParams {
				retry_duration: 0,
				min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE + 1,
			}))]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: PRINCIPAL_SWAP_ID,
					swap_type: SwapType::CcmPrincipal,
					input_amount: PRINCIPAL_AMOUNT,
					..
				}),
			);

			assert_swaps_scheduled_for_block(&[PRINCIPAL_SWAP_ID], PRINCIPAL_BLOCK);
		})
		.then_process_blocks_until_block(PRINCIPAL_BLOCK)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: REQUEST_ID,
					// Note that gas is refunded too:
					amount: INPUT_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: REQUEST_ID }),
			);
		});
}

#[test]
fn fok_ccm_refunded_no_gas_swap() {
	const PRINCIPAL_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const REQUEST_ID: u64 = 1;
	const PRINCIPAL_SWAP_ID: u64 = 1;

	const PRINCIPAL_AMOUNT: AssetAmount = INPUT_AMOUNT - GAS_BUDGET;

	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Usdc;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			let refund_params = TestRefundParams {
				retry_duration: 0,
				min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE + 1,
			}
			.into_channel_params(INPUT_AMOUNT);

			insert_swaps(&[TestSwapParams {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				refund_params: Some(refund_params),
				dca_params: None,
				output_address: (*EVM_OUTPUT_ADDRESS).clone(),
				is_ccm: true,
			}]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: PRINCIPAL_SWAP_ID,
					swap_type: SwapType::CcmPrincipal,
					input_amount: PRINCIPAL_AMOUNT,
					..
				}),
			);

			assert_swaps_scheduled_for_block(&[PRINCIPAL_SWAP_ID], PRINCIPAL_BLOCK);
		})
		.then_process_blocks_until_block(PRINCIPAL_BLOCK)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: REQUEST_ID,
					// Note that gas is refunded too:
					amount: INPUT_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: REQUEST_ID }),
			);
		});
}
