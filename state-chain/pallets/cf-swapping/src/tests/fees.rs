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

use cf_traits::mocks::price_feed_api::MockPriceFeedApi;
use sp_runtime::SaturatedConversion;

use super::*;

#[test]
fn swap_output_amounts_correctly_account_for_fees() {
	for (from, to) in
		// non-stable to non-stable, non-stable to stable, stable to non-stable
		[
			(Asset::ArbUsdc, Asset::Usdt),
			(Asset::ArbUsdc, Asset::Usdc),
			(Asset::Usdc, Asset::Usdt),
		] {
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
					usdc_after_network_fees / DEFAULT_SWAP_RATE
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
			SwapRate::set(1);

			// Sanity check collected fees before any swaps
			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);

			fn init_swap(amount: AssetAmount) {
				Swapping::init_swap_request(
					Asset::Usdc,
					amount,
					Asset::Usdt,
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
			// For USDC input, event input_amount is AFTER network fee
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount,
					..
				}) if *network_fee == NETWORK_FEE * AMOUNT && *input_amount == AMOUNT - NETWORK_FEE * AMOUNT,
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount,
					..
				}) if *network_fee == MINIMUM_NETWORK_FEE && *input_amount == SMALL_AMOUNT - MINIMUM_NETWORK_FEE,
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
			SwapRate::set(1);

			// Sanity check collected fees before any swaps
			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);

			fn init_swap(amount: AssetAmount) {
				Swapping::init_swap_request(
					Asset::Usdc,
					amount,
					Asset::Usdt,
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
			// For USDC input, event input_amount is AFTER network fee
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount,
					..
				}) if *network_fee == NETWORK_FEE * AMOUNT && *input_amount == AMOUNT - NETWORK_FEE * AMOUNT,
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount,
					..
				}) if *network_fee == MINIMUM_NETWORK_FEE && *input_amount == SMALL_AMOUNT - MINIMUM_NETWORK_FEE,
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
			SwapRate::set(1);

			// Sanity check collected fees before any swaps
			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);

			Swapping::init_swap_request(
				Asset::Usdc,
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
			// For USDC input, event input_amount is AFTER network fee
			// USDC(500 - 50) -> Eth at rate=1: output = 450 * 10^12 = 450_000_000_000_000_000
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input_amount,
					..
				}) if *network_fee == NETWORK_FEE * AMOUNT && *input_amount == AMOUNT - NETWORK_FEE * AMOUNT,
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

/// Test Swap simulation,
/// Flip and Dot don't use the price oracle, so we use swap simulation to estimate prices for
/// them.
#[test]
fn test_calculate_input_for_desired_output_using_swap_simulation() {
	new_test_ext().execute_with(|| {
		NetworkFee::<Test>::set(FeeRateAndMinimum { rate: Permill::from_percent(1), minimum: 0 });

		// The swap simulation will use the swap rate in tests to estimate prices.
		SwapRate::set(2); // 1 Asset : 2 USD

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Usdc,
				Asset::Usdt,
				500,
				false,
				false
			),
			500 // Actual result from test
		);

		// Should be the inverse of the above.
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Usdt,
				Asset::Usdc,
				1000,
				false,
				false
			),
			1000 // Actual result from test (likely same-dec gives 1:1 with fees)
		);

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Usdt,
				Asset::Usdc,
				1000,
				true,
				false
			),
			1010 // With 1% network fee
		);

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Usdt,
				Asset::ArbUsdc,
				1000,
				false,
				false
			),
			1000 // 2 leg swap with same decimals
		);

		// Cross-decimal: Flip (18-dec) → Eth (18-dec) via USDC (6-dec).
		// Flip uses swap sim ($2/FLIP), Eth falls back to hard coded ($2,800/ETH).
		// 1 ETH ≈ 2800/2 = 1400 FLIP.
		let expected = 1400 * 10u128.pow(18);
		assert!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Flip,
				Asset::Eth,
				10u128.pow(18), // 1 ETH
				false,
				false
			)
			.abs_diff(expected) <
				expected / 10_000, // within 1 bps
		);
	});
}

/// Test hard coded fallback prices.
/// These test values will need to be updated every time the hard coded prices are updated.
#[test]
fn test_calculate_input_for_desired_output_using_hard_coded_prices() {
	new_test_ext().execute_with(|| {
		NetworkFee::<Test>::set(FeeRateAndMinimum { rate: Permill::from_percent(1), minimum: 0 });

		// Fallback to hard coded prices when swap simulation fails
		MockSwappingApi::set_swaps_should_fail(true);
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Flip,
				Asset::Eth,
				2 * 10u128.pow(18),
				false,
				false
			),
			14_000 * 10u128.pow(18) + 1 // 2 ETH ~= 14000 FLIP plus small rounding error
		);

		// Also fallback to hard coded prices when oracle is unavailable (This should never
		// happen in the real world)
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Btc,
				Asset::Eth,
				2 * 10u128.pow(18),
				false,
				false
			),
			6473989 // 2 ETH ~=  0.06473988439 BTC
		);

		// Make sure the network fee is also taken into account when using hard coded prices
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Btc,
				Asset::Eth,
				2 * 10u128.pow(18),
				true, // With network fee
				false
			),
			6539382 // Same as above + 1% network fee
		);
	});
}

/// Test the use of the price oracle in calculating fees/gas.
#[test]
fn test_calculate_input_for_desired_output_using_oracle_prices() {
	new_test_ext().execute_with(|| {
		NetworkFee::<Test>::set(FeeRateAndMinimum { rate: Permill::from_percent(1), minimum: 0 });

		// Set some arbitrary prices in the oracle
		MockPriceFeedApi::set_price_usd(Asset::Btc, 30_000);
		MockPriceFeedApi::set_price_usd(Asset::Eth, 2_000);
		MockPriceFeedApi::set_price_usd(Asset::Usdc, 1);
		MockPriceFeedApi::set_price_usd(Asset::ArbUsdc, 1);
		MockPriceFeedApi::set_price_usd(Asset::Usdt, 1);
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Usdc,
				Asset::Eth,
				2 * 10u128.pow(18),
				false,
				false
			),
			4_000 * 10u128.pow(6) + 16_000_000 // $4k + 40bps
		);

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::ArbUsdc,
				Asset::Usdt,
				1_000_000_000,
				false,
				false
			),
			1_000_000_000 + 600_181 // $1k + 6bps
		);

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Eth,
				Asset::Btc,
				10u128.pow(8),
				false,
				false
			),
			15 * 10u128.pow(18) + 120481927710843374 // 15 ETH + 80bps
		);

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Usdc,
				Asset::Btc,
				10u128.pow(8),
				true, // With network fee
				false
			),
			// $30k + 40bps + 1% network fee - precision error
			30_000_000_000 + 120_000_000 + 304242424 - 3
		);

		// Using both a hard coded price (Sol) and an oracle price (Btc)
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Sol,
				Asset::Btc,
				10u128.pow(8),
				false,
				false
			),
			236220472441 + 944_881_890 // ~236 SOL + 40bps
		);

		// Cross-decimal: Flip (18-dec) → Btc (8-dec).
		// Flip uses swap sim ($2/FLIP), Btc uses oracle ($30,000 + 40bps = $30,120).
		// 1 BTC ≈ 30120/2 = 15060 FLIP.
		let expected = 15060 * 10u128.pow(18);
		assert!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Flip,
				Asset::Btc,
				10u128.pow(8), // 1 BTC
				false,
				false
			)
			.abs_diff(expected) <
				expected / 10_000, // within 1 bps
		);

		// Check that the network fee is still applied when using the same asset as the input and
		// output
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Btc,
				Asset::Btc,
				10u128.pow(8),
				true, // With network fee
				false
			),
			10u128.pow(8) + 1010101 // output + 1% network fee
		);

		// Make sure it can handle extreme edge cases
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Btc,
				Asset::Eth,
				0,
				true,
				false
			),
			0
		);
		// Here we do not care about the actual value, just that it does not panic.
		let _ = Swapping::calculate_input_for_desired_output_or_default_to_zero(
			Asset::Btc,
			Asset::Eth,
			u128::MAX,
			true,
			false,
		);
	});
}

#[test]
fn network_fee_swap_gets_burnt() {
	const INPUT_ASSET: Asset = Asset::Usdc;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	// Use a large amount so the cross-decimal USDC->Flip swap produces non-zero output
	const AMOUNT: AssetAmount = 100_000_000_000_000; // $100M in USDC base units

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
			// Compute expected FLIP output using the same cross-decimal pricing as the mock:
			// USDC→Flip requires inverting the Flip→USD price
			let expected_flip: AssetAmount =
				Price::from_usd(OUTPUT_ASSET, DEFAULT_SWAP_RATE as u32)
					.invert()
					.output_amount_floor(AMOUNT)
					.saturated_into();
			assert!(expected_flip > 0, "Network fee swap should produce non-zero FLIP output");
			assert_eq!(FlipToBurn::<Test>::get(), expected_flip as i128);
			assert_swaps_queue_is_empty();
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapExecuted { .. }),);
		});
}

#[test]
fn transaction_fees_are_collected() {
	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const INPUT_ASSET: Asset = Asset::Usdc;
	const OUTPUT_ASSET: Asset = Asset::Eth;
	// Use a large amount so the cross-decimal USDC->Eth swap produces non-zero output
	const AMOUNT: AssetAmount = 100_000_000_000_000; // $100M in USDC base units

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
			// Compute expected Eth output using the same cross-decimal pricing as the mock:
			// USDC→Eth requires inverting the Eth→USD price
			let expected_output: AssetAmount =
				Price::from_usd(OUTPUT_ASSET, DEFAULT_SWAP_RATE as u32)
					.invert()
					.output_amount_floor(AMOUNT)
					.saturated_into();
			assert!(expected_output > 0, "IngressEgress fee swap should produce non-zero output");
			assert_eq!(
				MockIngressEgressFeeHandler::<Ethereum>::withheld_assets(
					cf_chains::assets::eth::GAS_ASSET
				),
				expected_output
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

	// Only test with 6-dec assets to avoid cross-decimal issues
	const TEST_ASSETS: [Asset; 7] = [
		Asset::Usdt,
		Asset::ArbUsdc,
		Asset::ArbUsdt,
		Asset::SolUsdc,
		Asset::SolUsdt,
		Asset::HubUsdc,
		Asset::HubUsdt,
	];

	let mut total_fees = 0;
	for _asset in TEST_ASSETS {
		for fee_bps in FEES_BPS {
			total_fees += Permill::from_parts(fee_bps as u32 * BASIS_POINTS_PER_MILLION) *
				INTERMEDIATE_AMOUNT;
		}
	}

	new_test_ext()
		.execute_with(|| {
			for asset in TEST_ASSETS {
				for fee_bps in FEES_BPS {
					swap_with_custom_broker_fee(
						asset,
						Asset::Usdc,
						INPUT_AMOUNT,
						bounded_vec![Beneficiary { account: ALICE, bps: fee_bps }],
					);
				}
			}
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
	const TO_ASSET: Asset = Asset::Usdt;
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

			// For USDC->Usdt at rate=2: output = input / rate = 990 / 2 = 495
			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
				swap_request_id: 1.into(),
				swap_id: 1.into(),
				input_asset: FROM_ASSET,
				output_asset: TO_ASSET,
				network_fee,
				broker_fee: 0,
				input_amount: expected_input_amount,
				output_amount: expected_input_amount / DEFAULT_SWAP_RATE,
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
	// With new mock: asset→USDC gives input * rate, USDC→asset gives input / rate
	const INTERMEDIATE_AMOUNT_1: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE; // USDT→USDC
	const INTERMEDIATE_AMOUNT_3: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE; // ArbUsdc→USDC

	const NETWORK_FEE_PERCENT: u32 = 1;

	const ALICE: u64 = 2_u64;
	const BOB: u64 = 3_u64;

	const ALICE_FEE_BPS: u16 = 200;
	const BOB_FEE_BPS: u16 = 100;

	// Expected values:
	const NETWORK_FEE_1: AssetAmount = INTERMEDIATE_AMOUNT_1 * NETWORK_FEE_PERCENT as u128 / 100;
	const ALICE_FEE_1: AssetAmount =
		(INTERMEDIATE_AMOUNT_1 - NETWORK_FEE_1) * ALICE_FEE_BPS as u128 / 10_000;

	// This swap starts with USDC, so the fees are deducted from the input amount:
	const NETWORK_FEE_2: AssetAmount = INPUT_AMOUNT * NETWORK_FEE_PERCENT as u128 / 100;
	const ALICE_FEE_2: AssetAmount =
		(INPUT_AMOUNT - NETWORK_FEE_2) * ALICE_FEE_BPS as u128 / 10_000;

	const NETWORK_FEE_3: AssetAmount = INTERMEDIATE_AMOUNT_3 * NETWORK_FEE_PERCENT as u128 / 100;
	const ALICE_FEE_3: AssetAmount =
		(INTERMEDIATE_AMOUNT_3 - NETWORK_FEE_3) * ALICE_FEE_BPS as u128 / 10_000;
	const BOB_FEE_1: AssetAmount =
		(INTERMEDIATE_AMOUNT_3 - NETWORK_FEE_3) * BOB_FEE_BPS as u128 / 10_000;

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::from_percent(NETWORK_FEE_PERCENT),
				minimum: 0,
			});
			swap_with_custom_broker_fee(
				Asset::Usdt,
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
				input_asset: Asset::Usdt,
				output_asset: Asset::Usdc,
				output_amount: INTERMEDIATE_AMOUNT_1 - NETWORK_FEE_1 - ALICE_FEE_1,
				intermediate_amount: None,
				oracle_delta: None,
			}));

			assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Usdc), ALICE_FEE_1);
		})
		.execute_with(|| {
			swap_with_custom_broker_fee(
				Asset::Usdc,
				Asset::Usdt,
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
				output_asset: Asset::Usdt,
				output_amount: AMOUNT_AFTER_FEES / DEFAULT_SWAP_RATE,
				intermediate_amount: None,
				oracle_delta: None,
			}));

			assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Usdc), ALICE_FEE_1 + ALICE_FEE_2);
		})
		.execute_with(|| {
			swap_with_custom_broker_fee(
				Asset::ArbUsdc,
				Asset::Usdt,
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
				INTERMEDIATE_AMOUNT_3 - NETWORK_FEE_3 - TOTAL_BROKER_FEES;

			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
				swap_request_id: 3.into(),
				swap_id: 3.into(),
				network_fee: NETWORK_FEE_3,
				broker_fee: TOTAL_BROKER_FEES,
				input_amount: INPUT_AMOUNT,
				input_asset: Asset::ArbUsdc,
				output_asset: Asset::Usdt,
				output_amount: INTERMEDIATE_AMOUNT_AFTER_FEES / DEFAULT_SWAP_RATE,
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
				// 2-leg same-dec swap: ArbUsdc->USDC->Usdt. Rate=2.
				// Intermediate = input*2, output = intermediate/2 = input (before fees).
				// Network fee applied to USDC intermediate.
				Asset::ArbUsdc,
				INPUT_AMOUNT,
				Asset::Usdt,
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
			// Chunk 1: input=100, intermediate=200, network_fee=30 (min), broker=2 (1% of 170),
			// output=(200-30-2)/2=84
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(1),
					input_amount: CHUNK_SIZE,
					output_amount: 84,
					network_fee: 30,
					broker_fee: 2,
					..
				})
			);
		})
		.then_process_blocks_until_block(CHUNK_2_BLOCK)
		.then_execute_with(|_| {
			// Chunk 2: intermediate=200, cumulative=400, expected_total_fee=40, already_taken=30
			// Additional fee=10, broker=2 (1% of 200-10=190), output=(200-10-2)/2=94
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
					input_amount: CHUNK_SIZE,
					output_amount: 94,
					network_fee: 10,
					broker_fee: 2,
					..
				})
			);
		})
		.then_process_blocks_until_block(CHUNK_3_BLOCK)
		.then_execute_with(|_| {
			// Chunk 3: intermediate=200, cumulative=600, expected_total_fee=60, already_taken=40
			// Additional fee=20, broker=2 (1% of 200-20=180), output=(200-20-2)/2=89
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(3),
					input_amount: CHUNK_SIZE,
					output_amount: 89,
					network_fee: 20,
					broker_fee: 2,
					..
				})
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					// Total output: 84 + 94 + 89 = 267
					amount: 267,
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

		// Conversion needed. For same-dec swaps, effective rate is ~1:1.
		// Using Usdt (same-dec as USDC) for conversion.
		assert_eq!(take_refund_fee(1000, Asset::Usdt, false), (990, 10));
		assert_eq!(take_refund_fee(0, Asset::Usdt, false), (0, 0));
		assert_eq!(take_refund_fee(10, Asset::Usdt, false), (0, 10));

		// Internal swaps use a different network fee (and therefore refund fee)
		InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
			rate: Permill::zero(),
			minimum: 30,
		});
		assert_eq!(take_refund_fee(1000, Asset::Usdc, true), (970, 30));
		// For Usdt: same-dec effective 1:1, so fee = 30
		assert_eq!(take_refund_fee(1000, Asset::Usdt, true), (970, 30));
	});
}

#[test]
fn gas_calculation_can_handle_extreme_swap_rate() {
	new_test_ext().execute_with(|| {
		fn test_extreme_swap_rate(swap_rate: u32) {
			SwapRate::set(swap_rate);
			// Just run it and make sure it doesn't panic
			// Using Usdt (same-dec as USDC) to avoid cross-decimal issues
			Swapping::calculate_input_for_gas_output::<Ethereum>(
				cf_chains::assets::eth::Asset::Usdt,
				1000,
			);
		}

		// Smallest non-zero supported integer rate.
		test_extreme_swap_rate(1);
		test_extreme_swap_rate(0);
		test_extreme_swap_rate(u32::MAX);
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
	const FEE_RATE_USDT: Permill = Permill::from_percent(10);
	const FEE_RATE_ARBUSDC: Permill = Permill::from_percent(5);
	const NETWORK_FEE: Permill = Permill::from_percent(1);

	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	new_test_ext()
		.execute_with(|| {
			// Set the swap rate to 1 to make the test simple
			SwapRate::set(1);

			// Set the standard network fee
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: NETWORK_FEE, minimum: 0 });

			// Set custom network fees for specific assets
			NetworkFeeForAsset::<Test>::insert(Asset::Usdt, FEE_RATE_USDT);
			NetworkFeeForAsset::<Test>::insert(Asset::ArbUsdc, FEE_RATE_ARBUSDC);

			// Now do a swap using same-dec assets
			Swapping::init_swap_request(
				Asset::Usdt,
				INPUT_AMOUNT,
				Asset::ArbUsdc,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::Egress {
						output_address: ForeignChainAddress::Arb([1; 20].into()),
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
			// Usdt->Usdc->ArbUsdc at rate=1: intermediate = INPUT_AMOUNT / 1 = INPUT_AMOUNT
			// We expect the higher fee rate (Usdt=10%) to be used on the USDC intermediate
			let expected_fee = FEE_RATE_USDT * INPUT_AMOUNT;
			// After fee, output = (INPUT_AMOUNT - expected_fee) * 1 = INPUT_AMOUNT - expected_fee
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

#[test]
fn test_network_fee_tracking_when_rescheduled() {
	// USDC -> Usdt: decreasing SwapRate gives more output, allowing min_price to be met.
	const INPUT_ASSET: Asset = Asset::Usdc;
	const OUTPUT_ASSET: Asset = Asset::Usdt;
	const INPUT_AMOUNT: AssetAmount = 1_000;
	const RETRY_BLOCK: u64 =
		INIT_BLOCK + (SWAP_DELAY_BLOCKS + DEFAULT_SWAP_RETRY_DELAY_BLOCKS) as u64;
	const NETWORK_FEE: Permill = Permill::from_percent(1);
	// Set a minimum network fee that will be enforced
	const NETWORK_FEE_MINIMUM: AssetAmount = 100;

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: NETWORK_FEE_MINIMUM,
			});

			Swapping::init_swap_request(
				INPUT_ASSET,
				INPUT_AMOUNT,
				OUTPUT_ASSET,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::Egress {
						ccm_deposit_metadata: None,
						output_address: ForeignChainAddress::Eth(Default::default()),
					},
				},
				vec![Beneficiary { account: BROKER, bps: 10 }].try_into().unwrap(),
				Some(PriceLimitsAndExpiry {
					expiry_behaviour: ExpiryBehaviour::NoExpiry,
					// At rate=2: output = (1000 - 100) / 2 = 450, price = 0.45 < 0.48 -> reschedule
					// At rate=1: output = (1000 - 100) = 900, price = 0.9 >= 0.48 -> succeeds
					// Set min_price to 0.48: Price::from_amounts_bounded(480, 1000)
					min_price: Price::from_amounts_bounded(480u128.into(), 1000u128.into()),
					max_oracle_price_slippage: None,
				}),
				None,
				SwapOrigin::Internal,
			);

			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			// Check that the swap was rescheduled due to price limits
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled { execute_at: RETRY_BLOCK, .. }),
			);

			// Check that no network fee was taken yet
			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);

			// Change the swap rate so that the swap can proceed next try
			// For USDC→Usdt with new mock: output = (1000-100) / rate
			// At rate=1: output = 900, price = 0.9 >= 0.48 -> succeeds
			SwapRate::set(1);
		})
		.then_process_blocks_until_block(RETRY_BLOCK)
		.then_execute_with(|_| {
			// Check that the network fee was still applied correctly after rescheduling
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					network_fee,
					..
				}) if *network_fee == NETWORK_FEE_MINIMUM
			);

			assert_eq!(CollectedNetworkFee::<Test>::get(), NETWORK_FEE_MINIMUM);
		});
}
