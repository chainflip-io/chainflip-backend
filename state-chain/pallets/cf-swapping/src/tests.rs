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

mod ccm;
mod config;
mod dca;
mod fees;
mod fill_or_kill;

use std::sync::LazyLock;

use super::*;
use crate::{
	CollectedRejectedFunds, Error, Event, MaximumSwapAmount, Pallet, Swap, SwapOrigin, SwapType,
	mock::{RuntimeEvent, *},
};
use cf_amm_math::Price;
use cf_chains::{
	self, AnyChain, CcmChannelMetadata, CcmChannelMetadataUnchecked, CcmDepositMetadata,
	CcmDepositMetadataUnchecked, Ethereum, TransactionInIdForAnyChain,
	address::{AddressConverter, EncodedAddress, ForeignChainAddress},
	dot::PolkadotAccountId,
	evm::H256,
};
use cf_primitives::{
	Asset, AssetAmount, BasisPoints, Beneficiary, BlockNumber, DcaParameters, ForeignChain,
};
use cf_test_utilities::{assert_event_sequence, assert_has_matching_event};
use cf_traits::{
	AccountRoleRegistry, AssetConverter, Chainflip, INITIAL_FLIP_FUNDING, SetSafeMode,
	SwapExecutionProgress,
	mocks::{
		address_converter::MockAddressConverter,
		balance_api::MockBalance,
		egress_handler::{MockEgressHandler, MockEgressParameter},
		funding_info::MockFundingInfo,
		ingress_egress_fee_handler::MockIngressEgressFeeHandler,
		pool_price_api::MockPoolPriceApi,
		price_feed_api::MockPriceFeedApi,
	},
};
use frame_support::{
	assert_noop, assert_ok,
	testing_prelude::bounded_vec,
	traits::{Hooks, OriginTrait},
};

use sp_arithmetic::Permill;
use sp_core::{H160, U256};
use sp_runtime::SaturatedConversion;
use sp_std::iter;

const GAS_BUDGET: AssetAmount = 100_000u128;
const INPUT_AMOUNT: AssetAmount = 40_000;
const SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(1);
const INIT_BLOCK: u64 = 1;
const BROKER_FEE_BPS: u16 = 10;
const INPUT_ASSET: Asset = Asset::Usdc;
const OUTPUT_ASSET: Asset = Asset::Usdt;

static EVM_OUTPUT_ADDRESS: LazyLock<ForeignChainAddress> =
	LazyLock::new(|| ForeignChainAddress::Eth([1; 20].into()));

fn set_maximum_swap_amount(asset: Asset, amount: Option<AssetAmount>) {
	assert_ok!(Swapping::update_pallet_config(
		OriginTrait::root(),
		vec![PalletConfigUpdate::MaximumSwapAmount { asset, amount }]
			.try_into()
			.unwrap()
	));
}

struct TestSwapParams {
	input_asset: Asset,
	output_asset: Asset,
	input_amount: AssetAmount,
	price_limits_and_expiry: Option<PriceLimitsAndExpiry<u64>>,
	dca_params: Option<DcaParameters>,
	output_address: ForeignChainAddress,
	is_ccm: bool,
}

impl TestSwapParams {
	fn new(
		dca_params: Option<DcaParameters>,
		refund_params: Option<TestRefundParams>,
		is_ccm: bool,
	) -> TestSwapParams {
		TestSwapParams {
			input_asset: INPUT_ASSET,
			output_asset: OUTPUT_ASSET,
			input_amount: INPUT_AMOUNT,
			price_limits_and_expiry: refund_params
				.map(|params| params.into_extended_params(INPUT_AMOUNT)),
			dca_params,
			output_address: (*EVM_OUTPUT_ADDRESS).clone(),
			is_ccm,
		}
	}
}

// Convenience struct used in tests allowing to specify refund parameters
// with min output rather than min price:
#[derive(Debug, Clone)]
struct TestRefundParams {
	retry_duration: BlockNumber,
	min_output: AssetAmount,
}

impl TestRefundParams {
	/// Due to rounding errors, you may have to set the `min_output` to a value one unit higher than
	/// expected.
	fn into_extended_params(self, input_amount: AssetAmount) -> PriceLimitsAndExpiry<u64> {
		PriceLimitsAndExpiry {
			expiry_behaviour: ExpiryBehaviour::RefundIfExpires {
				retry_duration: self.retry_duration,
				refund_address: AccountOrAddress::ExternalAddress(ForeignChainAddress::Eth(
					[10; 20].into(),
				)),
				refund_ccm_metadata: None,
			},
			min_price: Price::from_amounts_bounded(self.min_output.into(), input_amount.into()),
			max_oracle_price_slippage: None,
		}
	}
}

/// Creates a test swap and corresponding swap request. Both use the same ID and no fees
fn create_test_swap(
	id: u64,
	input_asset: Asset,
	output_asset: Asset,
	amount: AssetAmount,
	dca_params: Option<DcaParameters>,
	execute_at: u64,
) -> Swap<Test> {
	let mut dca_state = DcaState::new(amount, dca_params);
	dca_state.record_scheduled_chunk(id.into(), amount);

	SwapRequests::<Test>::insert(
		SwapRequestId::from(id),
		SwapRequest {
			id: id.into(),
			input_asset,
			output_asset,
			state: SwapRequestState::UserSwap {
				price_limits_and_expiry: None,
				output_action: SwapOutputAction::Egress {
					ccm_deposit_metadata: None,
					output_address: ForeignChainAddress::Eth(H160::zero()),
				},
				dca_state,
				fees: Default::default(),
			},
		},
	);

	Swap::new(id.into(), id.into(), input_asset, output_asset, amount, None, execute_at)
}

// Returns some test data
fn generate_test_swaps() -> Vec<TestSwapParams> {
	vec![
		// asset -> USDC (1-leg, cross-decimal)
		TestSwapParams {
			input_asset: Asset::Flip,
			output_asset: Asset::Usdc,
			input_amount: 100 * 10u128.pow(18),
			price_limits_and_expiry: None,
			dca_params: None,
			output_address: ForeignChainAddress::Eth([2; 20].into()),
			is_ccm: false,
		},
		// Another asset -> USDC (1-leg, cross-decimal)
		TestSwapParams {
			input_asset: Asset::Eth,
			output_asset: Asset::Usdc,
			input_amount: 40 * 10u128.pow(18),
			price_limits_and_expiry: None,
			dca_params: None,
			output_address: ForeignChainAddress::Eth([9; 20].into()),
			is_ccm: false,
		},
		// 2-leg swap, same decimals (Flip -> Eth via USDC)
		TestSwapParams {
			input_asset: Asset::Flip,
			output_asset: Asset::Eth,
			input_amount: 500 * 10u128.pow(18),
			price_limits_and_expiry: None,
			dca_params: None,
			output_address: ForeignChainAddress::Eth([2; 20].into()),
			is_ccm: false,
		},
		// 2-leg cross-chain cross-decimal (Flip -> Dot, 18->10 dec)
		TestSwapParams {
			input_asset: Asset::Flip,
			output_asset: Asset::Dot,
			input_amount: 600 * 10u128.pow(18),
			price_limits_and_expiry: None,
			dca_params: None,
			output_address: ForeignChainAddress::Dot(PolkadotAccountId::from_aliased([4; 32])),
			is_ccm: false,
		},
	]
}

fn insert_swaps(swaps: &[TestSwapParams]) {
	for (broker_id, swap) in swaps.iter().enumerate() {
		let ccm_deposit_metadata = if swap.is_ccm {
			Some(
				generate_ccm_deposit()
					.to_checked(swap.output_asset, swap.output_address.clone())
					.unwrap(),
			)
		} else {
			None
		};

		let request_type = SwapRequestType::Regular {
			output_action: SwapOutputAction::Egress {
				ccm_deposit_metadata,
				output_address: swap.output_address.clone(),
			},
		};

		Swapping::init_swap_request(
			swap.input_asset,
			swap.input_amount,
			swap.output_asset,
			request_type,
			bounded_vec![Beneficiary { account: broker_id as u64, bps: BROKER_FEE_BPS }],
			swap.price_limits_and_expiry.clone(),
			swap.dca_params.clone(),
			SwapOrigin::Vault {
				tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
				broker_id: Some(BROKER),
			},
		);
	}
}

fn generate_ccm_channel() -> CcmChannelMetadataUnchecked {
	CcmChannelMetadata {
		message: vec![0x01].try_into().unwrap(),
		gas_budget: GAS_BUDGET,
		ccm_additional_data: Default::default(),
	}
}
fn generate_ccm_deposit() -> CcmDepositMetadataUnchecked<ForeignChainAddress> {
	CcmDepositMetadata {
		source_chain: ForeignChain::Ethereum,
		source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
		channel_metadata: generate_ccm_channel(),
	}
}

const REFUND_PARAMS: ChannelRefundParametersUncheckedEncoded =
	ChannelRefundParametersUncheckedEncoded {
		retry_duration: 100,
		refund_address: EncodedAddress::Eth([1; 20]),
		min_price: Price::zero(),
		max_oracle_price_slippage: None,
		refund_ccm_metadata: None,
	};

fn get_broker_balance<T: Config>(who: &T::AccountId, asset: Asset) -> AssetAmount {
	T::BalanceApi::get_balance(who, asset)
}

#[track_caller]
fn assert_swaps_queue_is_empty() {
	assert!(ScheduledSwaps::<Test>::get().is_empty());
}

#[track_caller]
fn swap_with_custom_broker_fee(
	from: Asset,
	to: Asset,
	amount: AssetAmount,
	broker_fees: Beneficiaries<u64>,
) {
	Swapping::init_swap_request(
		from,
		amount,
		to,
		SwapRequestType::Regular {
			output_action: SwapOutputAction::Egress {
				output_address: ForeignChainAddress::Eth(Default::default()),
				ccm_deposit_metadata: None,
			},
		},
		broker_fees,
		None,
		None,
		SwapOrigin::DepositChannel {
			deposit_address: MockAddressConverter::to_encoded_address(ForeignChainAddress::Eth(
				[0; 20].into(),
			)),
			channel_id: 1,
			deposit_block_height: 0,
			broker_id: BROKER,
		},
	);
}

/// Compute expected single-leg mock swap output for any asset pair.
/// Must be called inside test externalities (reads SwapRate).
fn mock_swap_output(from: Asset, to: Asset, input: AssetAmount) -> AssetAmount {
	let asset = if from == STABLE_ASSET { to } else { from };
	let price = Price::from_usd(asset, SwapRate::get());
	if price.is_zero() {
		return 0;
	}
	let rate = if from == STABLE_ASSET { price.invert() } else { price };
	rate.output_amount_floor(input).saturated_into()
}

#[track_caller]
fn get_scheduled_swap_block(swap_id: SwapId) -> Option<BlockNumberFor<Test>> {
	ScheduledSwaps::<Test>::get().get(&swap_id).map(|swap| swap.execute_at)
}

#[test]
fn request_swap_success_with_valid_parameters() {
	new_test_ext().execute_with(|| {
		assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
			RuntimeOrigin::signed(BROKER),
			Asset::Eth,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			0,
			None,
			0,
			Default::default(),
			REFUND_PARAMS,
			None,
		));
	});
}

#[test]
fn process_all_swaps() {
	new_test_ext().execute_with(|| {
		let swaps = generate_test_swaps();
		insert_swaps(&swaps);
		Swapping::on_finalize(System::block_number() + SWAP_DELAY_BLOCKS as u64);

		assert_swaps_queue_is_empty();
		let mut expected = swaps
			.iter()
			.map(|swap| {
				let output_amount = if swap.input_asset == STABLE_ASSET {
					let fee = swap.input_amount * BROKER_FEE_BPS as u128 / 10_000;
					mock_swap_output(STABLE_ASSET, swap.output_asset, swap.input_amount - fee)
				} else if swap.output_asset == STABLE_ASSET {
					let output_before_fee =
						mock_swap_output(swap.input_asset, STABLE_ASSET, swap.input_amount);
					let fee = output_before_fee * BROKER_FEE_BPS as u128 / 10_000;
					output_before_fee - fee
				} else {
					let intermediate =
						mock_swap_output(swap.input_asset, STABLE_ASSET, swap.input_amount);
					let fee = intermediate * BROKER_FEE_BPS as u128 / 10_000;
					mock_swap_output(STABLE_ASSET, swap.output_asset, intermediate - fee)
				};

				MockEgressParameter::<AnyChain>::Swap {
					asset: swap.output_asset,
					amount: output_amount,
					destination_address: swap.output_address.clone(),
					fee: 0,
				}
			})
			.collect::<Vec<_>>();
		expected.sort();
		let mut egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
		egresses.sort();
		assert_eq!(expected.len(), egresses.len());
		for (expected, actual) in iter::zip(expected, egresses) {
			match (&expected, &actual) {
				(
					MockEgressParameter::Swap { amount: expected_amount, .. },
					MockEgressParameter::Swap { amount: actual_amount, .. },
				) => {
					// Allow 1 bps tolerance for fixed-point rounding in cross-decimal swaps
					let tolerance = (*expected_amount / 10_000).max(1);
					assert!(
						expected_amount.abs_diff(*actual_amount) <= tolerance,
						"Amount mismatch (tolerance {tolerance}): expected {expected_amount}, got {actual_amount}"
					);
				},
				_ => assert_eq!(expected, actual),
			}
		}
	});
}

#[test]
#[should_panic]
fn cannot_swap_with_incorrect_destination_address_type() {
	new_test_ext().execute_with(|| {
		Swapping::init_swap_request(
			Asset::Eth,
			10,
			Asset::Dot,
			SwapRequestType::Regular {
				output_action: SwapOutputAction::Egress {
					output_address: ForeignChainAddress::Eth([2; 20].into()),
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

		assert_swaps_queue_is_empty();
	});
}

#[test]
fn affiliates_with_0_bps_and_swap_id_are_getting_emitted_in_events() {
	const AMOUNT: AssetAmount = 500 * 10u128.pow(18);

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			let beneficiaries: Beneficiaries<u64> = bounded_vec![
				Beneficiary { account: BROKER, bps: 0 },
				Beneficiary { account: 123, bps: 0 },
			];

			let affiliates: Affiliates<u64> = bounded_vec![Beneficiary { account: 123, bps: 0 },];

			// 1. Request a deposit address -> SwapDepositAddressReady
			assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
				RuntimeOrigin::signed(BROKER),
				Asset::Eth,
				Asset::Usdc,
				EncodedAddress::Eth(Default::default()),
				0,
				None,
				0,
				affiliates.clone(),
				REFUND_PARAMS,
				None,
			));

			// 2. Schedule the swap -> SwapScheduled
			swap_with_custom_broker_fee(Asset::Eth, Asset::Usdc, AMOUNT, beneficiaries.clone());

			// 3. Process swaps -> SwapExecuted, SwapEgressScheduled
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapDepositAddressReady {
					deposit_address: EncodedAddress::Eth(..),
					destination_address: EncodedAddress::Eth(..),
					source_asset: Asset::Eth,
					destination_asset: Asset::Usdc,
					channel_id: 0,
					ref affiliate_fees,
					..
				}) if *affiliate_fees == affiliates,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SwapRequestId(1),
					ref broker_fees,
					..
				}) if *broker_fees == beneficiaries,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SwapRequestId(1),
					swap_id: SwapId(1),
					input_amount: AMOUNT,
					..
				})
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(1),
					egress_id: (ForeignChain::Ethereum, 1),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1),
					reason: SwapRequestCompletionReason::Executed
				}),
			);
		});
}

#[test]
fn rejects_invalid_swap_deposit() {
	new_test_ext().execute_with(|| {
		let ccm = generate_ccm_channel();

		assert_noop!(
			Swapping::request_swap_deposit_address_with_affiliates(
				RuntimeOrigin::signed(BROKER),
				Asset::Btc,
				Asset::Eth,
				EncodedAddress::Dot(Default::default()),
				0,
				Some(ccm.clone()),
				0,
				Default::default(),
				REFUND_PARAMS,
				None,
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);

		assert_noop!(
			Swapping::request_swap_deposit_address_with_affiliates(
				RuntimeOrigin::signed(BROKER),
				Asset::Eth,
				Asset::Dot,
				EncodedAddress::Dot(Default::default()),
				0,
				Some(ccm),
				0,
				Default::default(),
				REFUND_PARAMS,
				None,
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);
	});
}

#[test]
fn swap_by_deposit_happy_path() {
	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	const AMOUNT: AssetAmount = 1_000u128;

	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	new_test_ext()
		.execute_with(|| {
			swap_with_custom_broker_fee(INPUT_ASSET, OUTPUT_ASSET, AMOUNT, bounded_vec![]);

			// Verify this swap is accepted and scheduled
			assert_eq!(
				ScheduledSwaps::<Test>::get(),
				BTreeMap::from([(
					1.into(),
					Swap::new(
						1.into(),
						1.into(),
						INPUT_ASSET,
						OUTPUT_ASSET,
						AMOUNT,
						None,
						SWAP_BLOCK
					)
				)])
			);

			assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
				swap_request_id: 1.into(),
				swap_id: 1.into(),
				input_amount: AMOUNT,
				swap_type: SwapType::Swap,
				execute_at: SWAP_BLOCK,
			}));
		})
		.then_process_blocks_until_block(SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			// Confiscated fund is unchanged
			assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
		});
}

#[test]
fn process_all_into_stable_swaps_first() {
	const SWAP_EXECUTION_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const AMOUNT: AssetAmount = 10u128.pow(18);
	new_test_ext().execute_with(|| {
		const NETWORK_FEE_RATE: Permill = Permill::from_parts(100);
		const NETWORK_FEE: FeeRateAndMinimum =
			FeeRateAndMinimum { rate: NETWORK_FEE_RATE, minimum: 0 };
		NetworkFee::<Test>::set(NETWORK_FEE);

		[Asset::Flip, Asset::Btc, Asset::Dot, Asset::Usdc]
			.into_iter()
			.for_each(|input_asset| {
				Swapping::init_swap_request(
					input_asset,
					AMOUNT,
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
			});

		assert_eq!(
			ScheduledSwaps::<Test>::get(),
			BTreeMap::from([
				(
					1.into(),
					Swap::new(
						1.into(),
						1.into(),
						Asset::Flip,
						Asset::Eth,
						AMOUNT,
						None,
						SWAP_EXECUTION_BLOCK
					),
				),
				(
					2.into(),
					Swap::new(
						2.into(),
						2.into(),
						Asset::Btc,
						Asset::Eth,
						AMOUNT,
						None,
						SWAP_EXECUTION_BLOCK
					)
				),
				(
					3.into(),
					Swap::new(
						3.into(),
						3.into(),
						Asset::Dot,
						Asset::Eth,
						AMOUNT,
						None,
						SWAP_EXECUTION_BLOCK
					),
				),
				(
					4.into(),
					Swap::new(
						4.into(),
						4.into(),
						Asset::Usdc,
						Asset::Eth,
						AMOUNT,
						None,
						SWAP_EXECUTION_BLOCK
					)
				)
			])
		);

		System::reset_events();
		// All of the swaps in the ScheduledSwaps queue are executed.
		Swapping::on_finalize(SWAP_EXECUTION_BLOCK);
		assert_swaps_queue_is_empty();

		// Non-USDC inputs produce different USDC intermediates (before network fee):
		let flip_usdc_raw = mock_swap_output(Asset::Flip, STABLE_ASSET, AMOUNT);
		let btc_usdc_raw = mock_swap_output(Asset::Btc, STABLE_ASSET, AMOUNT);
		let dot_usdc_raw = mock_swap_output(Asset::Dot, STABLE_ASSET, AMOUNT);

		// Network fee is taken on each intermediate USDC amount:
		let flip_usdc = flip_usdc_raw - NETWORK_FEE_RATE * flip_usdc_raw;
		let btc_usdc = btc_usdc_raw - NETWORK_FEE_RATE * btc_usdc_raw;
		let dot_usdc = dot_usdc_raw - NETWORK_FEE_RATE * dot_usdc_raw;
		// Swap 4 (Usdc→Eth): USDC is input, fee taken directly on AMOUNT
		let usdc_direct = AMOUNT - NETWORK_FEE_RATE * AMOUNT;
		let total_usdc = flip_usdc + btc_usdc + dot_usdc + usdc_direct;

		// Verify swap "from" -> STABLE_ASSET, then "to" -> Output Asset
		// into-stable legs execute first (sorted by swap ID), then one combined from-stable leg
		assert_eq!(
			Swaps::get(),
			vec![
				(Asset::Flip, Asset::Usdc, AMOUNT),
				(Asset::Dot, Asset::Usdc, AMOUNT),
				(Asset::Btc, Asset::Usdc, AMOUNT),
				(Asset::Usdc, Asset::Eth, total_usdc),
			]
		);

		// Each swap request produces a different amount of Eth:
		let eth_from_flip = mock_swap_output(STABLE_ASSET, Asset::Eth, flip_usdc);
		let eth_from_btc = mock_swap_output(STABLE_ASSET, Asset::Eth, btc_usdc);
		let eth_from_dot = mock_swap_output(STABLE_ASSET, Asset::Eth, dot_usdc);
		let eth_from_usdc = mock_swap_output(STABLE_ASSET, Asset::Eth, usdc_direct);

		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_request_id: SwapRequestId(1),
				egress_id: (ForeignChain::Ethereum, 1),
				amount,
				..
			}) if amount == eth_from_flip,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(2), .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_request_id: SwapRequestId(2),
				egress_id: (ForeignChain::Ethereum, 2),
				amount,
				..
			}) if amount == eth_from_btc,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(3), .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_request_id: SwapRequestId(3),
				egress_id: (ForeignChain::Ethereum, 3),
				amount,
				..
			}) if amount == eth_from_dot,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(4), .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_request_id: SwapRequestId(4),
				egress_id: (ForeignChain::Ethereum, 4),
				amount,
				..
			}) if amount == eth_from_usdc,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
		);
	});
}

#[test]
fn can_handle_ccm_with_zero_swap_outputs() {
	const PRINCIPAL_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const PRINCIPAL_AMOUNT: AssetAmount = 9000;

	// Note: we use a constant to make sure we don't accidentally change the value
	const ZERO_AMOUNT: AssetAmount = 0;

	new_test_ext()
		.execute_with(|| {
			let eth_address = ForeignChainAddress::Eth(Default::default());
			let ccm = generate_ccm_deposit().to_checked(OUTPUT_ASSET, eth_address.clone()).unwrap();

			Swapping::init_swap_request(
				INPUT_ASSET,
				PRINCIPAL_AMOUNT,
				OUTPUT_ASSET,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::Egress {
						ccm_deposit_metadata: Some(ccm.clone()),
						output_address: eth_address,
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

			// Change the swap rate so swap output will be 0
			SwapRate::set(0);
			System::reset_events();
		})
		.then_process_blocks_until_block(PRINCIPAL_SWAP_BLOCK)
		.then_execute_with(|_| {
			// Swap outputs are zero
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_request_id: SwapRequestId(1),
					swap_id: SwapId(1),
					network_fee: 0,
					broker_fee: 0,
					input_amount: PRINCIPAL_AMOUNT,
					input_asset: INPUT_ASSET,
					output_asset: OUTPUT_ASSET,
					output_amount: ZERO_AMOUNT,
					intermediate_amount: None,
					oracle_delta: None,
				}),
			);
		})
		.then_execute_with(|_| {
			// CCM are processed and egressed even if principal output is zero.
			assert_eq!(MockEgressHandler::<AnyChain>::get_scheduled_egresses().len(), 1);
			assert_swaps_queue_is_empty();
		});
}

#[test]
fn can_handle_swaps_with_zero_outputs() {
	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			swap_with_custom_broker_fee(Asset::Usdc, Asset::Eth, 100, bounded_vec![]);
			swap_with_custom_broker_fee(Asset::Usdc, Asset::Eth, 1, bounded_vec![]);

			// Change the swap rate so swap output will be 0
			SwapRate::set(0);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			// Swap outputs are zero
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_id: SwapId(1),
					output_asset: Asset::Eth,
					output_amount: 0,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressIgnored {
					swap_request_id: SwapRequestId(1),
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1),
					reason: SwapRequestCompletionReason::Executed
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_id: SwapId(2),
					output_asset: Asset::Eth,
					output_amount: 0,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressIgnored {
					swap_request_id: SwapRequestId(2),
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(2),
					reason: SwapRequestCompletionReason::Executed
				}),
			);

			// Swaps are not egressed when output is 0.
			assert_swaps_queue_is_empty();
			assert!(
				MockEgressHandler::<AnyChain>::get_scheduled_egresses().is_empty(),
				"No egresses should be scheduled."
			);
		});
}

#[test]
fn swap_excess_are_confiscated() {
	const MAX_SWAP: AssetAmount = 100;
	const AMOUNT: AssetAmount = 1_000;
	const CONFISCATED_AMOUNT: AssetAmount = AMOUNT - MAX_SWAP;

	new_test_ext().execute_with(|| {
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		set_maximum_swap_amount(from, Some(MAX_SWAP));

		swap_with_custom_broker_fee(from, to, AMOUNT, bounded_vec![]);

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_request_id: SwapRequestId(1),
			asset: from,
			total_amount: AMOUNT,
			confiscated_amount: CONFISCATED_AMOUNT,
		}));

		assert_eq!(
			ScheduledSwaps::<Test>::get(),
			BTreeMap::from([(
				1.into(),
				Swap::new(
					1.into(),
					1.into(),
					from,
					to,
					MAX_SWAP,
					None,
					System::block_number() + SWAP_DELAY_BLOCKS as u64
				)
			)])
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900);
	});
}

#[test]
fn swaps_are_executed_according_to_execute_at_field() {
	let mut swaps = generate_test_swaps();
	let later_swaps = swaps.split_off(2);

	new_test_ext()
		.then_execute_at_block(1_u64, |_| {
			// Block 1, swaps should be scheduled at block 3
			insert_swaps(&swaps);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(1),
					execute_at: 3,
					..
				}),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(2),
					execute_at: 3,
					..
				}),
			);
		})
		.then_execute_at_next_block(|_| {
			// Block 2, swaps should be scheduled at block 4
			assert_eq!(System::block_number(), 2);
			insert_swaps(&later_swaps);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(3),
					execute_at: 4,
					..
				}),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(4),
					execute_at: 4,
					..
				}),
			);
		})
		.then_execute_at_next_block(|_| {
			// First group of swaps will be processed at the end of this block
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 3);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(1),
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1),
					reason: SwapRequestCompletionReason::Executed
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(2), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(2),
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(2),
					reason: SwapRequestCompletionReason::Executed
				}),
			);
		})
		.then_execute_at_next_block(|_| {
			// Second group of swaps will be processed at the end of this block
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 4);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(3), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(3),
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(3),
					reason: SwapRequestCompletionReason::Executed
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(4), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(4),
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(4),
					reason: SwapRequestCompletionReason::Executed
				}),
			);
		});
}

#[test]
fn swaps_get_retried_after_failure() {
	let mut swaps = generate_test_swaps();
	let later_swaps = swaps.split_off(2);

	const EXECUTE_AT_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const RETRY_AT_BLOCK: u64 = EXECUTE_AT_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			assert_eq!(SwapRetryDelay::<Test>::get(), DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);
			// Block 1, swaps should be scheduled at block 3
			insert_swaps(&swaps);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(1),
					execute_at: EXECUTE_AT_BLOCK,
					..
				}),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(2),
					execute_at: EXECUTE_AT_BLOCK,
					..
				}),
			);
		})
		.then_execute_at_next_block(|_| {
			// Block 2, swaps should be scheduled at block 4
			assert_eq!(System::block_number(), 2);
			insert_swaps(&later_swaps);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(3),
					execute_at: 4,
					..
				}),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(4),
					execute_at: 4,
					..
				}),
			);
		})
		.then_execute_at_next_block(|_| {
			// First group of swaps will be processed at the end of this block,
			// but we force them to fail:
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 3);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(1),
					execute_at: RETRY_AT_BLOCK,
					reason: SwapFailureReason::PriceImpactLimit,
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(2),
					execute_at: RETRY_AT_BLOCK,
					reason: SwapFailureReason::PriceImpactLimit,
				})
			);

			assert_eq!(get_scheduled_swap_block(SwapId(1)), Some(RETRY_AT_BLOCK));
			assert_eq!(get_scheduled_swap_block(SwapId(2)), Some(RETRY_AT_BLOCK));
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(System::block_number(), 4);
			// The swaps originally scheduled for block 4 should be executed now,
			// and should succeed.
			MockSwappingApi::set_swaps_should_fail(false);
		})
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(3), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(3),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(3),
					reason: SwapRequestCompletionReason::Executed
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(4), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(4),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(4),
					reason: SwapRequestCompletionReason::Executed
				}),
			);
		})
		.then_process_blocks_until_block(RETRY_AT_BLOCK)
		.then_execute_with(|_| {
			// Re-trying failed swaps originally scheduled for block 3 (which should
			// now be successful):
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(1),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1),
					reason: SwapRequestCompletionReason::Executed
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(2), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(2),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(2),
					reason: SwapRequestCompletionReason::Executed
				}),
			);
		});
}

#[test]
fn fee_swap_is_retried_after_failure() {
	const EXECUTE_AT_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const RETRY_AT_BLOCK: u64 = EXECUTE_AT_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			assert_eq!(SwapRetryDelay::<Test>::get(), DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

			Swapping::init_swap_request(
				Asset::Usdc,
				1000,
				Asset::Flip,
				SwapRequestType::NetworkFee,
				Default::default(),
				None,
				None,
				SwapOrigin::Internal,
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SwapRequestId(1),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: SwapId(1),
					execute_at: EXECUTE_AT_BLOCK,
					..
				}),
			);

			// Make sure that the swap initially fails:
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_process_blocks_until_block(EXECUTE_AT_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(1),
					execute_at: RETRY_AT_BLOCK,
					reason: SwapFailureReason::PriceImpactLimit,
				})
			);

			assert_eq!(get_scheduled_swap_block(SwapId(1)), Some(RETRY_AT_BLOCK));

			// Make sure that the swap will now succeed:
			MockSwappingApi::set_swaps_should_fail(false);
		})
		.then_process_blocks_until_block(RETRY_AT_BLOCK)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1),
					reason: SwapRequestCompletionReason::Executed
				}),
			);
		});
}

#[test]
fn deposit_address_ready_event_contains_correct_parameters() {
	new_test_ext().execute_with(|| {
		let dca_parameters = DcaParameters { number_of_chunks: 5, chunk_interval: 2 };

		let refund_parameters = REFUND_PARAMS;

		const BOOST_FEE: u16 = 100;
		assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
			RuntimeOrigin::signed(BROKER),
			Asset::Eth,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			0,
			None,
			BOOST_FEE,
			Default::default(),
			REFUND_PARAMS,
			Some(dca_parameters.clone()),
		));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapDepositAddressReady {
				boost_fee: BOOST_FEE,
				refund_parameters: ref refund_parameters_in_event,
				dca_parameters: Some(ref dca_params_in_event),
				..
			}) if *refund_parameters_in_event == refund_parameters && dca_params_in_event == &dca_parameters
		);
	});
}

#[test]
fn test_get_scheduled_swap_legs() {
	new_test_ext().execute_with(|| {
		const INIT_AMOUNT: AssetAmount = 10u128.pow(18);
		const BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

		ScheduledSwaps::<Test>::mutate(|swaps| {
			swaps.extend(vec![
				(1.into(), create_test_swap(1, Asset::Flip, Asset::Usdc, INIT_AMOUNT, None, BLOCK)),
				(2.into(), create_test_swap(2, Asset::Usdc, Asset::Flip, INIT_AMOUNT, None, BLOCK)),
				(3.into(), create_test_swap(3, Asset::Btc, Asset::Eth, INIT_AMOUNT, None, BLOCK)),
				(4.into(), create_test_swap(4, Asset::Flip, Asset::Btc, INIT_AMOUNT, None, BLOCK)),
				(5.into(), create_test_swap(5, Asset::Eth, Asset::Flip, INIT_AMOUNT, None, BLOCK)),
			]);
		});

		SwapRate::set(2);
		// Eth→USDC intermediate for swap 5: Eth(18-dec)→USDC(6-dec) at rate=2
		// = INIT_AMOUNT * 2 / 10^12 = 10^18 * 2 / 10^12 = 2_000_000
		let intermediate_amount = mock_swap_output(Asset::Eth, STABLE_ASSET, INIT_AMOUNT);

		// The test is more useful when these aren't equal:
		assert_ne!(INIT_AMOUNT, intermediate_amount);

		assert_eq!(
			Swapping::get_scheduled_swap_legs(Asset::Flip),
			vec![
				(
					SwapLegInfo {
						swap_id: SwapId(1),
						swap_request_id: SwapRequestId(1),
						base_asset: Asset::Flip,
						quote_asset: Asset::Usdc,
						side: Side::Sell,
						amount: INIT_AMOUNT,
						source_asset: None,
						source_amount: None,
						remaining_chunks: 0,
						chunk_interval: SWAP_DELAY_BLOCKS,
					},
					BLOCK
				),
				(
					SwapLegInfo {
						swap_id: SwapId(2),
						swap_request_id: SwapRequestId(2),
						base_asset: Asset::Flip,
						quote_asset: Asset::Usdc,
						side: Side::Buy,
						amount: INIT_AMOUNT,
						source_asset: None,
						source_amount: None,
						remaining_chunks: 0,
						chunk_interval: SWAP_DELAY_BLOCKS,
					},
					BLOCK
				),
				(
					SwapLegInfo {
						swap_id: SwapId(4),
						swap_request_id: SwapRequestId(4),
						base_asset: Asset::Flip,
						quote_asset: Asset::Usdc,
						side: Side::Sell,
						amount: INIT_AMOUNT,
						source_asset: None,
						source_amount: None,
						remaining_chunks: 0,
						chunk_interval: SWAP_DELAY_BLOCKS,
					},
					BLOCK
				),
				(
					SwapLegInfo {
						swap_id: SwapId(5),
						swap_request_id: SwapRequestId(5),
						base_asset: Asset::Flip,
						quote_asset: Asset::Usdc,
						side: Side::Buy,
						amount: intermediate_amount,
						source_asset: Some(Asset::Eth),
						source_amount: Some(INIT_AMOUNT),
						remaining_chunks: 0,
						chunk_interval: SWAP_DELAY_BLOCKS,
					},
					BLOCK
				),
			]
		);
	});
}

#[test]
fn test_get_scheduled_swap_legs_fallback() {
	new_test_ext().execute_with(|| {
		const INIT_AMOUNT: AssetAmount = 1000000000000000000000;
		const PRICE: u128 = 2;
		const BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

		ScheduledSwaps::<Test>::mutate(|swaps| {
			swaps.extend(vec![
				(1.into(), create_test_swap(1, Asset::Flip, Asset::Eth, INIT_AMOUNT, None, BLOCK)),
				(2.into(), create_test_swap(2, Asset::Eth, Asset::Usdc, INIT_AMOUNT, None, BLOCK)),
			]);
		});

		// Setting the swap rate to something different from the price so that if the fallback is
		// not used, it will give a different result, avoiding a false positive.
		SwapRate::set((PRICE.checked_add(1).unwrap()).saturated_into());

		// The swap simulation must fail for it to use the fallback price estimation
		MockSwappingApi::set_swaps_should_fail(true);

		// Only setting pool price for FLIP to make sure that the test would fail
		// if the code tried to use the price of some other asset
		MockPoolPriceApi::set_pool_price(
			Asset::Flip,
			STABLE_ASSET,
			Price::from_usd_fine_amount(PRICE),
		);

		assert_eq!(
			Swapping::get_scheduled_swap_legs(Asset::Eth),
			vec![
				(
					SwapLegInfo {
						swap_id: SwapId(1),
						swap_request_id: SwapRequestId(1),
						base_asset: Asset::Eth,
						quote_asset: Asset::Usdc,
						side: Side::Buy,
						amount: INIT_AMOUNT * PRICE,
						source_asset: Some(Asset::Flip),
						source_amount: Some(INIT_AMOUNT),
						remaining_chunks: 0,
						chunk_interval: SWAP_DELAY_BLOCKS,
					},
					BLOCK
				),
				(
					SwapLegInfo {
						swap_id: SwapId(2),
						swap_request_id: SwapRequestId(2),
						base_asset: Asset::Eth,
						quote_asset: Asset::Usdc,
						side: Side::Sell,
						amount: INIT_AMOUNT,
						source_asset: None,
						source_amount: None,
						remaining_chunks: 0,
						chunk_interval: SWAP_DELAY_BLOCKS,
					},
					BLOCK
				)
			]
		);
	});
}

#[test]
fn test_get_scheduled_swap_legs_for_dca() {
	new_test_ext().execute_with(|| {
		const INIT_AMOUNT: AssetAmount = 1000000000000000000000;
		const NUMBER_OF_CHUNKS: u32 = 3;
		const CHUNK_INTERVAL: u32 = 10;
		const BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		SwapRate::set(1);

		let dca_params =
			DcaParameters { number_of_chunks: NUMBER_OF_CHUNKS, chunk_interval: CHUNK_INTERVAL };

		ScheduledSwaps::<Test>::mutate(|swaps| {
			swaps.extend(vec![(
				1.into(),
				create_test_swap(1, Asset::Flip, Asset::Eth, INIT_AMOUNT, Some(dca_params), BLOCK),
			)]);
		});

		// Flip(18-dec)→USDC(6-dec) at rate=1: INIT_AMOUNT * 1 / 10^12 = 10^21 / 10^12 = 10^9
		let intermediate_usdc = mock_swap_output(Asset::Flip, STABLE_ASSET, INIT_AMOUNT);

		assert_eq!(
			Swapping::get_scheduled_swap_legs(Asset::Eth),
			vec![(
				SwapLegInfo {
					swap_id: SwapId(1),
					swap_request_id: SwapRequestId(1),
					base_asset: Asset::Eth,
					quote_asset: Asset::Usdc,
					side: Side::Buy,
					amount: intermediate_usdc,
					source_asset: Some(Asset::Flip),
					source_amount: Some(INIT_AMOUNT),
					// This is the first chunk, so there are 2 remaining
					remaining_chunks: NUMBER_OF_CHUNKS - 1,
					chunk_interval: CHUNK_INTERVAL,
				},
				BLOCK
			)]
		);
	});
}

#[test]
fn broker_deregistration_checks_earned_fees() {
	new_test_ext().execute_with(|| {
		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_broker(
			OriginTrait::signed(BROKER),
		)
		.expect("BROKER was registered in test setup.");

		// Earn some fees.
		<Test as Config>::BalanceApi::credit_account(&BROKER, Asset::Eth, 100);

		assert_noop!(
			Swapping::deregister_as_broker(OriginTrait::signed(BROKER)),
			Error::<Test>::EarnedFeesNotWithdrawn,
		);

		assert_ok!(Swapping::withdraw(
			OriginTrait::signed(BROKER),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));

		assert_ok!(Swapping::deregister_as_broker(OriginTrait::signed(BROKER)),);

		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_broker(
			OriginTrait::signed(BROKER),
		)
		.expect_err("BROKER should be deregistered.");
	});
}

#[test]
fn broker_deregistration_checks_private_channels() {
	new_test_ext().execute_with(|| {
		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_broker(
			OriginTrait::signed(BROKER),
		)
		.expect("BROKER was registered in test setup.");

		MockFundingInfo::<Test>::credit_funds(&BROKER, FLIPPERINOS_PER_FLIP * 200);

		// Create a private broker channel
		assert_ok!(Swapping::open_private_btc_channel(OriginTrait::signed(BROKER)));

		assert_noop!(
			Swapping::deregister_as_broker(OriginTrait::signed(BROKER)),
			Error::<Test>::PrivateChannelExistsForBroker,
		);

		assert_ok!(Swapping::close_private_btc_channel(OriginTrait::signed(BROKER)));

		assert_ok!(Swapping::deregister_as_broker(OriginTrait::signed(BROKER)));

		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_broker(
			OriginTrait::signed(BROKER),
		)
		.expect_err("BROKER should be deregistered.");
	});
}

#[test]
fn can_handle_input_and_output_being_the_same_asset() {
	const ASSET: Asset = Asset::Eth;
	const AMOUNT: AssetAmount = 10u128.pow(18);
	const NETWORK_FEE_RATE: Permill = Permill::from_parts(100);
	const NETWORK_FEE: FeeRateAndMinimum = FeeRateAndMinimum { rate: NETWORK_FEE_RATE, minimum: 0 };

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(NETWORK_FEE);

			Swapping::init_swap_request(
				ASSET,
				AMOUNT,
				ASSET,
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

			assert_eq!(ScheduledSwaps::<Test>::get().len(), 1);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			// Eth→Eth is still a 2-leg swap via USDC:
			// Eth→USDC intermediate, then fee taken, then USDC→Eth
			let intermediate = mock_swap_output(ASSET, STABLE_ASSET, AMOUNT);
			let network_fee_amount = NETWORK_FEE_RATE * intermediate;
			let output_amount =
				mock_swap_output(STABLE_ASSET, ASSET, intermediate - network_fee_amount);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(1),
					egress_id: (ForeignChain::Ethereum, 1),
					amount,
					..
				}) if amount == output_amount,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
			);
		});
}

#[test]
fn can_handle_input_and_output_being_the_stable_asset() {
	const ASSET: Asset = STABLE_ASSET;
	const AMOUNT: AssetAmount = 1_000_000;
	const NETWORK_FEE_RATE: Permill = Permill::from_parts(100);
	const NETWORK_FEE: FeeRateAndMinimum = FeeRateAndMinimum { rate: NETWORK_FEE_RATE, minimum: 0 };

	new_test_ext()
		.execute_with(|| {
			NetworkFee::<Test>::set(NETWORK_FEE);

			Swapping::init_swap_request(
				ASSET,
				AMOUNT,
				ASSET,
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

			assert_eq!(ScheduledSwaps::<Test>::get().len(), 1);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			let network_fee_amount = NETWORK_FEE_RATE * AMOUNT;
			// The swap rate should not apply because its already in the stable asset.
			let output_amount: AssetAmount = AMOUNT - network_fee_amount;

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(1),
					egress_id: (ForeignChain::Ethereum, 1),
					amount,
					..
				}) if amount == output_amount,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
			);
		});
}

#[cfg(test)]
mod swap_batching {

	use super::*;

	impl<T: Config> Swap<T> {
		fn to_state(&self, stable_amount: Option<AssetAmount>) -> SwapState<T> {
			SwapState {
				swap: self.clone(),
				network_fee_taken: None,
				broker_fee_taken: None,
				stable_amount,
				final_output: None,
				oracle_delta: None,
			}
		}
	}

	#[test]
	fn single_swap() {
		let swap1 = Swap::new(0.into(), 0.into(), Asset::Btc, Asset::Usdc, 1000, None, 1);
		let mut swaps = vec![swap1.clone()];

		let swap_states = vec![swap1.to_state(None)];

		assert_eq!(
			utilities::split_off_highest_impact_swap::<mock::Test>(
				&mut swaps,
				&swap_states,
				SwapLeg::ToStable
			),
			Some(swap1)
		);
		assert_eq!(swaps, vec![]);
	}

	#[test]
	fn swaps_fail_into_stable() {
		let swap1 = Swap::new(0.into(), 0.into(), Asset::Btc, Asset::Usdc, 500, None, 1);
		let swap2 = Swap::new(1.into(), 1.into(), Asset::Btc, Asset::Eth, 1000, None, 1);
		let swap3 = Swap::new(2.into(), 2.into(), Asset::Eth, Asset::Usdc, 1000, None, 1);

		let mut swaps = vec![swap1.clone(), swap2.clone(), swap3.clone()];

		// The test assumes the BTC->USDC leg failed (so swap3 is excluded from `swap_states`)
		let swap_states = vec![swap1.to_state(None), swap2.to_state(None)];

		assert_eq!(
			utilities::split_off_highest_impact_swap::<mock::Test>(
				&mut swaps,
				&swap_states,
				SwapLeg::ToStable
			),
			Some(swap2)
		);
		assert_eq!(swaps, vec![swap1, swap3]);
	}

	#[test]
	fn swaps_fail_from_stable() {
		// BTC swap should be removed because it would result in a larger amount
		// of USDC and thus will have higher impact on the Eth pool
		let swap1 = Swap::new(1.into(), 1.into(), Asset::Btc, Asset::Eth, 1, None, 1);
		let swap2 = Swap::new(2.into(), 2.into(), Asset::Usdc, Asset::Eth, 1000, None, 1);
		let swap3 = Swap::new(3.into(), 3.into(), Asset::Eth, Asset::Usdc, 100, None, 1);

		let mut swaps = vec![swap1.clone(), swap2.clone(), swap3.clone()];

		// The test assumes the USDC->ETH leg failed (so swap3 is excluded from `swap_states`)
		let swap_state = vec![swap1.to_state(Some(60000)), swap2.to_state(Some(3000))];

		assert_eq!(
			utilities::split_off_highest_impact_swap::<mock::Test>(
				&mut swaps,
				&swap_state,
				SwapLeg::FromStable
			),
			Some(swap1)
		);
		assert_eq!(swaps, vec![swap2, swap3]);
	}

	#[test]
	fn price_impact_removes_one_swap() {
		// Initial execution of a batch results in a "price impact" error while swapping from
		// stable asset. A swap with "the largest" impact should be removed (rescheduled for a later
		// block), and the remaining swaps should be retried immediately and succeed.

		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const SWAP_RESCHEDULED_BLOCK: u64 = SWAP_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64;

		new_test_ext()
			.execute_with(|| {
				NetworkFee::<Test>::set(FeeRateAndMinimum {
					rate: Permill::from_percent(1),
					minimum: 0,
				});

				let swap = |input_asset: Asset, output_asset: Asset, input_amount: AssetAmount| {
					TestSwapParams {
						input_asset,
						output_asset,
						input_amount,
						price_limits_and_expiry: None,
						dca_params: None,
						output_address: ForeignChainAddress::Eth([2; 20].into()),
						is_ccm: false,
					}
				};

				// swap1: Flip→Eth (cross-decimal 2-leg), swap2: Usdc→Eth (1-leg from stable)
				// swap1 Flip→USDC intermediate at rate=2: 10^18 * 2 / 10^12 = 2_000_000 USDC
				// After 1% network fee: 1_980_000 USDC → USDC→Eth: ~9.9e17 Eth
				// swap2 Usdc→Eth at rate=2: after 1% fee on 150_000:
				//   network fee = 1500 USDC, output = 148_500 * 10^12/2 = 7.425e16 Eth
				// Set Eth liquidity to allow swap2 (~7.5e16) but not swap1 (~9.9e17)
				let swap1 = swap(Asset::Flip, Asset::Eth, 10u128.pow(18));
				let swap2 = swap(Asset::Usdc, Asset::Eth, 150_000);

				insert_swaps(&[swap1, swap2]);

				// 10^17 Eth is enough for swap2 (~7.5e16) but not swap1 (~9.9e17):
				MockSwappingApi::add_liquidity(Asset::Eth, 10u128.pow(17));
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
					RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(2), .. }),
					RuntimeEvent::Swapping(Event::SwapEgressScheduled { .. }),
					RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
					RuntimeEvent::Swapping(Event::SwapRescheduled {
						swap_id: SwapId(1),
						execute_at: SWAP_RESCHEDULED_BLOCK,
						reason: SwapFailureReason::PriceImpactLimit,
					}),
				);

				// Ensure that storage has been reverted from the first (failed) attempt
				// by checking the network fee (which should only be collected
				// from swap 2):
				// swap2: Usdc→Eth, network fee taken on 150_000 USDC input = 1% = 1500
				assert_eq!(CollectedNetworkFee::<Test>::get(), 1500);

				// Adding enough Eth liquidity to make swap1 succeed (~9.9e17 needed):
				MockSwappingApi::add_liquidity(Asset::Eth, 10u128.pow(18));
			})
			.then_process_blocks_until_block(SWAP_RESCHEDULED_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				);

				// swap1: Flip→USDC gives 2_000_000 USDC intermediate, 1% network fee = 20_000
				assert_eq!(CollectedNetworkFee::<Test>::get(), 1500 + 20_000);
			});
	}

	#[test]
	fn price_impact_removes_all_swaps() {
		// Initial execution of a batch results in a "price impact" error while swapping into stable
		// asset. Both swaps end up being rescheduled (i.e. removing swaps individually did not
		// help).

		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

		new_test_ext()
			.execute_with(|| {
				NetworkFee::<Test>::set(FeeRateAndMinimum {
					rate: Permill::from_percent(1),
					minimum: 0,
				});
				let swap = |input_asset: Asset, output_asset: Asset, input_amount: AssetAmount| {
					TestSwapParams {
						input_asset,
						output_asset,
						input_amount,
						price_limits_and_expiry: None,
						dca_params: None,
						output_address: ForeignChainAddress::Eth([2; 20].into()),
						is_ccm: false,
					}
				};

				// swap1: Btc→Eth (needs Btc→USDC first, then USDC→Eth)
				// swap2: Eth→Usdc (needs Eth→USDC directly)
				// Both swaps need USDC in the intermediate/output step.
				// Setting USDC liquidity to 0 makes all to-USDC legs fail.
				let swap1 = swap(Asset::Btc, Asset::Eth, 100_000);
				let swap2 = swap(Asset::Eth, Asset::Usdc, 10u128.pow(18));

				insert_swaps(&[swap1, swap2]);

				// This activates liquidity check for USDC in the Mock, and provides insufficient
				// amount, leading to all swaps failing:
				MockSwappingApi::add_liquidity(Asset::Usdc, 0);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
					RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
					RuntimeEvent::Swapping(Event::SwapRescheduled { swap_id: SwapId(2), .. }),
					RuntimeEvent::Swapping(Event::SwapRescheduled { swap_id: SwapId(1), .. }),
				);

				assert_eq!(CollectedNetworkFee::<Test>::get(), 0);
			});
	}
}

#[cfg(test)]
mod internal_swaps {

	use cf_traits::{SwapOutputActionEncoded, mocks::balance_api::MockBalance};

	use super::*;

	const INPUT_ASSET: Asset = Asset::Usdc;
	const OUTPUT_ASSET: Asset = Asset::Eth;

	const INPUT_AMOUNT: AssetAmount = 1000;

	#[test]
	fn swap_into_internal_balance() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

		let min_price = Price::from_usd_fine_amount(DEFAULT_SWAP_RATE).invert();

		new_test_ext()
			.execute_with(|| {
				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					0,
					PriceLimits { min_price, max_oracle_price_slippage: None },
					None,
					LP_ACCOUNT,
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequested {
						input_asset: INPUT_ASSET,
						input_amount: INPUT_AMOUNT,
						output_asset: OUTPUT_ASSET,
						origin: SwapOrigin::OnChainAccount(LP_ACCOUNT),
						request_type: SwapRequestTypeEncoded::Regular {
							output_action: SwapOutputActionEncoded::CreditOnChain {
								account_id: LP_ACCOUNT
							}
						},
						..
					})
				);

				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, INPUT_ASSET), 0);
				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, OUTPUT_ASSET), 0);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				// Usdc(6-dec) → Eth(18-dec) at rate=2: 1000 * 10^18 / (2*10^6) = 5e14
				let expected_output = mock_swap_output(INPUT_ASSET, OUTPUT_ASSET, INPUT_AMOUNT);

				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						swap_request_id: SWAP_REQUEST_ID,
						..
					}),
					RuntimeEvent::Swapping(Event::CreditedOnChain {
						swap_request_id: SWAP_REQUEST_ID,
						account_id: LP_ACCOUNT,
						asset: OUTPUT_ASSET,
						amount,
						..
					}) if amount == expected_output,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID,
						reason: SwapRequestCompletionReason::Executed
					}),
				);

				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, INPUT_ASSET), 0);
				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, OUTPUT_ASSET), expected_output);
			});
	}

	#[test]
	fn swap_on_chain_with_refund() {
		const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const MIN_NETWORK_FEE: u128 = 10;
		const INITIAL_RATE: u32 = (DEFAULT_SWAP_RATE * 2) as u32; // 4
		const NEW_SWAP_RATE: u32 = 10; // Higher rate = less output
		const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / 2;

		// USDC→Eth at rate=4: output/USDC = 10^18/(4*10^6) = 2.5e11 per USDC atom
		// USDC→Eth at rate=10: output/USDC = 10^18/(10*10^6) = 10^11 per USDC atom
		// min_price at 1.5e11 sits between → chunk 1 passes, chunk 2 fails.
		let min_price = Price::from_amounts_bounded(150_000_000_000u128.into(), 1u128.into());

		new_test_ext()
			.execute_with(|| {
				// Internal swaps use a different network fee minimum than the regular swaps.
				// This minimum network fee is used as the refund fee.
				InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
					rate: Permill::from_percent(0),
					minimum: MIN_NETWORK_FEE,
				});
				// Set price for refund fee calculation
				MockPriceFeedApi::set_price_usd_fine(INPUT_ASSET, 1);
				SwapRate::set(INITIAL_RATE);

				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					0,
					PriceLimits { min_price, max_oracle_price_slippage: None },
					Some(DcaParameters { number_of_chunks: 2, chunk_interval: 2 }),
					LP_ACCOUNT,
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequested {
						input_asset: INPUT_ASSET,
						input_amount: INPUT_AMOUNT,
						output_asset: OUTPUT_ASSET,
						origin: SwapOrigin::OnChainAccount(LP_ACCOUNT),
						request_type: SwapRequestTypeEncoded::Regular {
							output_action: SwapOutputActionEncoded::CreditOnChain {
								account_id: LP_ACCOUNT
							}
						},
						..
					})
				);

				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, INPUT_ASSET), 0);
				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, OUTPUT_ASSET), 0);
			})
			.then_process_blocks_until_block(CHUNK_1_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { .. }),
				);

				// Now we adjust execution price so that the next chunk gets refunded:
				SwapRate::set(NEW_SWAP_RATE);
			})
			.then_process_blocks_until_block(CHUNK_2_BLOCK)
			.then_execute_with(|_| {
				const REFUND_FEE: AssetAmount = MIN_NETWORK_FEE;
				const EXPECTED_REFUND_AMOUNT: AssetAmount = CHUNK_AMOUNT - REFUND_FEE;
				// USDC→Eth at INITIAL_RATE=4: (500-10) USDC → Eth
				// Chunk 1 executed at INITIAL_RATE, but SwapRate was changed to NEW_SWAP_RATE,
				// so temporarily restore it for computing expected output:
				SwapRate::set(INITIAL_RATE);
				let expected_output =
					mock_swap_output(INPUT_ASSET, OUTPUT_ASSET, CHUNK_AMOUNT - MIN_NETWORK_FEE);
				SwapRate::set(NEW_SWAP_RATE);

				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::SwapAborted {
						swap_id: SwapId(2),
						reason: SwapFailureReason::MinPriceViolation
					}),
					RuntimeEvent::Swapping(Event::SwapRequested {
						request_type: SwapRequestTypeEncoded::NetworkFee,
						input_amount: REFUND_FEE,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapScheduled {
						swap_type: SwapType::NetworkFee,
						input_amount: REFUND_FEE,
						..
					}),
					RuntimeEvent::Swapping(Event::RefundedOnChain {
						swap_request_id: SWAP_REQUEST_ID,
						account_id: LP_ACCOUNT,
						asset: INPUT_ASSET,
						amount: EXPECTED_REFUND_AMOUNT,
						refund_fee: REFUND_FEE,
					}),
					RuntimeEvent::Swapping(Event::CreditedOnChain {
						swap_request_id: SWAP_REQUEST_ID,
						account_id: LP_ACCOUNT,
						asset: OUTPUT_ASSET,
						amount,
						..
					}) if amount == expected_output,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID,
						reason: SwapRequestCompletionReason::Expired
					}),
				);

				assert_eq!(
					MockBalance::get_balance(&LP_ACCOUNT, INPUT_ASSET),
					EXPECTED_REFUND_AMOUNT
				);
				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, OUTPUT_ASSET), expected_output);
			});
	}
}

#[cfg(test)]
mod credit_flip_and_transfer {

	use cf_traits::{AccountInfo, SwapOutputActionEncoded, mocks::balance_api::MockBalance};

	use super::*;

	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Flip;

	const INPUT_AMOUNT: AssetAmount = INITIAL_FLIP_FUNDING;
	const INPUT_AMOUNT2: AssetAmount = 1000;

	#[test]
	fn swap_into_flip_to_credit() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		// Eth(18-dec) -> USDC(6-dec) -> Flip(18-dec): rates cancel out for same-decimal assets
		const EXPECTED_OUTPUT_AMOUNT: AssetAmount = INPUT_AMOUNT;

		new_test_ext()
			.execute_with(|| {
				Swapping::init_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					SwapRequestType::RegularNoNetworkFee {
						output_action: SwapOutputAction::CreditFlipAndTransferToGateway {
							account_id: LP_ACCOUNT,
							flip_to_subtract_from_swap_output: INITIAL_FLIP_FUNDING,
						},
					},
					Beneficiaries::default(),
					None,
					None,
					SwapOrigin::OnChainAccount(LP_ACCOUNT),
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequested {
						input_asset: INPUT_ASSET,
						input_amount: INPUT_AMOUNT,
						output_asset: OUTPUT_ASSET,
						origin: SwapOrigin::OnChainAccount(LP_ACCOUNT),
						request_type: SwapRequestTypeEncoded::RegularNoNetworkFee {
							output_action:
								SwapOutputActionEncoded::CreditFlipAndTransferToGateway {
									account_id: LP_ACCOUNT,
									flip_to_subtract_from_swap_output: INITIAL_FLIP_FUNDING,
								}
						},
						..
					})
				);

				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, INPUT_ASSET), 0);
				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, OUTPUT_ASSET), 0);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						swap_request_id: SWAP_REQUEST_ID,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID,
						reason: _
					}),
				);
				assert_eq!(
					MockFundingInfo::<Test>::balance(&LP_ACCOUNT),
					EXPECTED_OUTPUT_AMOUNT.saturating_sub(INITIAL_FLIP_FUNDING)
				);
				assert_eq!(FlipToBeSentToGateway::<Test>::get(), EXPECTED_OUTPUT_AMOUNT);
			});
	}

	#[test]
	fn swap_into_flip_to_credit_not_enough_output() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		// 1000 Eth_smallest -> USDC -> Flip: with rate=2, cross-decimal conversion loses
		// precision due to floor division: 1000 Eth_atoms -> 0 USDC_atoms -> 0 Flip_atoms
		const EXPECTED_OUTPUT_AMOUNT: AssetAmount = 0;

		new_test_ext()
			.execute_with(|| {
				Swapping::init_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT2,
					OUTPUT_ASSET,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::CreditFlipAndTransferToGateway {
							account_id: LP_ACCOUNT,
							flip_to_subtract_from_swap_output: INITIAL_FLIP_FUNDING,
						},
					},
					Beneficiaries::default(),
					None,
					None,
					SwapOrigin::OnChainAccount(LP_ACCOUNT),
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequested {
						input_asset: INPUT_ASSET,
						input_amount: INPUT_AMOUNT2,
						output_asset: OUTPUT_ASSET,
						origin: SwapOrigin::OnChainAccount(LP_ACCOUNT),
						request_type: SwapRequestTypeEncoded::Regular {
							output_action:
								SwapOutputActionEncoded::CreditFlipAndTransferToGateway {
									account_id: LP_ACCOUNT,
									flip_to_subtract_from_swap_output: INITIAL_FLIP_FUNDING,
								}
						},
						..
					})
				);

				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, INPUT_ASSET), 0);
				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, OUTPUT_ASSET), 0);
				assert_eq!(FlipToBurn::<Test>::get(), 0);
				assert_eq!(FlipToBeSentToGateway::<Test>::get(), 0);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						swap_request_id: SWAP_REQUEST_ID,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID,
						reason: _
					}),
				);

				assert_eq!(
					FlipToBurn::<Test>::get(),
					0i128.saturating_sub(
						INITIAL_FLIP_FUNDING
							.saturating_sub(EXPECTED_OUTPUT_AMOUNT)
							.try_into()
							.unwrap()
					)
				);
				assert_eq!(FlipToBeSentToGateway::<Test>::get(), INITIAL_FLIP_FUNDING);
			});
	}
}

mod private_channels {

	use super::*;
	use cf_traits::mocks::account_role_registry::MockAccountRoleRegistry;
	use sp_runtime::DispatchError::BadOrigin;

	#[test]
	fn open_private_btc_channel() {
		new_test_ext().execute_with(|| {
			const FIRST_CHANNEL_ID: u64 = 0;
			// Only brokers can open private channels
			assert_noop!(Swapping::open_private_btc_channel(OriginTrait::signed(ALICE)), BadOrigin);

			MockFundingInfo::<Test>::credit_funds(&BROKER, FLIPPERINOS_PER_FLIP * 200);

			assert_eq!(BrokerPrivateBtcChannels::<Test>::get(BROKER), None);

			assert_ok!(Swapping::open_private_btc_channel(OriginTrait::signed(BROKER)));

			assert_eq!(BrokerPrivateBtcChannels::<Test>::get(BROKER), Some(FIRST_CHANNEL_ID));

			System::assert_has_event(RuntimeEvent::Swapping(
				Event::<Test>::PrivateBrokerChannelOpened {
					broker_id: BROKER,
					channel_id: FIRST_CHANNEL_ID,
				},
			));

			// The same broker should not be able to open another private channel:
			{
				assert_noop!(
					Swapping::open_private_btc_channel(OriginTrait::signed(BROKER)),
					Error::<Test>::PrivateChannelExistsForBroker
				);
			}

			// A different broker can still open another private channel:
			{
				const BROKER_2: u64 = 777;
				<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
					&BROKER_2,
				)
				.unwrap();

				MockFundingInfo::<Test>::credit_funds(&BROKER_2, FLIPPERINOS_PER_FLIP * 200);

				assert_ok!(Swapping::open_private_btc_channel(OriginTrait::signed(BROKER_2)));

				assert_eq!(
					BrokerPrivateBtcChannels::<Test>::get(BROKER_2),
					Some(FIRST_CHANNEL_ID + 1)
				);
			}
		});
	}

	#[test]
	fn close_private_btc_channel() {
		new_test_ext().execute_with(|| {
			const CHANNEL_ID: u64 = 0;
			// Only brokers can close channels
			assert_noop!(
				Swapping::close_private_btc_channel(OriginTrait::signed(ALICE)),
				BadOrigin
			);

			// Can't close a channel if one does not exist:
			assert_noop!(
				Swapping::close_private_btc_channel(OriginTrait::signed(BROKER)),
				Error::<Test>::NoPrivateChannelExistsForBroker
			);

			MockFundingInfo::<Test>::credit_funds(&BROKER, FLIPPERINOS_PER_FLIP * 200);

			assert_ok!(Swapping::open_private_btc_channel(OriginTrait::signed(BROKER)));
			assert_eq!(BrokerPrivateBtcChannels::<Test>::get(BROKER), Some(CHANNEL_ID));

			// Now closing should succeed:
			assert_ok!(Swapping::close_private_btc_channel(OriginTrait::signed(BROKER)));
			assert_eq!(BrokerPrivateBtcChannels::<Test>::get(BROKER), None);

			System::assert_has_event(RuntimeEvent::Swapping(
				Event::<Test>::PrivateBrokerChannelClosed {
					broker_id: BROKER,
					channel_id: CHANNEL_ID,
				},
			));

			// The same broker can re-open a (different) private channel:
			assert_ok!(Swapping::open_private_btc_channel(OriginTrait::signed(BROKER)));
			assert_eq!(BrokerPrivateBtcChannels::<Test>::get(BROKER), Some(CHANNEL_ID + 1));
		});
	}

	#[test]
	fn default_broker_bond() {
		new_test_ext().execute_with(|| {
			assert_eq!(BrokerBond::<Test>::get(), FLIPPERINOS_PER_FLIP * 100);
		});
	}
}

#[cfg(test)]
mod affiliates {

	use super::*;

	use cf_traits::mocks::account_role_registry::MockAccountRoleRegistry;
	use sp_runtime::DispatchError::BadOrigin;

	#[test]
	fn register_affiliate() {
		new_test_ext().execute_with(|| {
			const SHORT_ID: AffiliateShortId = AffiliateShortId(0);

			let withdrawal_address: EthereumAddress = Default::default();

			// Only brokers can register affiliates
			assert_noop!(
				Swapping::register_affiliate(OriginTrait::signed(ALICE), withdrawal_address,),
				BadOrigin
			);
			assert_eq!(Swapping::get_short_id(&BROKER, &BOB), None);

			// Registering an affiliate for the first time (no existing records)
			{
				assert_ok!(Swapping::register_affiliate(
					OriginTrait::signed(BROKER),
					withdrawal_address,
				));

				let affiliate_account_id = AffiliateIdMapping::<Test>::get(BROKER, SHORT_ID)
					.expect("Affiliate must be registered!");

				assert!(
					frame_system::Account::<Test>::contains_key(affiliate_account_id),
					"Account not created"
				);
				System::assert_has_event(RuntimeEvent::Swapping(
					Event::<Test>::AffiliateRegistration {
						broker_id: BROKER,
						short_id: SHORT_ID,
						withdrawal_address,
						affiliate_id: affiliate_account_id,
					},
				));
				assert_eq!(Swapping::get_short_id(&BROKER, &affiliate_account_id), Some(SHORT_ID));
			}
		});
	}

	#[test]
	fn register_address_and_request_withdrawal_success() {
		new_test_ext().execute_with(|| {
			const SHORT_ID: AffiliateShortId = AffiliateShortId(0);
			const BALANCE: AssetAmount = 200;
			let withdrawal_address: EthereumAddress = Default::default();

			assert_ok!(Swapping::register_affiliate(
				OriginTrait::signed(BROKER),
				withdrawal_address,
			));

			let affiliate_account_id = AffiliateIdMapping::<Test>::get(BROKER, SHORT_ID)
				.expect("Affiliate must be registered!");

			MockBalance::credit_account(&affiliate_account_id, Asset::Usdc, BALANCE);

			assert_ok!(Swapping::affiliate_withdrawal_request(
				OriginTrait::signed(BROKER),
				affiliate_account_id,
			));

			assert_event_sequence!(
				Test,
				RuntimeEvent::System(frame_system::Event::NewAccount { .. }),
				RuntimeEvent::Swapping(Event::AffiliateRegistration { .. }),
				RuntimeEvent::Swapping(Event::WithdrawalRequested { .. }),
			);

			assert_eq!(MockBalance::get_balance(&affiliate_account_id, Asset::Usdc), 0);

			let egresses = MockEgressHandler::<Ethereum>::get_scheduled_egresses();
			assert_eq!(egresses.len(), 1);
			assert_eq!(egresses.first().unwrap().amount(), BALANCE);
		});
	}

	#[test]
	fn fail_due_to_insufficient_funds() {
		new_test_ext().execute_with(|| {
			const SHORT_ID: AffiliateShortId = AffiliateShortId(0);
			let withdrawal_address: EthereumAddress = Default::default();

			assert_ok!(Swapping::register_affiliate(
				OriginTrait::signed(BROKER),
				withdrawal_address
			));

			let affiliate_account_id = AffiliateIdMapping::<Test>::get(BROKER, SHORT_ID)
				.expect("Affiliate must be registered!");

			assert_noop!(
				Swapping::affiliate_withdrawal_request(
					OriginTrait::signed(BROKER),
					affiliate_account_id
				),
				Error::<Test>::NoFundsAvailable
			);
		});
	}

	#[test]
	fn withdrawal_can_only_get_triggered_by_associated_broker() {
		new_test_ext().execute_with(|| {
			const SHORT_ID: AffiliateShortId = AffiliateShortId(0);
			const BALANCE: AssetAmount = 200;
			let withdrawal_address: EthereumAddress = Default::default();

			assert_ok!(Swapping::register_affiliate(
				OriginTrait::signed(BROKER),
				withdrawal_address
			));

			let affiliate_account_id = AffiliateIdMapping::<Test>::get(BROKER, SHORT_ID)
				.expect("Affiliate must be registered!");

			MockBalance::credit_account(&affiliate_account_id, Asset::Usdc, BALANCE);

			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(&ALICE)
				.unwrap();

			assert_noop!(
				Swapping::affiliate_withdrawal_request(
					OriginTrait::signed(ALICE),
					affiliate_account_id
				),
				Error::<Test>::AffiliateNotRegisteredForBroker
			);
		});
	}

	#[test]
	fn can_not_deregister_broker_if_affiliates_still_have_balance() {
		new_test_ext().execute_with(|| {
			const SHORT_ID: AffiliateShortId = AffiliateShortId(0);
			const BALANCE: AssetAmount = 200;
			let withdrawal_address: EthereumAddress = Default::default();

			assert_ok!(Swapping::register_affiliate(
				OriginTrait::signed(BROKER),
				withdrawal_address
			));

			let affiliate_account_id = AffiliateIdMapping::<Test>::get(BROKER, SHORT_ID)
				.expect("Affiliate must be registered!");

			assert!(
				frame_system::Account::<Test>::contains_key(affiliate_account_id),
				"Account not created"
			);
			MockBalance::credit_account(&affiliate_account_id, Asset::Usdc, BALANCE);

			assert_noop!(
				Swapping::deregister_as_broker(OriginTrait::signed(BROKER)),
				Error::<Test>::AffiliateEarnedFeesNotWithdrawn
			);

			MockBalance::try_debit_account(&affiliate_account_id, Asset::Usdc, BALANCE).unwrap();

			assert_ok!(Swapping::deregister_as_broker(OriginTrait::signed(BROKER)));

			assert!(AffiliateAccountDetails::<Test>::get(BROKER, affiliate_account_id).is_none());
			assert!(AffiliateIdMapping::<Test>::get(BROKER, SHORT_ID).is_none());
			assert!(
				!frame_system::Account::<Test>::contains_key(affiliate_account_id),
				"Account not deleted"
			);
		});
	}
}

mod lending_liquidation_swaps {

	use cf_traits::{
		lending::LoanId,
		mocks::lending_pools::{LiquidationSwapOutcome, MockLendingSystemApi},
	};

	use super::*;

	#[test]
	fn abort_swap_request() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const SWAP_AMOUNT: AssetAmount = 1000;

		const NUMBER_OF_CHUNKS: u32 = 2;

		const SWAP_REQUEST_2_ID: SwapRequestId = SwapRequestId(2);

		const CHUNK_AMOUNT: AssetAmount = SWAP_AMOUNT / NUMBER_OF_CHUNKS as u128;

		fn init_swap() -> SwapRequestId {
			Swapping::init_swap_request(
				INPUT_ASSET,
				SWAP_AMOUNT,
				OUTPUT_ASSET,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::CreditLendingPool {
						swap_type: cf_traits::LendingSwapType::Liquidation {
							borrower_id: ALICE,
							loan_id: cf_traits::lending::LoanId(0),
						},
					},
				},
				Default::default(), // no broker fees
				None,               // no FoK
				Some(DcaParameters { number_of_chunks: NUMBER_OF_CHUNKS, chunk_interval: 2 }),
				SwapOrigin::Internal,
			)
		}

		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);

				// Start 2 swap requests, one of them is aborted immediately
				assert_eq!(init_swap(), SWAP_REQUEST_ID);
				assert_eq!(init_swap(), SWAP_REQUEST_2_ID);

				assert_eq!(
					Swapping::abort_swap_request(SWAP_REQUEST_2_ID),
					Some(SwapExecutionProgress {
						remaining_input_amount: SWAP_AMOUNT,
						accumulated_output_amount: 0
					})
				);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				// One chunk should have been executed here:
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						swap_request_id: SWAP_REQUEST_ID,
						..
					}),
				);

				assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());
				assert_eq!(ScheduledSwaps::<Test>::get().len(), 1);

				// Aborting at this point should return the remaining chunk and the swapped amount:
				assert_eq!(
					Swapping::abort_swap_request(SWAP_REQUEST_ID),
					Some(SwapExecutionProgress {
						remaining_input_amount: CHUNK_AMOUNT,
						accumulated_output_amount: CHUNK_AMOUNT / DEFAULT_SWAP_RATE
					})
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID,
						reason: SwapRequestCompletionReason::Aborted
					}),
				);

				assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_none());
				assert!(ScheduledSwaps::<Test>::get().is_empty());
			});
	}

	#[test]
	fn lending_liquidation_swap() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const FEE_AMOUNT: AssetAmount = 1000;

		const BORROWER_ID: u64 = 8;
		const LOAN_ID: LoanId = LoanId(0);

		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);

				assert_eq!(
					Swapping::init_swap_request(
						INPUT_ASSET,
						FEE_AMOUNT,
						OUTPUT_ASSET,
						SwapRequestType::Regular {
							output_action: SwapOutputAction::CreditLendingPool {
								swap_type: cf_traits::LendingSwapType::Liquidation {
									borrower_id: BORROWER_ID,
									loan_id: LOAN_ID,
								},
							},
						},
						Default::default(), // no broker fees
						None,               // no FoK
						None,               // no DCA
						SwapOrigin::Internal,
					),
					SWAP_REQUEST_ID
				);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				// Check that upon completion liquidation swap calls into the lending
				// pallet through LendingSystemApi:
				// With new mock: USDC→USDT gives output = input / rate
				assert_eq!(
					MockLendingSystemApi::get_liquidation_swap_outcome(SWAP_REQUEST_ID),
					Some(LiquidationSwapOutcome {
						borrower_id: BORROWER_ID,
						loan_id: LOAN_ID,
						output_amount: FEE_AMOUNT / DEFAULT_SWAP_RATE
					})
				)
			});
	}
}

mod bound_broker_withdrawal {
	use super::*;

	#[test]
	fn can_bind_withdrawal_address() {
		new_test_ext().execute_with(|| {
			let address: EthereumAddress = [1; 20].into();

			assert_ok!(Swapping::bind_broker_fee_withdrawal_address(
				OriginTrait::signed(BROKER),
				address
			));

			assert_eq!(BoundBrokerWithdrawalAddress::<Test>::get(BROKER), Some(address));

			System::assert_has_event(RuntimeEvent::Swapping(
				Event::<Test>::BoundBrokerWithdrawalAddress { broker: BROKER, address },
			));

			// Cannot bind again
			assert_noop!(
				Swapping::bind_broker_fee_withdrawal_address(
					OriginTrait::signed(BROKER),
					[2; 20].into()
				),
				Error::<Test>::BrokerAlreadyBound
			);
		});
	}

	#[test]
	fn enforce_withdrawal_address_restriction_for_usdc() {
		new_test_ext().execute_with(|| {
			let bound_addr: EthereumAddress = [0xaa; 20].into();
			let other_addr: EthereumAddress = [0xbb; 20].into();

			assert_ok!(Swapping::bind_broker_fee_withdrawal_address(
				OriginTrait::signed(BROKER),
				bound_addr
			));

			// Fund broker
			<Test as Config>::BalanceApi::credit_account(&BROKER, Asset::Usdc, 1000);
			<Test as Config>::BalanceApi::credit_account(&BROKER, Asset::Eth, 1000);

			// Withdraw to different address fails
			assert_noop!(
				Swapping::withdraw(
					OriginTrait::signed(BROKER),
					Asset::Usdc,
					EncodedAddress::Eth(other_addr.into()),
				),
				Error::<Test>::BrokerBoundWithdrwalAddressRestrictionViolated
			);
			assert_noop!(
				Swapping::withdraw(
					OriginTrait::signed(BROKER),
					Asset::Eth,
					EncodedAddress::Eth(other_addr.into()),
				),
				Error::<Test>::BrokerBoundWithdrwalAddressRestrictionViolated
			);

			// Withdraw to bound address succeeds
			assert_ok!(Swapping::withdraw(
				OriginTrait::signed(BROKER),
				Asset::Usdc,
				EncodedAddress::Eth(bound_addr.into()),
			));
			assert_ok!(Swapping::withdraw(
				OriginTrait::signed(BROKER),
				Asset::Eth,
				EncodedAddress::Eth(bound_addr.into()),
			));
		});
	}

	#[test]
	fn no_restriction_for_other_chains() {
		new_test_ext().execute_with(|| {
			let bound_addr: EthereumAddress = [0xaa; 20].into();

			assert_ok!(Swapping::bind_broker_fee_withdrawal_address(
				OriginTrait::signed(BROKER),
				bound_addr
			));

			// Fund broker with SOL
			<Test as Config>::BalanceApi::credit_account(&BROKER, Asset::Sol, 1000);

			// Withdraw BTC to different address succeeds (restriction only applies to Ethereum
			// withdrawals)
			assert_ok!(Swapping::withdraw(
				OriginTrait::signed(BROKER),
				Asset::Sol,
				EncodedAddress::Sol([0xbb; 32]),
			));
		});
	}
}
