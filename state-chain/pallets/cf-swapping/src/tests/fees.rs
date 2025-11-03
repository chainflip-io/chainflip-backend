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

use super::*;

#[test]
fn swap_output_amounts_correctly_account_for_fees() {
	for (from, to) in
		// non-stable to non-stable, non-stable to stable, stable to non-stable
		[(Asset::Btc, Asset::Eth), (Asset::Btc, Asset::Usdc), (Asset::Usdc, Asset::Eth)]
	{
		new_test_ext().execute_with(|| {
			const INPUT_AMOUNT: AssetAmount = 1000;

			let network_fee = Permill::from_percent(1);
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: network_fee, minimum: 0 });

			let expected_output: AssetAmount = {
				let usdc_amount = if from == Asset::Usdc {
					INPUT_AMOUNT
				} else {
					INPUT_AMOUNT * DEFAULT_SWAP_RATE
				};

				let usdc_after_network_fees = usdc_amount - network_fee * usdc_amount;

				if to == Asset::Usdc {
					usdc_after_network_fees
				} else {
					usdc_after_network_fees * DEFAULT_SWAP_RATE
				}
			};

			{
				Swapping::init_swap_request(
					from,
					INPUT_AMOUNT,
					to,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::Egress {
							ccm_deposit_metadata: None,
							output_address: ForeignChainAddress::Eth(H160::zero()),
						},
					},
					Default::default(),
					None,
					None,
					SwapOrigin::Vault {
						tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
						broker_id: Some(BROKER),
					},
				);

				Swapping::on_finalize(System::block_number() + SWAP_DELAY_BLOCKS as u64);

				assert_eq!(
					MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
					vec![MockEgressParameter::Swap {
						asset: to,
						amount: expected_output,
						fee: 0,
						destination_address: ForeignChainAddress::Eth(H160::zero()),
					},]
				);
			}
		});
	}
}

#[test]
fn test_buy_back_flip() {
	new_test_ext().execute_with(|| {
		const INTERVAL: BlockNumberFor<Test> = 5;
		const NETWORK_FEE_AMOUNT: AssetAmount = 100;

		// Get some network fees, just like we did a swap.
		CollectedNetworkFee::<Test>::set(NETWORK_FEE_AMOUNT);

		// The default buy interval is zero. Check that buy back is disabled & on_initialize does
		// not panic.
		assert_eq!(FlipBuyInterval::<Test>::get(), 0);
		Swapping::on_initialize(1);
		assert_eq!(NETWORK_FEE_AMOUNT, CollectedNetworkFee::<Test>::get());

		// Set a non-zero buy interval
		FlipBuyInterval::<Test>::set(INTERVAL);

		// Nothing is bought if we're not at the interval.
		Swapping::on_initialize(INTERVAL * 3 - 1);
		assert_eq!(NETWORK_FEE_AMOUNT, CollectedNetworkFee::<Test>::get());

		// If we're at an interval, we should buy flip.
		Swapping::on_initialize(INTERVAL * 3);
		assert_eq!(0, CollectedNetworkFee::<Test>::get());

		// Note that the network fee will not be charged in this case:
		assert_eq!(
			ScheduledSwaps::<Test>::get()
				.get(&1.into())
				.expect("Should have scheduled a swap usdc -> flip"),
			&Swap::new(
				1.into(),
				1.into(),
				STABLE_ASSET,
				Asset::Flip,
				NETWORK_FEE_AMOUNT,
				None,
				[],
				System::block_number() + SWAP_DELAY_BLOCKS as u64
			)
		);
	});
}

#[test]
fn normal_swap_uses_correct_network_fee() {
	const AMOUNT: AssetAmount = 10000;
	const SMALL_AMOUNT: AssetAmount = 500;
	const NETWORK_FEE: Permill = Permill::from_percent(10);
	const MINIMUM_NETWORK_FEE: AssetAmount = 100;

	new_test_ext()
		.execute_with(|| {
			// Set both network fees to different values
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: MINIMUM_NETWORK_FEE,
			});
			InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::zero(),
				minimum: 0,
			});

			// Set a swap rate of 1 to make it easier
			SwapRate::set(1_f64);

			// Sanity check collected fees before any swaps
			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);

			fn init_swap(amount: AssetAmount) {
				Swapping::init_swap_request(
					Asset::Flip,
					amount,
					Asset::Eth,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::Egress {
							ccm_deposit_metadata: None,
							output_address: ForeignChainAddress::Eth(H160::zero()),
						},
					},
					Default::default(),
					None,
					None,
					SwapOrigin::Vault {
						tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
						broker_id: Some(BROKER),
					},
				);
			}
			// Swap with network fee
			init_swap(AMOUNT);
			// Swap that will be charged the minimum network fee
			init_swap(SMALL_AMOUNT);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount: AMOUNT,
					..
				}) if *network_fee == NETWORK_FEE * AMOUNT,
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount: SMALL_AMOUNT,
					..
				}) if *network_fee == MINIMUM_NETWORK_FEE,
			);

			// Check that the network fee is actually collected
			assert_eq!(
				CollectedNetworkFee::<Test>::get(),
				(NETWORK_FEE * AMOUNT) + MINIMUM_NETWORK_FEE
			);
		});
}

#[test]
fn internal_swap_uses_correct_network_fee() {
	const AMOUNT: AssetAmount = 10000;
	const SMALL_AMOUNT: AssetAmount = 500;
	const NETWORK_FEE: Permill = Permill::from_percent(10);
	const MINIMUM_NETWORK_FEE: AssetAmount = 100;

	new_test_ext()
		.execute_with(|| {
			// Set both network fees to different values
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: Permill::zero(), minimum: 0 });
			InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: MINIMUM_NETWORK_FEE,
			});

			// Set a swap rate of 1 to make it easier
			SwapRate::set(1_f64);

			// Sanity check collected fees before any swaps
			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);

			fn init_swap(amount: AssetAmount) {
				Swapping::init_swap_request(
					Asset::Flip,
					amount,
					Asset::Eth,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::CreditOnChain { account_id: 0_u64 },
					},
					Default::default(),
					None,
					None,
					SwapOrigin::OnChainAccount(0_u64),
				);
			}
			// Swap with network fee
			init_swap(AMOUNT);
			// Swap that will be charged the minimum network fee
			init_swap(SMALL_AMOUNT);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount: AMOUNT,
					..
				}) if *network_fee == NETWORK_FEE * AMOUNT,
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount: SMALL_AMOUNT,
					..
				}) if *network_fee == MINIMUM_NETWORK_FEE,
			);

			// Check that the network fee is actually collected
			assert_eq!(
				CollectedNetworkFee::<Test>::get(),
				(NETWORK_FEE * AMOUNT) + MINIMUM_NETWORK_FEE
			);
		});
}

#[test]
fn no_network_fee_minimum_for_gas_swaps() {
	const AMOUNT: AssetAmount = 500;
	const NETWORK_FEE: Permill = Permill::from_percent(10);
	const MINIMUM_NETWORK_FEE: AssetAmount = 100;

	assert!(NETWORK_FEE * AMOUNT < MINIMUM_NETWORK_FEE, "Minimum network fee must be large enough");

	new_test_ext()
		.execute_with(|| {
			// Set both minimums, just in case.
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: MINIMUM_NETWORK_FEE,
			});
			InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: MINIMUM_NETWORK_FEE,
			});

			// Set a swap rate of 1 to make it easier
			SwapRate::set(1_f64);

			// Sanity check collected fees before any swaps
			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);

			Swapping::init_swap_request(
				Asset::Flip,
				AMOUNT,
				Asset::Eth,
				SwapRequestType::IngressEgressFee,
				Default::default(),
				None,
				None,
				SwapOrigin::Internal,
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount: AMOUNT,
					..
				}) if *network_fee == NETWORK_FEE * AMOUNT,
			);

			// Check that the network fee is actually collected
			assert_eq!(CollectedNetworkFee::<Test>::get(), NETWORK_FEE * AMOUNT);
		});
}

#[test]
fn test_network_fee_tracking() {
	new_test_ext().execute_with(|| {
		const NETWORK_FEE: Permill = Permill::from_percent(10);
		const MIN_NETWORK_FEE: AssetAmount = 160;
		const CHUNK_AMOUNT: AssetAmount = 1000;
		let normal_fee_amount = NETWORK_FEE * CHUNK_AMOUNT;
		assert!(
			normal_fee_amount < MIN_NETWORK_FEE,
			"Minimum network fee must be larger than the network fee of a chunk"
		);

		// Setup a fresh tracker
		let mut fee_tracker = NetworkFeeTracker::new(FeeRateAndMinimum {
			minimum: MIN_NETWORK_FEE,
			rate: NETWORK_FEE,
		});

		// Take fees from each chunk in order and make sure it gives the expected result
		// First chunk gets the minimum network fee taken from it
		assert_eq!(
			fee_tracker.take_fee(CHUNK_AMOUNT),
			FeeTaken { remaining_amount: CHUNK_AMOUNT - MIN_NETWORK_FEE, fee: MIN_NETWORK_FEE }
		);
		// Second chunk gets partial network fee taken from it
		let partial_fee = normal_fee_amount * 2 - MIN_NETWORK_FEE;
		assert_eq!(
			fee_tracker.take_fee(CHUNK_AMOUNT),
			FeeTaken { remaining_amount: CHUNK_AMOUNT - partial_fee, fee: partial_fee }
		);
		// Remaining chunks get the full network fee taken from them
		assert_eq!(
			fee_tracker.take_fee(CHUNK_AMOUNT),
			FeeTaken { remaining_amount: CHUNK_AMOUNT - normal_fee_amount, fee: normal_fee_amount }
		);
		assert_eq!(
			fee_tracker.take_fee(CHUNK_AMOUNT),
			FeeTaken { remaining_amount: CHUNK_AMOUNT - normal_fee_amount, fee: normal_fee_amount }
		);
		// Make sure it can handle the chunk size changing
		assert_eq!(
			fee_tracker.take_fee(CHUNK_AMOUNT / 2),
			FeeTaken {
				remaining_amount: (CHUNK_AMOUNT / 2) - normal_fee_amount / 2,
				fee: normal_fee_amount / 2
			}
		);
		assert_eq!(
			fee_tracker.take_fee(CHUNK_AMOUNT * 2),
			FeeTaken {
				remaining_amount: (CHUNK_AMOUNT * 2) - normal_fee_amount * 2,
				fee: normal_fee_amount * 2
			}
		);
	});
}

#[test]
fn test_network_fee_calculation() {
	fn take_fees_from_swap(
		network_fee_percent: u32,
		minimum_network_fee: AssetAmount,
		chunk_amount: AssetAmount,
		accumulated_fee: AssetAmount,
		accumulated_stable_amount: AssetAmount,
	) -> (AssetAmount, AssetAmount) {
		let FeeTaken { remaining_amount, fee } = NetworkFeeTracker {
			network_fee: FeeRateAndMinimum {
				minimum: minimum_network_fee,
				rate: Permill::from_percent(network_fee_percent),
			},
			accumulated_stable_amount,
			accumulated_fee,
		}
		.take_fee(chunk_amount);
		(remaining_amount, fee)
	}

	new_test_ext().execute_with(|| {
		// Default amount to use in most cases
		const CHUNK_AMOUNT: AssetAmount = 1000;
		// Used when testing a network fee that is over the minimum
		const SMALL_MIN_NETWORK_FEE: AssetAmount = 20;
		// Default network fee used in most cases
		const NETWORK_FEE: u32 = 10;

		// Normal network fee
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, SMALL_MIN_NETWORK_FEE, CHUNK_AMOUNT, 0, 0),
			(CHUNK_AMOUNT - 100, 100)
		);
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, SMALL_MIN_NETWORK_FEE, CHUNK_AMOUNT, 1000, 10_000),
			(CHUNK_AMOUNT - 100, 100)
		);

		// Minimum network fee enforced
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, 200, CHUNK_AMOUNT, 0, 0),
			(CHUNK_AMOUNT - 200, 200)
		);
		assert_eq!(
			take_fees_from_swap(
				NETWORK_FEE,
				CHUNK_AMOUNT + 500,
				CHUNK_AMOUNT,
				CHUNK_AMOUNT,
				10_000,
			),
			(CHUNK_AMOUNT - 500, 500)
		);
		assert_eq!(take_fees_from_swap(NETWORK_FEE, 1500, CHUNK_AMOUNT, 0, 0), (0, CHUNK_AMOUNT));

		// Minimum network fee was taken on previous chunk
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, 200, CHUNK_AMOUNT, 200, CHUNK_AMOUNT),
			(CHUNK_AMOUNT, 0)
		);
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, 150, CHUNK_AMOUNT, 150, CHUNK_AMOUNT),
			(CHUNK_AMOUNT - 50, 50)
		);

		// Network fee changed after first chunk, so more or less is taken from this chunk
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, SMALL_MIN_NETWORK_FEE, CHUNK_AMOUNT, 50, 1000),
			(CHUNK_AMOUNT - 150, 150)
		);
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, SMALL_MIN_NETWORK_FEE, CHUNK_AMOUNT, 150, 1000),
			(CHUNK_AMOUNT - 50, 50)
		);

		// Unrealistic scenarios, but just to make sure it can handle it.
		assert_eq!(take_fees_from_swap(NETWORK_FEE, SMALL_MIN_NETWORK_FEE, 0, 100, 1000), (0, 0));
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, SMALL_MIN_NETWORK_FEE, CHUNK_AMOUNT, 0, 10_000),
			(0, CHUNK_AMOUNT)
		);
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, SMALL_MIN_NETWORK_FEE, CHUNK_AMOUNT, 10_000, 0),
			(CHUNK_AMOUNT, 0)
		);
		assert_eq!(
			take_fees_from_swap(10, SMALL_MIN_NETWORK_FEE, CHUNK_AMOUNT, u128::MAX, u128::MAX,),
			(CHUNK_AMOUNT, 0)
		);
		assert_eq!(
			take_fees_from_swap(NETWORK_FEE, SMALL_MIN_NETWORK_FEE, u128::MAX, 1000, 10_000,),
			// Because the calculation saturates, the existing 1000 fee taken is deducted from the
			// calculated fee
			(
				Permill::from_percent(90) * u128::MAX + 1 + 1000,
				Permill::from_percent(10) * u128::MAX - 1000
			)
		);
		assert_eq!(take_fees_from_swap(NETWORK_FEE, 0, 0, 0, 0), (0, 0));
	});
}

#[test]
fn test_calculate_input_for_desired_output() {
	new_test_ext().execute_with(|| {
		// If swap simulation fails -> no conversion.
		MockSwappingApi::set_swaps_should_fail(true);
		assert!(Swapping::calculate_input_for_desired_output(Asset::Flip, Asset::Eth, 1000, true)
			.is_none());

		// Set swap rate to 2 and turn swaps back on.
		SwapRate::set(2_f64);
		MockSwappingApi::set_swaps_should_fail(false);

		// Desired output is zero -> trivially ok.
		assert_eq!(
			Swapping::calculate_input_for_desired_output(Asset::Flip, Asset::Eth, 0, true),
			Some(0)
		);

		// Desired output requires 2 swap legs, each with a swap rate of 2. So output should be
		// 1/4th of input.
		assert_eq!(
			Swapping::calculate_input_for_desired_output(Asset::Flip, Asset::Eth, 1000, true),
			Some(250)
		);
		// Answer should be the same for gas calculation function
		assert_eq!(
			Swapping::calculate_input_for_gas_output::<Ethereum>(
				cf_chains::assets::eth::Asset::Flip,
				1000
			),
			250
		);

		// Desired output is gas asset, requires 1 swap leg. So output should be 1/2 of input.
		assert_eq!(
			Swapping::calculate_input_for_desired_output(Asset::Usdc, Asset::Eth, 1000, true),
			Some(500)
		);

		// Input is same asset -> trivially ok.
		assert_eq!(
			Swapping::calculate_input_for_desired_output(Asset::Eth, Asset::Eth, 1000, true),
			Some(1000)
		);

		// Make sure the network fee is taken. Also checking that the minimum is not enforced.
		NetworkFee::<Test>::set(FeeRateAndMinimum {
			rate: Permill::from_percent(1),
			minimum: 1000,
		});
		assert_eq!(
			Swapping::calculate_input_for_desired_output(Asset::Eth, Asset::Usdc, 1000, true),
			Some(505)
		);

		// Now that the network fee is not 0, make sure it can be ignored if desired.
		assert_eq!(
			Swapping::calculate_input_for_desired_output(Asset::Eth, Asset::Usdc, 1000, false),
			Some(500)
		);
	});
}

#[test]
fn network_fee_swap_gets_burnt() {
	const INPUT_ASSET: Asset = Asset::Usdc;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const AMOUNT: AssetAmount = 100;

	new_test_ext()
		.execute_with(|| {
			Swapping::init_network_fee_swap_request(INPUT_ASSET, AMOUNT);

			assert_eq!(FlipToBurn::<Test>::get(), 0);

			assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_asset: INPUT_ASSET,
					input_amount: AMOUNT,
					output_asset: OUTPUT_ASSET,
					request_type: SwapRequestTypeEncoded::NetworkFee,
					origin: SwapOrigin::Internal,
					..
				}),
			);

			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapScheduled { .. }),);
		})
		.then_process_blocks_until_block(SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(FlipToBurn::<Test>::get(), AMOUNT * DEFAULT_SWAP_RATE);
			assert_swaps_queue_is_empty();
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapExecuted { .. }),);
		});
}

#[test]
fn transaction_fees_are_collected() {
	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const INPUT_ASSET: Asset = Asset::Flip;
	const OUTPUT_ASSET: Asset = Asset::Eth;
	const AMOUNT: AssetAmount = 100;

	new_test_ext()
		.execute_with(|| {
			Swapping::init_swap_request(
				INPUT_ASSET,
				AMOUNT,
				OUTPUT_ASSET,
				SwapRequestType::IngressEgressFee,
				Default::default(),
				None,
				None,
				SwapOrigin::Internal,
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_asset: INPUT_ASSET,
					input_amount: AMOUNT,
					output_asset: OUTPUT_ASSET,
					request_type: SwapRequestTypeEncoded::IngressEgressFee,
					origin: SwapOrigin::Internal,
					..
				}),
			);

			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapScheduled { .. }),);

			assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

			assert_eq!(
				MockIngressEgressFeeHandler::<Ethereum>::withheld_assets(
					cf_chains::assets::eth::GAS_ASSET
				),
				0
			);
		})
		.then_process_blocks_until_block(SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(
				MockIngressEgressFeeHandler::<Ethereum>::withheld_assets(
					cf_chains::assets::eth::GAS_ASSET
				),
				AMOUNT * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE
			);
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			assert_swaps_queue_is_empty();
			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapExecuted { .. }),);
		});
}
#[test]
fn swap_broker_fee_calculated_correctly() {
	const FEES_BPS: [BasisPoints; 12] =
		[1, 5, 10, 100, 200, 500, 1000, 1500, 2000, 5000, 7500, 10000];
	const INPUT_AMOUNT: AssetAmount = 100000;

	const INTERMEDIATE_AMOUNT: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE;

	let mut total_fees = 0;
	for asset in Asset::all() {
		if asset != Asset::Usdc {
			for fee_bps in FEES_BPS {
				total_fees += Permill::from_parts(fee_bps as u32 * BASIS_POINTS_PER_MILLION) *
					INTERMEDIATE_AMOUNT;
			}
		}
	}

	new_test_ext()
		.execute_with(|| {
			Asset::all().for_each(|asset| {
				if asset != Asset::Usdc {
					for fee_bps in FEES_BPS {
						swap_with_custom_broker_fee(
							asset,
							Asset::Usdc,
							INPUT_AMOUNT,
							bounded_vec![Beneficiary { account: ALICE, bps: fee_bps }],
						);
					}
				}
			});
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Usdc), total_fees);
		});
}
#[test]
fn input_amount_excludes_network_fee() {
	const AMOUNT: AssetAmount = 1_000;
	const FROM_ASSET: Asset = Asset::Usdc;
	const TO_ASSET: Asset = Asset::Flip;
	let output_address: ForeignChainAddress = ForeignChainAddress::Eth(Default::default());
	const NETWORK_FEE: Permill = Permill::from_percent(1);

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: NETWORK_FEE, minimum: 0 });

			swap_with_custom_broker_fee(FROM_ASSET, TO_ASSET, AMOUNT, bounded_vec![]);

			<Pallet<Test> as SwapRequestHandler>::init_swap_request(
				FROM_ASSET,
				AMOUNT,
				TO_ASSET,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::Egress {
						ccm_deposit_metadata: None,
						output_address: output_address.clone(),
					},
				},
				bounded_vec![],
				None,
				None,
				SwapOrigin::Vault {
					tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(BROKER),
				},
			);
		})
		.then_process_blocks_until(|_| System::block_number() == 3)
		.then_execute_with(|_| {
			let network_fee = NETWORK_FEE * AMOUNT;
			let expected_input_amount = AMOUNT - network_fee;

			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
				swap_request_id: 1.into(),
				swap_id: 1.into(),
				input_asset: FROM_ASSET,
				output_asset: TO_ASSET,
				network_fee,
				broker_fee: 0,
				input_amount: expected_input_amount,
				output_amount: expected_input_amount * DEFAULT_SWAP_RATE,
				intermediate_amount: None,
				oracle_delta: None,
			}));
		});
}

#[test]
fn withdraw_broker_fees() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Swapping::withdraw(
				RuntimeOrigin::signed(BROKER),
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			<Error<Test>>::NoFundsAvailable
		);

		<Test as Config>::BalanceApi::credit_account(&BROKER, Asset::Eth, 200);
		assert_ok!(Swapping::withdraw(
			RuntimeOrigin::signed(BROKER),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
		let mut egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
		assert!(egresses.len() == 1);
		assert_eq!(egresses.pop().expect("must exist").amount(), 200);
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::WithdrawalRequested {
			account_id: BROKER,
			egress_id: (ForeignChain::Ethereum, 1),
			egress_asset: Asset::Eth,
			egress_amount: 200,
			destination_address: EncodedAddress::Eth(Default::default()),
			egress_fee: 0,
		}));
	});
}

#[test]
fn expect_earned_fees_to_be_recorded() {
	const INPUT_AMOUNT: AssetAmount = 10_000;
	const INTERMEDIATE_AMOUNT: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE;

	const NETWORK_FEE_PERCENT: u32 = 1;

	const ALICE: u64 = 2_u64;
	const BOB: u64 = 3_u64;

	const ALICE_FEE_BPS: u16 = 200;
	const BOB_FEE_BPS: u16 = 100;

	// Expected values:
	const NETWORK_FEE_1: AssetAmount = INTERMEDIATE_AMOUNT * NETWORK_FEE_PERCENT as u128 / 100;
	const ALICE_FEE_1: AssetAmount =
		(INTERMEDIATE_AMOUNT - NETWORK_FEE_1) * ALICE_FEE_BPS as u128 / 10_000;

	// This swap starts with USDC, so the fees are deducted from the input amount:
	const NETWORK_FEE_2: AssetAmount = INPUT_AMOUNT * NETWORK_FEE_PERCENT as u128 / 100;
	const ALICE_FEE_2: AssetAmount =
		(INPUT_AMOUNT - NETWORK_FEE_2) * ALICE_FEE_BPS as u128 / 10_000;

	const NETWORK_FEE_3: AssetAmount = INTERMEDIATE_AMOUNT * NETWORK_FEE_PERCENT as u128 / 100;
	const ALICE_FEE_3: AssetAmount =
		(INTERMEDIATE_AMOUNT - NETWORK_FEE_3) * ALICE_FEE_BPS as u128 / 10_000;
	const BOB_FEE_1: AssetAmount =
		(INTERMEDIATE_AMOUNT - NETWORK_FEE_3) * BOB_FEE_BPS as u128 / 10_000;

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::from_percent(NETWORK_FEE_PERCENT),
				minimum: 0,
			});
			swap_with_custom_broker_fee(
				Asset::Flip,
				Asset::Usdc,
				INPUT_AMOUNT,
				bounded_vec![Beneficiary { account: ALICE, bps: ALICE_FEE_BPS }],
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
				swap_request_id: 1.into(),
				swap_id: 1.into(),
				network_fee: NETWORK_FEE_1,
				broker_fee: ALICE_FEE_1,
				input_amount: INPUT_AMOUNT,
				input_asset: Asset::Flip,
				output_asset: Asset::Usdc,
				output_amount: INTERMEDIATE_AMOUNT - NETWORK_FEE_1 - ALICE_FEE_1,
				intermediate_amount: None,
				oracle_delta: None,
			}));

			assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Usdc), ALICE_FEE_1);
		})
		.execute_with(|| {
			swap_with_custom_broker_fee(
				Asset::Usdc,
				Asset::Flip,
				INPUT_AMOUNT,
				bounded_vec![Beneficiary { account: ALICE, bps: ALICE_FEE_BPS }],
			);
		})
		.then_process_blocks_until_block(5u32)
		.then_execute_with(|_| {
			const AMOUNT_AFTER_FEES: AssetAmount = INPUT_AMOUNT - NETWORK_FEE_2 - ALICE_FEE_2;
			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
				swap_request_id: 2.into(),
				swap_id: 2.into(),
				network_fee: NETWORK_FEE_2,
				broker_fee: ALICE_FEE_2,
				input_amount: AMOUNT_AFTER_FEES,
				input_asset: Asset::Usdc,
				output_asset: Asset::Flip,
				output_amount: AMOUNT_AFTER_FEES * DEFAULT_SWAP_RATE,
				intermediate_amount: None,
				oracle_delta: None,
			}));

			assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Usdc), ALICE_FEE_1 + ALICE_FEE_2);
		})
		.execute_with(|| {
			swap_with_custom_broker_fee(
				Asset::ArbEth,
				Asset::Flip,
				INPUT_AMOUNT,
				bounded_vec![
					Beneficiary { account: ALICE, bps: ALICE_FEE_BPS },
					Beneficiary { account: BOB, bps: BOB_FEE_BPS }
				],
			);
		})
		.then_process_blocks_until_block(7u32)
		.then_execute_with(|_| {
			const TOTAL_BROKER_FEES: AssetAmount = ALICE_FEE_3 + BOB_FEE_1;
			const INTERMEDIATE_AMOUNT_AFTER_FEES: AssetAmount =
				INTERMEDIATE_AMOUNT - NETWORK_FEE_3 - TOTAL_BROKER_FEES;

			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
				swap_request_id: 3.into(),
				swap_id: 3.into(),
				network_fee: NETWORK_FEE_3,
				broker_fee: TOTAL_BROKER_FEES,
				input_amount: INPUT_AMOUNT,
				input_asset: Asset::ArbEth,
				output_asset: Asset::Flip,
				output_amount: INTERMEDIATE_AMOUNT_AFTER_FEES * DEFAULT_SWAP_RATE,
				intermediate_amount: Some(INTERMEDIATE_AMOUNT_AFTER_FEES),
				oracle_delta: None,
			}));

			assert_eq!(
				get_broker_balance::<Test>(&ALICE, Asset::Usdc),
				ALICE_FEE_1 + ALICE_FEE_2 + ALICE_FEE_3
			);
			assert_eq!(get_broker_balance::<Test>(&BOB, Asset::Usdc), BOB_FEE_1);
		});
}

#[test]
fn minimum_network_fee_is_enforced_on_dca_swap() {
	const INPUT_AMOUNT: u128 = 300;
	const NUMBER_OF_CHUNKS: u32 = 3;
	const CHUNK_SIZE: u128 = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;

	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_3_BLOCK: u64 = CHUNK_2_BLOCK + SWAP_DELAY_BLOCKS as u64;

	// We set network fee so that the amount is small enough that the min network fee
	// will be enforced on the first chunk, but large enough that the rest of the chunks fees will
	// be above the minimum. And also large enough that the second chunk will only be partially
	// charged a fee.
	const NETWORK_FEE: Permill = Permill::from_percent(10);
	const BROKER_FEE_BPS: u16 = 100;
	const MIN_NETWORK_FEE: u128 = 30;
	assert!(MIN_NETWORK_FEE > NETWORK_FEE * CHUNK_SIZE * 2);
	assert!(MIN_NETWORK_FEE < NETWORK_FEE * CHUNK_SIZE * 4);

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: MIN_NETWORK_FEE,
			});

			Swapping::init_swap_request(
				// Doing a 2 leg swap, swap rate is 2, so output without fees is x4 and network fee
				// is applied to x2 of input (after first leg).
				Asset::Btc,
				INPUT_AMOUNT,
				Asset::ArbEth,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::Egress {
						output_address: ForeignChainAddress::Eth([1; 20].into()),
						ccm_deposit_metadata: None,
					},
				},
				vec![Beneficiary { account: BROKER, bps: BROKER_FEE_BPS }].try_into().unwrap(),
				None,
				Some(DcaParameters { number_of_chunks: NUMBER_OF_CHUNKS, chunk_interval: 2 }),
				SwapOrigin::Vault {
					tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(BROKER),
				},
			);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(1),
					input_amount: CHUNK_SIZE,
					// The first chunk has the minimum network fee enforced.
					// With swap rate at 2, output is (100*2-30-2)*2
					output_amount: 336,
					network_fee: 30,
					broker_fee: 2,
					..
				})
			);
		})
		.then_process_blocks_until_block(CHUNK_2_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
					input_amount: CHUNK_SIZE,
					// The second chunk will only partially be charged the network fee because the
					// amount that was already charged to the first chunk has covered part of
					// its fee already.
					output_amount: 376,
					network_fee: 10,
					broker_fee: 2,
					..
				})
			);
		})
		.then_process_blocks_until_block(CHUNK_3_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(3),
					input_amount: CHUNK_SIZE,
					// The rest of the chunks will be charged the normal network fee.
					output_amount: 356,
					network_fee: 20,
					broker_fee: 2,
					..
				})
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					// The final output should be 4x input amount minus the network fee and broker
					// fee. 1200 - 11%
					amount: 1068,
					..
				})
			);
		});
}

#[test]
fn test_refund_fee_calculation() {
	fn take_refund_fee(
		amount: AssetAmount,
		asset: Asset,
		is_internal_swap: bool,
	) -> (AssetAmount, AssetAmount) {
		let FeeTaken { remaining_amount, fee } =
			Swapping::take_refund_fee(amount, asset, is_internal_swap).unwrap();
		(remaining_amount, fee)
	}

	new_test_ext().execute_with(|| {
		// The regular refund fee is actually just the minimum network fee
		NetworkFee::<Test>::set(FeeRateAndMinimum { rate: Permill::zero(), minimum: 10 });

		// Usdc, no conversion needed, so the refund fee is just 10
		assert_eq!(take_refund_fee(1000, Asset::Usdc, false), (990, 10));
		assert_eq!(take_refund_fee(0, Asset::Usdc, false), (0, 0));
		assert_eq!(take_refund_fee(5, Asset::Usdc, false), (0, 5));
		assert_eq!(take_refund_fee(u128::MAX, Asset::Usdc, false), (u128::MAX - 10, 10));

		// Conversion needed, so the refund fee is 10 / DEFAULT_SWAP_RATE = 5
		assert_eq!(take_refund_fee(1000, Asset::Eth, false), (995, 5));
		assert_eq!(take_refund_fee(0, Asset::Eth, false), (0, 0));
		assert_eq!(take_refund_fee(3, Asset::Eth, false), (0, 3));

		// Internal swaps use a different network fee (and therefore refund fee)
		InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
			rate: Permill::zero(),
			minimum: 30,
		});
		assert_eq!(take_refund_fee(1000, Asset::Usdc, true), (970, 30));
		assert_eq!(take_refund_fee(1000, Asset::Eth, true), (985, 15));
	});
}

#[test]
fn gas_calculation_can_handle_extreme_swap_rate() {
	new_test_ext().execute_with(|| {
		fn test_extreme_swap_rate(swap_rate: f64) {
			SwapRate::set(swap_rate);
			assert_eq!(
				Swapping::calculate_input_for_gas_output::<Ethereum>(
					cf_chains::assets::eth::Asset::Flip,
					1000
				),
				8400000
			);
		}

		test_extreme_swap_rate(1_f64 / (u128::MAX as f64));
		test_extreme_swap_rate(0_f64);
		test_extreme_swap_rate(u128::MAX as f64);

		// Using Solana here because it has a ChainAmount of u64, so the conversion to AssetAmount
		// will fail when gas needed is larger than u64::MAX.
		SwapRate::set(0.5);
		assert_eq!(
			Swapping::calculate_input_for_gas_output::<cf_chains::Solana>(
				cf_chains::assets::sol::Asset::SolUsdc,
				u64::MAX
			),
			4058283696216101356
		);
	});
}

#[test]
fn test_get_network_fee() {
	const REGULAR_NETWORK_FEE: u32 = 5;
	const INTERNAL_SWAP_NETWORK_FEE: u32 = 6;
	const MINIMUM_NETWORK_FEE: AssetAmount = 123;

	fn test_get_fee(
		input_asset_fee: (Asset, Option<u32>),
		output_asset_fee: (Asset, Option<u32>),
		is_internal: bool,
		expected_fee: u32,
	) {
		new_test_ext().execute_with(|| {
			// Set the standard network fee
			if is_internal {
				InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
					rate: Permill::from_percent(INTERNAL_SWAP_NETWORK_FEE),
					minimum: MINIMUM_NETWORK_FEE,
				});
			} else {
				NetworkFee::<Test>::set(FeeRateAndMinimum {
					rate: Permill::from_percent(REGULAR_NETWORK_FEE),
					minimum: MINIMUM_NETWORK_FEE,
				});
			}

			// Set the custom network fees for the assets
			if let (asset, Some(fee)) = input_asset_fee {
				if is_internal {
					InternalSwapNetworkFeeForAsset::<Test>::insert(
						asset,
						Permill::from_percent(fee),
					);
				} else {
					NetworkFeeForAsset::<Test>::insert(asset, Permill::from_percent(fee));
				}
			}
			if let (asset, Some(fee)) = output_asset_fee {
				if is_internal {
					InternalSwapNetworkFeeForAsset::<Test>::insert(
						asset,
						Permill::from_percent(fee),
					);
				} else {
					NetworkFeeForAsset::<Test>::insert(asset, Permill::from_percent(fee));
				}
			}

			// Get the network fee for the swap
			let fee = Pallet::<Test>::get_network_fee_for_swap(
				input_asset_fee.0,
				output_asset_fee.0,
				is_internal,
			);

			// Check that the fee rate and minimum are as expected
			assert_eq!(fee.minimum, MINIMUM_NETWORK_FEE);
			assert_eq!(fee.rate, Permill::from_percent(expected_fee));
		});
	}

	fn test_all(is_internal: bool) {
		let network_fee = if is_internal { INTERNAL_SWAP_NETWORK_FEE } else { REGULAR_NETWORK_FEE };

		// The Standard network fee is used as a default when no custom fee is set
		test_get_fee((Asset::Flip, None), (Asset::Eth, None), is_internal, network_fee);
		test_get_fee(
			// Using a fee that is lower than the standard network fee, so the standard fee of the
			// other asset will be used.
			(Asset::Flip, Some(network_fee - 1)),
			(Asset::Eth, None),
			is_internal,
			network_fee,
		);
		test_get_fee(
			(Asset::Flip, None),
			// Using a fee that is lower than the standard network fee, so the standard fee of the
			// other asset will be used.
			(Asset::Eth, Some(network_fee - 2)),
			is_internal,
			network_fee,
		);

		// When above the standard network fee, The highest of the 2 custom fees is used.
		test_get_fee(
			(Asset::Flip, Some(network_fee + 10)),
			(Asset::Eth, Some(network_fee + 15)),
			is_internal,
			network_fee + 15,
		);
		test_get_fee(
			(Asset::Flip, None),
			(Asset::Eth, Some(network_fee + 15)),
			is_internal,
			network_fee + 15,
		);
		test_get_fee(
			(Asset::Flip, Some(network_fee + 15)),
			(Asset::Eth, Some(network_fee + 10)),
			is_internal,
			network_fee + 15,
		);
	}

	// Run test for both internal and regular swaps
	test_all(false);
	test_all(true);
}

#[test]
fn test_swap_with_custom_network_fee_for_asset() {
	const FEE_RATE_FLIP: Permill = Permill::from_percent(10);
	const FEE_RATE_ETH: Permill = Permill::from_percent(5);
	const NETWORK_FEE: Permill = Permill::from_percent(1);

	// We expect the higher fee rate to be used.
	let expected_fee = FEE_RATE_FLIP * INPUT_AMOUNT;

	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	new_test_ext()
		.execute_with(|| {
			// Set the swap rate to 1 to make the test simple
			SwapRate::set(1.0);

			// Set the standard network fee
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: NETWORK_FEE, minimum: 0 });

			// Set custom network fees for specific assets
			NetworkFeeForAsset::<Test>::insert(Asset::Flip, FEE_RATE_FLIP);
			NetworkFeeForAsset::<Test>::insert(Asset::Eth, FEE_RATE_ETH);

			// Now do a swap
			Swapping::init_swap_request(
				Asset::Flip,
				INPUT_AMOUNT,
				Asset::Eth,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::Egress {
						output_address: ForeignChainAddress::Eth([1; 20].into()),
						ccm_deposit_metadata: None,
					},
				},
				Default::default(),
				None,
				None,
				SwapOrigin::Vault {
					tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(BROKER),
				},
			);
		})
		.then_process_blocks_until_block(SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					input_amount: INPUT_AMOUNT,
					output_amount,
					network_fee,
					..
				}) if *network_fee == expected_fee && *output_amount == INPUT_AMOUNT - expected_fee
			);
		});
}
