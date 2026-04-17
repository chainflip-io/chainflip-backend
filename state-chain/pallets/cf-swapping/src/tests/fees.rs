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

use cf_primitives::AssetAndAmount;
use cf_test_utilities::assert_no_matching_event;
use cf_traits::mocks::price_feed_api::MockPriceFeedApi;
use sp_std::collections::btree_set::BTreeSet;

use super::*;

#[test]
fn all_swaps_have_correct_egress_amounts_after_fees() {
	let swaps = generate_test_swaps();
	let network_fee = Permill::from_percent(1);

	new_test_ext()
		.execute_with(|| {
			// Set the network fee
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: network_fee, minimum: 0 });

			// Add the test swaps to the queue
			insert_swaps(&swaps, Some(BROKER_FEE_BPS));
		})
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// Calculate the expected egress amounts: network fee taken from input,
			// broker fee taken from output.
			let mut expected_egresses = swaps
				.iter()
				.map(|swap| {
					let network_fee = network_fee * swap.input_amount;
					let input_after_network_fee = swap.input_amount - network_fee;

					let is_one_leg_swap =
						swap.input_asset == STABLE_ASSET || swap.output_asset == STABLE_ASSET;

					let pre_broker_output = if is_one_leg_swap {
						input_after_network_fee * DEFAULT_SWAP_RATE
					} else {
						input_after_network_fee * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE
					};

					let broker_fee = permill_from_bps(BROKER_FEE_BPS) * pre_broker_output;
					let output_amount = pre_broker_output - broker_fee;

					MockEgressParameter::<AnyChain>::Swap {
						asset: swap.output_asset,
						amount: output_amount,
						destination_address: swap.output_address.clone(),
						fee: 0,
					}
				})
				.collect::<Vec<_>>();
			expected_egresses.sort();

			// Compare with the actual scheduled egresses
			let mut actual_egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
			actual_egresses.sort();
			assert_eq!(expected_egresses, actual_egresses);
		});
}

#[test]
fn test_buy_back_flip() {
	new_test_ext().execute_with(|| {
		const INTERVAL: BlockNumberFor<Test> = 5;
		const USDC_FEE: AssetAmount = 100;
		const BTC_FEE: AssetAmount = 200;
		const ETH_FEE: AssetAmount = 300;

		// Get some network fees for 3 different assets
		CollectedNetworkFee::<Test>::mutate(|fees| {
			fees.insert(Asset::Usdc, USDC_FEE);
			fees.insert(Asset::Btc, BTC_FEE);
			fees.insert(Asset::Eth, ETH_FEE);
		});

		// The default buy interval is zero. Check that buy back is disabled & on_initialize does
		// not panic.
		assert_eq!(FlipBuyInterval::<Test>::get(), 0);
		Swapping::on_initialize(1);
		assert_eq!(get_collected_network_fee(Asset::Usdc), USDC_FEE);
		assert_eq!(get_collected_network_fee(Asset::Btc), BTC_FEE);
		assert_eq!(get_collected_network_fee(Asset::Eth), ETH_FEE);

		// Set a non-zero buy interval
		FlipBuyInterval::<Test>::set(INTERVAL);

		// Nothing is bought if we're not at the interval.
		Swapping::on_initialize(INTERVAL * 3 - 1);
		assert_eq!(get_collected_network_fee(Asset::Usdc), USDC_FEE);
		assert_eq!(get_collected_network_fee(Asset::Btc), BTC_FEE);
		assert_eq!(get_collected_network_fee(Asset::Eth), ETH_FEE);

		// If we're at an interval, all collected fees should be cleared and swap requests
		// scheduled for each asset.
		Swapping::on_initialize(INTERVAL * 3);
		assert_eq!(get_collected_network_fee(Asset::Usdc), 0);
		assert_eq!(get_collected_network_fee(Asset::Btc), 0);
		assert_eq!(get_collected_network_fee(Asset::Eth), 0);

		// Note that no network fee will be charged on these buy-back swaps:
		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::SwapRequested {
				input_asset: Asset::Usdc,
				input_amount: USDC_FEE,
				output_asset: Asset::Flip,
				request_type: SwapRequestTypeEncoded::NetworkFee,
				origin: SwapOrigin::Internal,
				..
			})
		);
		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::SwapRequested {
				input_asset: Asset::Btc,
				input_amount: BTC_FEE,
				output_asset: Asset::Flip,
				request_type: SwapRequestTypeEncoded::NetworkFee,
				origin: SwapOrigin::Internal,
				..
			})
		);
		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::SwapRequested {
				input_asset: Asset::Eth,
				input_amount: ETH_FEE,
				output_asset: Asset::Flip,
				request_type: SwapRequestTypeEncoded::NetworkFee,
				origin: SwapOrigin::Internal,
				..
			})
		);
		// 3 network fee swap requests were initiated
		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::NetworkFeeSwapsInitiated { swap_request_ids })
				if BTreeSet::from_iter(swap_request_ids.iter().copied()) == BTreeSet::from([SwapRequestId(1), SwapRequestId(2), SwapRequestId(3)])
		);
	});
}

/// This test covers:
/// - The internal network fee is not used for normal swaps
/// - The custom network fee is applied if set for an asset
/// - The network fee minimum is correct for oracle assets
/// - The network fee minimum is correct for non-oracle assets (using swap simulation)
#[test]
fn normal_swap_uses_correct_network_fee() {
	const AMOUNT: AssetAmount = 10000;
	const SMALL_AMOUNT: AssetAmount = 500;
	const NETWORK_FEE: Permill = Permill::from_percent(10);
	const NETWORK_FEE_FOR_BTC: Permill = Permill::from_percent(15);
	const BTC_PRICE_USD: u32 = 4;
	const MINIMUM_NETWORK_FEE_USDC: AssetAmount = 400;
	// Btc will use the oracle to calculate the minimum fee. This means a small oracle slippage will
	// be a added to the minimum.
	const MINIMUM_NETWORK_FEE_BTC: AssetAmount =
		(MINIMUM_NETWORK_FEE_USDC / BTC_PRICE_USD as AssetAmount) + 1;
	// Flip will use the swap simulation to calculate the minimum fee
	const MINIMUM_NETWORK_FEE_FLIP: AssetAmount = MINIMUM_NETWORK_FEE_USDC / DEFAULT_SWAP_RATE;

	new_test_ext()
		.execute_with(|| {
			// Set both network fees to different values
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: MINIMUM_NETWORK_FEE_USDC,
			});
			InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::zero(),
				minimum: 0,
			});
			// Setting a different network fee for BTC to make sure the correct one is applied for each asset
			NetworkFeeForAsset::<Test>::insert(Asset::Btc, NETWORK_FEE_FOR_BTC);

			// Set the price oracle so the minimum can be calculated
			MockPriceFeedApi::set_price_usd_fine(Asset::Btc, BTC_PRICE_USD.into());
			MockPriceFeedApi::set_price_usd_fine(Asset::Usdc, 1);

			// Sanity check collected fees before any swaps
			assert_eq!(get_collected_network_fee(Asset::Btc), 0);
			assert_eq!(get_collected_network_fee(Asset::Flip), 0);

			fn init_swap(asset: Asset, amount: AssetAmount) {
				Swapping::init_swap_request(
					asset,
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
			// Swap with normal network fee (no minimum applied)
			init_swap(Asset::Btc, AMOUNT);
			init_swap(Asset::Flip, AMOUNT);
			// Swap with swap simulation network fee minimum (Flip)
			init_swap(Asset::Flip, SMALL_AMOUNT);
			// Swap with oracle network fee minimum (Btc)
			init_swap(Asset::Btc, SMALL_AMOUNT);
		})
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input: AssetAndAmount{ asset: Asset::Btc, amount },
					..
				}) if *network_fee == AssetAndAmount { asset: Asset::Btc, amount: NETWORK_FEE_FOR_BTC * AMOUNT } && *amount == AMOUNT - (NETWORK_FEE_FOR_BTC * AMOUNT),
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input: AssetAndAmount{ asset: Asset::Flip, amount },
					..
				}) if *network_fee == AssetAndAmount { asset: Asset::Flip, amount: NETWORK_FEE * AMOUNT } && *amount == AMOUNT - (NETWORK_FEE * AMOUNT),
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input: AssetAndAmount{ asset: Asset::Btc, amount},
					..
				}) if *network_fee == AssetAndAmount { asset: Asset::Btc, amount: MINIMUM_NETWORK_FEE_BTC } && *amount == SMALL_AMOUNT - MINIMUM_NETWORK_FEE_BTC,
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input: AssetAndAmount{ asset: Asset::Flip, amount},
					..
				}) if *network_fee == AssetAndAmount { asset: Asset::Flip, amount: MINIMUM_NETWORK_FEE_FLIP } && *amount == SMALL_AMOUNT - MINIMUM_NETWORK_FEE_FLIP,
			);


			// Check that the network fee is actually collected
			assert_eq!(
				get_collected_network_fee(Asset::Btc),
				(NETWORK_FEE_FOR_BTC * AMOUNT) + MINIMUM_NETWORK_FEE_BTC
			);
			assert_eq!(
				get_collected_network_fee(Asset::Flip),
				(NETWORK_FEE * AMOUNT) + MINIMUM_NETWORK_FEE_FLIP
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
			assert_eq!(get_collected_network_fee(Asset::Flip), 0);

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
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input: AssetAndAmount { asset: Asset::Flip, amount },
					..
				}) if *network_fee == AssetAndAmount { asset: Asset::Flip, amount: NETWORK_FEE * AMOUNT } && *amount == AMOUNT - (NETWORK_FEE * AMOUNT),
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					network_fee,
					input: AssetAndAmount { asset: Asset::Flip, amount },
					..
				}) if *network_fee == AssetAndAmount { asset: Asset::Flip, amount: MINIMUM_NETWORK_FEE } && *amount == SMALL_AMOUNT - MINIMUM_NETWORK_FEE,
			);

			// Check that the network fee is actually collected
			assert_eq!(
				get_collected_network_fee(Asset::Flip),
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
			assert_eq!(get_collected_network_fee(Asset::Flip), 0);

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
					input: AssetAndAmount { asset: Asset::Flip, amount },
					..
				}) if *network_fee == AssetAndAmount { asset: Asset::Flip, amount: NETWORK_FEE * AMOUNT } && *amount == AMOUNT - (NETWORK_FEE * AMOUNT),
			);

			// Check that the network fee is actually collected
			assert_eq!(get_collected_network_fee(Asset::Flip), NETWORK_FEE * AMOUNT);
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
		processed_asset_amount: AssetAmount,
	) -> (AssetAmount, AssetAmount) {
		let FeeTaken { remaining_amount, fee } = NetworkFeeTracker {
			network_fee: FeeRateAndMinimum {
				minimum: minimum_network_fee,
				rate: Permill::from_percent(network_fee_percent),
			},
			processed_asset_amount,
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
		SwapRate::set(2_f64);

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Flip,
				Asset::Usdc,
				1000,
				false,
				false
			),
			500 // 1 leg swap, so 1/2 of input
		);

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Flip,
				Asset::Usdc,
				1000,
				true,
				false
			),
			505 // 1 leg swap + 1% network fee
		);

		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Flip,
				Asset::Dot,
				1000,
				false,
				false
			),
			250 // 2 leg swap, so 1/4th of input
		);

		// Using a combination of swap simulation (flip) and hard coded price (Eth).
		SwapRate::set(0.000000000002_f64); // Flip will be worth $2 via swap simulation
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Flip,
				Asset::Eth,
				10_u128.pow(18),
				false,
				false
			),
			// So the result is half the Eth price, plus a small rounding error
			1400 * 10_u128.pow(18) + 1
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

		// Using both Swap Simulation (Flip) and an oracle price (Btc)
		SwapRate::set(0.000000000002_f64); // Flip will be worth $2 via swap simulation
		assert_eq!(
			Swapping::calculate_input_for_desired_output_or_default_to_zero(
				Asset::Flip,
				Asset::Btc,
				10u128.pow(8),
				false,
				false
			),
			// ~=15k + 40bps + rounding error
			(15_000 + 60) * 10u128.pow(Asset::Flip.decimals()) + 1
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
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			assert_eq!(FlipToBurn::<Test>::get(), (AMOUNT * DEFAULT_SWAP_RATE).try_into().unwrap());
			assert_swaps_queue_is_empty();
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapExecuted { .. }),);
		});
}

#[test]
fn transaction_fees_are_collected() {
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
		.then_process_blocks(SWAP_DELAY_BLOCKS)
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
	const OUTPUT_AMOUNT: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE;

	let mut total_usdc_fees: AssetAmount = 0;
	for asset in Asset::all() {
		if asset != Asset::Usdc {
			for fee_bps in FEES_BPS {
				let fee = permill_from_bps(fee_bps) * OUTPUT_AMOUNT;
				total_usdc_fees += fee;
			}
		}
	}

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: Permill::zero(), minimum: 0 });

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
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// Output is USDC so broker fees are credited directly — no broker fee swap needed.
			assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Usdc), total_usdc_fees);
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
fn minimum_network_fee_is_enforced_on_dca_swap() {
	const INPUT_AMOUNT: u128 = 3000;
	const NUMBER_OF_CHUNKS: u32 = 3;
	const CHUNK_SIZE: u128 = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;

	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_3_BLOCK: u64 = CHUNK_2_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const NETWORK_FEE: Permill = Permill::from_percent(10);
	const BROKER_FEE_BPS: u16 = 100;
	const MIN_NETWORK_FEE: u128 = 150;
	// Min network fee is larger than the fee for one chunk
	assert!(MIN_NETWORK_FEE > NETWORK_FEE * CHUNK_SIZE);
	// But smaller than the total fee across all chunks
	assert!(MIN_NETWORK_FEE < NETWORK_FEE * INPUT_AMOUNT);

	// Input amounts after network fee only (broker fee no longer deducted from input):
	const CHUNK_1_INPUT_AFTER_NET_FEE: u128 = CHUNK_SIZE - 150;
	const CHUNK_2_INPUT_AFTER_NET_FEE: u128 = CHUNK_SIZE - 50;
	const CHUNK_3_INPUT_AFTER_NET_FEE: u128 = CHUNK_SIZE - 100;

	// 1% of each chunk after network fee, 8+9+9 = 26 ArbEth.
	const TOTAL_BROKER_FEE_ARB_ETH: u128 = 26;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: MIN_NETWORK_FEE,
			});

			// Setting the swap rate to 1 so the minimum network fee is easier to calculate.
			SwapRate::set(1_f64);

			Swapping::init_swap_request(
				// 2 leg swap: Flip -> USDC -> ArbEth, swap rate is DEFAULT_SWAP_RATE per leg.
				// Fees are now taken from the input asset (Flip).
				Asset::Flip,
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
					input: AssetAndAmount {
						asset: Asset::Flip,
						amount: CHUNK_1_INPUT_AFTER_NET_FEE
					},
					network_fee: AssetAndAmount { asset: Asset::Flip, amount: 150 },
					broker_fee: AssetAndAmount { asset: Asset::ArbEth, amount: 8 },
					output,
					..
				}) if output.amount == 842
			);
		})
		.then_process_blocks_until_block(CHUNK_2_BLOCK)
		.then_execute_with(|_| {
			// Second chunk: network fee 50 from Flip input. Broker fee 1% of 950 ArbEth = 9.
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
					input: AssetAndAmount {
						asset: Asset::Flip,
						amount: CHUNK_2_INPUT_AFTER_NET_FEE
					},
					network_fee: AssetAndAmount { asset: Asset::Flip, amount: 50 },
					broker_fee: AssetAndAmount { asset: Asset::ArbEth, amount: 9 },
					output: AssetAndAmount { asset: Asset::ArbEth, amount: 941 },
					..
				})
			);
		})
		.then_process_blocks_until_block(CHUNK_3_BLOCK)
		.then_execute_with(|_| {
			// Third chunk: network fee 100 from Flip input. Broker fee 1% of 900 ArbEth = 9.
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(3),
					input: AssetAndAmount {
						asset: Asset::Flip,
						amount: CHUNK_3_INPUT_AFTER_NET_FEE
					},
					network_fee: AssetAndAmount { asset: Asset::Flip, amount: 100 },
					broker_fee: AssetAndAmount { asset: Asset::ArbEth, amount: 9 },
					output: AssetAndAmount { asset: Asset::ArbEth, amount: 891 },
					..
				})
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					amount: 2674, // 891 + 941 + 842
					..
				})
			);
			// On completion, a broker fee swap is started: TOTAL_BROKER_FEE_ARB_ETH ArbEth -> USDC.
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					input_asset: Asset::ArbEth,
					input_amount: TOTAL_BROKER_FEE_ARB_ETH,
					output_asset: Asset::Usdc,
					request_type: SwapRequestTypeEncoded::BrokerFee { account_id: BROKER },
					..
				})
			);
		});
}

#[test]
fn gas_calculation_can_handle_extreme_swap_rate() {
	new_test_ext().execute_with(|| {
		fn test_extreme_swap_rate(swap_rate: f64) {
			SwapRate::set(swap_rate);
			// Just run it and make sure it doesn't panic
			Swapping::calculate_input_for_gas_output::<Ethereum>(
				cf_chains::assets::eth::Asset::Flip,
				1000,
			);
		}

		test_extreme_swap_rate(1_f64 / (u128::MAX as f64));
		test_extreme_swap_rate(0_f64);
		test_extreme_swap_rate(u128::MAX as f64);
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
			// Set the prices and swap rate so they will not effect the minimum
			MockPriceFeedApi::set_price_usd_fine(Asset::Eth, 1);
			MockPriceFeedApi::set_price_usd_fine(Asset::Usdc, 1);
			SwapRate::set(1_f64);

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
	const INPUT_AMOUNT: AssetAmount = 1000;
	const FEE_RATE_FLIP: Permill = Permill::from_percent(10);
	const FEE_RATE_ETH: Permill = Permill::from_percent(5);
	const NETWORK_FEE: Permill = Permill::from_percent(1);

	// The higher of the two custom fees is used. Fee is taken from input asset (Flip).
	const EXPECTED_FEE: AssetAmount =
		FEE_RATE_FLIP.deconstruct() as u128 * INPUT_AMOUNT / 1_000_000;
	const INPUT_AFTER_FEE: AssetAmount = INPUT_AMOUNT - EXPECTED_FEE;
	let expected_fee = FEE_RATE_FLIP * INPUT_AMOUNT;

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
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// Fee is taken from input (Flip), then the remainder is swapped.
			// With swap rate 1, output = input - fee.
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					input: AssetAndAmount { asset: Asset::Flip, amount: INPUT_AFTER_FEE },
					network_fee,
					output,
					..
				}) if *network_fee == AssetAndAmount { asset: Asset::Flip, amount: expected_fee }
					&& output.amount == INPUT_AMOUNT - expected_fee
			);
		});
}

#[test]
fn network_fee_minimum_exceeds_input_amount() {
	const INPUT_ASSET: Asset = Asset::Flip;
	const OUTPUT_ASSET: Asset = Asset::Usdc;
	const INPUT_AMOUNT: AssetAmount = 500;
	// Minimum is larger than the entire input amount.
	const NETWORK_FEE_MINIMUM: AssetAmount = INPUT_AMOUNT * 2;

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::from_percent(1),
				minimum: NETWORK_FEE_MINIMUM,
			});

			SwapRate::set(1_f64);

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
				Default::default(),
				None,
				None,
				SwapOrigin::Vault {
					tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(BROKER),
				},
			);
		})
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// Network fee is capped at the full input amount; nothing remains to swap.
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					network_fee: AssetAndAmount { asset: INPUT_ASSET, amount: INPUT_AMOUNT },
					input: AssetAndAmount { asset: INPUT_ASSET, amount: 0 },
					output: AssetAndAmount { asset: OUTPUT_ASSET, amount: 0 },
					..
				})
			);

			// The full input amount is recorded as collected network fee.
			assert_eq!(get_collected_network_fee(INPUT_ASSET), INPUT_AMOUNT);

			// No egress is scheduled because the output is zero.
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapEgressIgnored {
					asset: OUTPUT_ASSET,
					amount: 0,
					..
				})
			);
			assert!(MockEgressHandler::<AnyChain>::get_scheduled_egresses().is_empty());
		});
}

#[test]
fn test_network_fee_tracking_when_rescheduled() {
	const INPUT_ASSET: Asset = Asset::Flip;
	const OUTPUT_ASSET: Asset = Asset::Usdc;
	const INPUT_AMOUNT: AssetAmount = 1_000;
	const NETWORK_FEE: Permill = Permill::from_percent(1);
	// Set a minimum network fee that will be enforced (taken from input asset).
	const NETWORK_FEE_MINIMUM: AssetAmount = 100;
	const BROKER_FEE_BPS: u16 = 50;

	const INPUT_AFTER_NET_FEE: AssetAmount = INPUT_AMOUNT - NETWORK_FEE_MINIMUM;
	const EXPECTED_OUTPUT_BEFORE_BROKER_FEE: AssetAmount = INPUT_AFTER_NET_FEE * 4;
	const EXPECTED_BROKER_FEE: AssetAmount =
		EXPECTED_OUTPUT_BEFORE_BROKER_FEE * BROKER_FEE_BPS as u128 / 10_000;

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE,
				minimum: NETWORK_FEE_MINIMUM,
			});

			// Set swap rate to 1 to make minimum network fee calculations easier
			SwapRate::set(1_f64);

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
				vec![Beneficiary { account: BROKER, bps: BROKER_FEE_BPS }].try_into().unwrap(),
				Some(PriceLimitsAndExpiry {
					expiry_behaviour: ExpiryBehaviour::NoExpiry,
					// Setting a min price that will trigger a reschedule
					min_price: Price::from_usd_fine_amount(2),
					max_oracle_price_slippage: None,
				}),
				None,
				SwapOrigin::Internal,
			);

			assert_eq!(get_collected_network_fee(INPUT_ASSET), 0);
		})
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// Check that the swap was rescheduled due to price limits
			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapRescheduled { .. }),);

			// Check that no network fee was taken yet
			assert_eq!(get_collected_network_fee(INPUT_ASSET), 0);

			// Change the swap rate so that the swap can proceed next try
			SwapRate::set(4_f64);
		})
		.then_process_blocks(DEFAULT_SWAP_RETRY_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// Network fee (minimum) taken from Flip input. Broker fee taken from USDC output.
			const EXPECTED_OUTPUT_AMOUNT: AssetAmount =
				EXPECTED_OUTPUT_BEFORE_BROKER_FEE - EXPECTED_BROKER_FEE;
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					input: AssetAndAmount { asset: INPUT_ASSET, amount: INPUT_AFTER_NET_FEE },
					network_fee: AssetAndAmount { asset: INPUT_ASSET, amount: NETWORK_FEE_MINIMUM },
					broker_fee: AssetAndAmount { asset: OUTPUT_ASSET, amount: EXPECTED_BROKER_FEE },
					output: AssetAndAmount { asset: OUTPUT_ASSET, amount: EXPECTED_OUTPUT_AMOUNT },
					..
				})
			);

			assert_eq!(get_collected_network_fee(INPUT_ASSET), NETWORK_FEE_MINIMUM);
		});
}

/// Verify that broker fees on swaps to assets that are not USDC are automatically converted to USDC
/// via secondary swaps, and that events carry the correct amounts and swap request IDs.
/// Two brokers are used to confirm each beneficiary gets their own swap and correct credit.
#[test]
fn broker_fees_are_swapped_to_usdc() {
	const INPUT_ASSET: Asset = Asset::Usdc;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	const INPUT_AMOUNT: AssetAmount = 10_000;

	// Two beneficiaries with different fee rates.
	const ALICE_FEE_BPS: BasisPoints = 200; // 2%
	const BROKER_FEE_BPS: BasisPoints = 100; // 1%

	const OUTPUT_BEFORE_BROKER_FEES: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE;
	// Each beneficiary's fee is computed independently from the pre-fee output.
	// 2% of 20_000 = 400, 1% of 20_000 = 200 (both exact, no rounding).
	const ALICE_FEE: AssetAmount = OUTPUT_BEFORE_BROKER_FEES * ALICE_FEE_BPS as u128 / 10_000;
	const BROKER_FEE: AssetAmount = OUTPUT_BEFORE_BROKER_FEES * BROKER_FEE_BPS as u128 / 10_000;
	const TOTAL_BROKER_FEE: AssetAmount = ALICE_FEE + BROKER_FEE;
	const USER_OUTPUT: AssetAmount = OUTPUT_BEFORE_BROKER_FEES - TOTAL_BROKER_FEE;
	// Broker fee swaps: FLIP -> USDC, 1-leg, output = fee * DEFAULT_SWAP_RATE
	const ALICE_FEE_USDC: AssetAmount = ALICE_FEE * DEFAULT_SWAP_RATE;
	const BROKER_FEE_USDC: AssetAmount = BROKER_FEE * DEFAULT_SWAP_RATE;

	const MAIN_SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(1);
	// ALICE sorts before BROKER, so her swap is initiated first.
	const ALICE_FEE_SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(2);
	const BROKER_FEE_SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(3);
	const MAIN_SWAP_ID: SwapId = SwapId(1);
	const ALICE_FEE_SWAP_ID: SwapId = SwapId(2);
	const BROKER_FEE_SWAP_ID: SwapId = SwapId(3);

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: Permill::zero(), minimum: 0 });

			swap_with_custom_broker_fee(
				INPUT_ASSET,
				OUTPUT_ASSET,
				INPUT_AMOUNT,
				bounded_vec![
					Beneficiary { account: ALICE, bps: ALICE_FEE_BPS },
					Beneficiary { account: BROKER, bps: BROKER_FEE_BPS },
				],
			);
		})
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// The main swap executes
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_request_id: MAIN_SWAP_REQUEST_ID,
					swap_id: MAIN_SWAP_ID,
					broker_fee: AssetAndAmount { asset: OUTPUT_ASSET, amount: TOTAL_BROKER_FEE },
					output: AssetAndAmount { asset: OUTPUT_ASSET, amount: USER_OUTPUT },
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapEgressScheduled {
					swap_request_id: MAIN_SWAP_REQUEST_ID,
					amount: USER_OUTPUT,
					..
				}),
				// ALICE's fee swap (sorted first by account ID).
				RuntimeEvent::Swapping(Event::<Test>::SwapRequested {
					swap_request_id: ALICE_FEE_SWAP_REQUEST_ID,
					input_asset: OUTPUT_ASSET,
					input_amount: ALICE_FEE,
					output_asset: Asset::Usdc,
					request_type: SwapRequestTypeEncoded::BrokerFee { account_id: ALICE },
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
					swap_request_id: ALICE_FEE_SWAP_REQUEST_ID,
					swap_id: ALICE_FEE_SWAP_ID,
					..
				}),
				// BROKER's fee swap
				RuntimeEvent::Swapping(Event::<Test>::SwapRequested {
					swap_request_id: BROKER_FEE_SWAP_REQUEST_ID,
					input_asset: OUTPUT_ASSET,
					input_amount: BROKER_FEE,
					output_asset: Asset::Usdc,
					request_type: SwapRequestTypeEncoded::BrokerFee { account_id: BROKER },
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
					swap_request_id: BROKER_FEE_SWAP_REQUEST_ID,
					swap_id: BROKER_FEE_SWAP_ID,
					..
				}),
				// The main swap request completes with the broker fee swap request IDs
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: MAIN_SWAP_REQUEST_ID,
					ref broker_fee_swaps,
					..
				}) if *broker_fee_swaps == BTreeMap::from([
					(ALICE, ALICE_FEE_SWAP_REQUEST_ID),
					(BROKER, BROKER_FEE_SWAP_REQUEST_ID),
				]),
			);
		})
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// Both broker fee swaps execute in SwapId order
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_request_id: ALICE_FEE_SWAP_REQUEST_ID,
					swap_id: ALICE_FEE_SWAP_ID,
					input: AssetAndAmount { asset: OUTPUT_ASSET, amount: ALICE_FEE },
					output: AssetAndAmount { asset: Asset::Usdc, amount: ALICE_FEE_USDC },
					broker_fee: AssetAndAmount { asset: Asset::Usdc, amount: 0 },
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: ALICE_FEE_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_request_id: BROKER_FEE_SWAP_REQUEST_ID,
					swap_id: BROKER_FEE_SWAP_ID,
					input: AssetAndAmount { asset: OUTPUT_ASSET, amount: BROKER_FEE },
					output: AssetAndAmount { asset: Asset::Usdc, amount: BROKER_FEE_USDC },
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: BROKER_FEE_SWAP_REQUEST_ID,
					..
				}),
			);

			// Each broker's USDC balance is credited with their respective fee swap output.
			assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Usdc), ALICE_FEE_USDC);
			assert_eq!(get_broker_balance::<Test>(&ALICE, OUTPUT_ASSET), 0);
			assert_eq!(get_broker_balance::<Test>(&BROKER, Asset::Usdc), BROKER_FEE_USDC);
		});
}

/// When the output asset is USDC, broker fees are credited directly to the broker's free
/// balance — no broker fee swap is created.
#[test]
fn broker_fees_credited_directly_when_output_is_usdc() {
	const INPUT_ASSET: Asset = Asset::Flip;
	const OUTPUT_ASSET: Asset = Asset::Usdc;
	const INPUT_AMOUNT: AssetAmount = 10_000;
	const BROKER_FEE_BPS: BasisPoints = 100; // 1%

	// No network fee to keep calculations simple.
	const OUTPUT_BEFORE_BROKER_FEE: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE;
	const BROKER_FEE: AssetAmount = OUTPUT_BEFORE_BROKER_FEE / 100;

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum { rate: Permill::zero(), minimum: 0 });

			swap_with_custom_broker_fee(
				INPUT_ASSET,
				OUTPUT_ASSET,
				INPUT_AMOUNT,
				bounded_vec![Beneficiary { account: BROKER, bps: BROKER_FEE_BPS }],
			);
		})
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			// Confirm the main swap executed and the broker fee was taken.
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					broker_fee: AssetAndAmount { asset: OUTPUT_ASSET, amount: BROKER_FEE },
					..
				})
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					ref broker_fee_swaps,
					..
					// Make sure the event also confirms that no broker fee swaps were created.
				}) if broker_fee_swaps.is_empty(),
			);

			// No broker fee swap was ever created.
			assert_no_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					request_type: SwapRequestTypeEncoded::BrokerFee { .. },
					..
				})
			);

			// The broker's USDC balance is credited directly with the fee amount.
			assert_eq!(get_broker_balance::<Test>(&BROKER, Asset::Usdc), BROKER_FEE);
		});
}

/// Make sure the network fee is deducted even when a swap fails on the first chunk.
#[test]
fn minimum_network_fee_is_deducted_from_refund_amount() {
	const INPUT_ASSET: Asset = Asset::Usdc;

	// Rate-based fee on the full amount is less than the minimum, so the minimum is enforced.
	const NETWORK_FEE_RATE: Permill = Permill::from_percent(1);
	const NETWORK_FEE_MINIMUM: AssetAmount = INPUT_AMOUNT / 2;
	assert!(NETWORK_FEE_RATE * INPUT_AMOUNT < NETWORK_FEE_MINIMUM);

	// The refund amount is the input minus the minimum network fee.
	const EXPECTED_REFUND_AMOUNT: AssetAmount = INPUT_AMOUNT - NETWORK_FEE_MINIMUM;

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: NETWORK_FEE_RATE,
				minimum: NETWORK_FEE_MINIMUM,
			});

			// Use a min_output that can never be satisfied so the swap is refunded.
			insert_swaps(
				&[TestSwapParams::new(
					None,
					Some(TestRefundParams { retry_duration: 0, min_output: AssetAmount::MAX }),
					false,
				)],
				None,
			);
		})
		.then_process_blocks(SWAP_DELAY_BLOCKS)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					asset: INPUT_ASSET,
					amount: EXPECTED_REFUND_AMOUNT,
					..
				})
			);

			// The minimum network fee was collected.
			assert_eq!(get_collected_network_fee(INPUT_ASSET), NETWORK_FEE_MINIMUM);
		});
}
