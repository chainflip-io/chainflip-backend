mod ccm;
mod config;
mod dca;
mod fees;
mod fill_or_kill;

use std::sync::LazyLock;

use super::*;
use crate::{
	mock::{RuntimeEvent, *},
	CollectedRejectedFunds, Error, Event, MaximumSwapAmount, Pallet, Swap, SwapOrigin, SwapQueue,
	SwapType,
};
use cf_amm::math::{price_to_sqrt_price, PRICE_FRACTIONAL_BITS};
use cf_chains::{
	self,
	address::{AddressConverter, EncodedAddress, ForeignChainAddress},
	dot::PolkadotAccountId,
	evm::H256,
	AnyChain, CcmChannelMetadata, CcmDepositMetadata, Ethereum, TransactionInIdForAnyChain,
};
use cf_primitives::{
	Asset, AssetAmount, BasisPoints, Beneficiary, BlockNumber, DcaParameters, ForeignChain,
};
use cf_test_utilities::{assert_event_sequence, assert_events_eq, assert_has_matching_event};
use cf_traits::{
	mocks::{
		address_converter::MockAddressConverter,
		egress_handler::{MockEgressHandler, MockEgressParameter},
		funding_info::MockFundingInfo,
		ingress_egress_fee_handler::MockIngressEgressFeeHandler,
	},
	AccountRoleRegistry, AssetConverter, Chainflip, SetSafeMode,
};
use frame_support::{
	assert_noop, assert_ok,
	testing_prelude::bounded_vec,
	traits::{Hooks, OriginTrait},
};
use sp_arithmetic::Permill;
use sp_core::{H160, U256};
use sp_std::iter;

const GAS_BUDGET: AssetAmount = 100_000u128;
const INPUT_AMOUNT: AssetAmount = 40_000;
const SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(1);
const INIT_BLOCK: u64 = 1;
const BROKER_FEE_BPS: u16 = 10;
const INPUT_ASSET: Asset = Asset::Usdc;
const OUTPUT_ASSET: Asset = Asset::Eth;

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
	refund_params: Option<RefundParametersExtended<u64>>,
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
			refund_params: refund_params.map(|params| params.into_extended_params(INPUT_AMOUNT)),
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
	fn into_extended_params(self, input_amount: AssetAmount) -> RefundParametersExtended<u64> {
		use cf_amm::math::{bounded_sqrt_price, sqrt_price_to_price};

		RefundParametersExtended {
			retry_duration: self.retry_duration,
			refund_destination: RefundDestination::ExternalAddress(ForeignChainAddress::Eth(
				[10; 20].into(),
			)),
			min_price: sqrt_price_to_price(bounded_sqrt_price(
				self.min_output.into(),
				input_amount.into(),
			)),
		}
	}
}

/// Creates a test swap and corresponding swap request. Both use the same ID.
fn create_test_swap(
	id: u64,
	input_asset: Asset,
	output_asset: Asset,
	amount: AssetAmount,
	dca_params: Option<DcaParameters>,
) -> Swap<Test> {
	SwapRequests::<Test>::insert(
		SwapRequestId::from(id),
		SwapRequest {
			id: id.into(),
			input_asset,
			output_asset,
			state: SwapRequestState::UserSwap {
				refund_params: None,
				output_action: SwapOutputAction::Egress {
					ccm_deposit_metadata: None,
					output_address: ForeignChainAddress::Eth(H160::zero()),
				},
				dca_state: DcaState::create_with_first_chunk(amount, dca_params).0,
				broker_fees: Default::default(),
			},
		},
	);

	Swap::new(
		id.into(),
		id.into(),
		input_asset,
		output_asset,
		amount,
		None,
		[FeeType::NetworkFee { min_fee_enforced: true }],
	)
}

// Returns some test data
fn generate_test_swaps() -> Vec<TestSwapParams> {
	vec![
		// asset -> USDC
		TestSwapParams {
			input_asset: Asset::Flip,
			output_asset: Asset::Usdc,
			input_amount: 100,
			refund_params: None,
			dca_params: None,
			output_address: ForeignChainAddress::Eth([2; 20].into()),
			is_ccm: false,
		},
		// USDC -> asset
		TestSwapParams {
			input_asset: Asset::Eth,
			output_asset: Asset::Usdc,
			input_amount: 40,
			refund_params: None,
			dca_params: None,
			output_address: ForeignChainAddress::Eth([9; 20].into()),
			is_ccm: false,
		},
		// Both assets are on the Eth chain
		TestSwapParams {
			input_asset: Asset::Flip,
			output_asset: Asset::Eth,
			input_amount: 500,
			refund_params: None,
			dca_params: None,
			output_address: ForeignChainAddress::Eth([2; 20].into()),
			is_ccm: false,
		},
		// Cross chain
		TestSwapParams {
			input_asset: Asset::Flip,
			output_asset: Asset::Dot,
			input_amount: 600,
			refund_params: None,
			dca_params: None,
			output_address: ForeignChainAddress::Dot(PolkadotAccountId::from_aliased([4; 32])),
			is_ccm: false,
		},
	]
}

fn insert_swaps(swaps: &[TestSwapParams]) {
	for (broker_id, swap) in swaps.iter().enumerate() {
		let ccm_deposit_metadata = if swap.is_ccm { Some(generate_ccm_deposit()) } else { None };

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
			swap.refund_params.clone(),
			swap.dca_params.clone(),
			SwapOrigin::Vault {
				tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
				broker_id: Some(BROKER),
			},
		);
	}
}

fn generate_ccm_channel() -> CcmChannelMetadata {
	CcmChannelMetadata {
		message: vec![0x01].try_into().unwrap(),
		gas_budget: GAS_BUDGET,
		ccm_additional_data: Default::default(),
	}
}
fn generate_ccm_deposit() -> CcmDepositMetadata {
	CcmDepositMetadata {
		source_chain: ForeignChain::Ethereum,
		source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
		channel_metadata: generate_ccm_channel(),
	}
}

fn get_broker_balance<T: Config>(who: &T::AccountId, asset: Asset) -> AssetAmount {
	T::BalanceApi::get_balance(who, asset)
}

#[track_caller]
fn assert_swaps_queue_is_empty() {
	assert_eq!(SwapQueue::<Test>::iter_keys().count(), 0);
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
			None,
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
				let output_amount = if swap.input_asset == Asset::Usdc {
					let fee = swap.input_amount * BROKER_FEE_BPS as u128 / 10_000;
					(swap.input_amount - fee) * DEFAULT_SWAP_RATE
				} else if swap.output_asset == Asset::Usdc {
					let output_before_fee = swap.input_amount * DEFAULT_SWAP_RATE;
					let fee = output_before_fee * BROKER_FEE_BPS as u128 / 10_000;
					output_before_fee - fee
				} else {
					let intermediate_amount = swap.input_amount * DEFAULT_SWAP_RATE;
					let fee = intermediate_amount * BROKER_FEE_BPS as u128 / 10_000;
					(intermediate_amount - fee) * DEFAULT_SWAP_RATE
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
		for (input, output) in iter::zip(expected, egresses) {
			assert_eq!(input, output);
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
fn expect_swap_id_to_be_emitted() {
	const AMOUNT: AssetAmount = 500;

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			// 1. Request a deposit address -> SwapDepositAddressReady
			assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
				RuntimeOrigin::signed(BROKER),
				Asset::Eth,
				Asset::Usdc,
				EncodedAddress::Eth(Default::default()),
				0,
				None,
				0,
				Default::default(),
				None,
				None,
			));

			// 2. Schedule the swap -> SwapScheduled
			swap_with_custom_broker_fee(Asset::Eth, Asset::Usdc, AMOUNT, bounded_vec![]);

			// 3. Process swaps -> SwapExecuted, SwapEgressScheduled
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapDepositAddressReady {
					deposit_address: EncodedAddress::Eth(..),
					destination_address: EncodedAddress::Eth(..),
					source_asset: Asset::Eth,
					destination_asset: Asset::Usdc,
					channel_id: 0,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SwapRequestId(1),
					..
				}),
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
					swap_request_id: SwapRequestId(1)
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
				None,
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
				None,
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
				SwapQueue::<Test>::get(SWAP_BLOCK),
				vec![Swap::new(
					1.into(),
					1.into(),
					INPUT_ASSET,
					OUTPUT_ASSET,
					AMOUNT,
					None,
					[FeeType::NetworkFee { min_fee_enforced: true }],
				)]
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
	const AMOUNT: AssetAmount = 1_000_000;
	new_test_ext().execute_with(|| {
		NetworkFee::set(Permill::from_parts(100));

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
			SwapQueue::<Test>::get(SWAP_EXECUTION_BLOCK),
			vec![
				Swap::new(
					1.into(),
					1.into(),
					Asset::Flip,
					Asset::Eth,
					AMOUNT,
					None,
					[FeeType::NetworkFee { min_fee_enforced: true }]
				),
				Swap::new(
					2.into(),
					2.into(),
					Asset::Btc,
					Asset::Eth,
					AMOUNT,
					None,
					[FeeType::NetworkFee { min_fee_enforced: true }]
				),
				Swap::new(
					3.into(),
					3.into(),
					Asset::Dot,
					Asset::Eth,
					AMOUNT,
					None,
					[FeeType::NetworkFee { min_fee_enforced: true }]
				),
				Swap::new(
					4.into(),
					4.into(),
					Asset::Usdc,
					Asset::Eth,
					AMOUNT,
					None,
					[FeeType::NetworkFee { min_fee_enforced: true }]
				),
			]
		);

		System::reset_events();
		// All swaps in the SwapQueue are executed.
		Swapping::on_finalize(SWAP_EXECUTION_BLOCK);
		assert_swaps_queue_is_empty();

		let usdc_amount_swapped_after_fee =
			Swapping::take_network_fee(AMOUNT * DEFAULT_SWAP_RATE, false).remaining_amount;
		let usdc_amount_deposited_after_fee =
			Swapping::take_network_fee(AMOUNT, false).remaining_amount;

		// Verify swap "from" -> STABLE_ASSET, then "to" -> Output Asset
		assert_eq!(
			Swaps::get(),
			vec![
				(Asset::Flip, Asset::Usdc, AMOUNT),
				(Asset::Dot, Asset::Usdc, AMOUNT),
				(Asset::Btc, Asset::Usdc, AMOUNT),
				(
					Asset::Usdc,
					Asset::Eth,
					usdc_amount_swapped_after_fee * 3 + usdc_amount_deposited_after_fee
				),
			]
		);

		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_request_id: SwapRequestId(1),
				egress_id: (ForeignChain::Ethereum, 1),
				amount,
				..
			}) if amount == usdc_amount_swapped_after_fee * DEFAULT_SWAP_RATE,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(2), .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_request_id: SwapRequestId(2),
				egress_id: (ForeignChain::Ethereum, 2),
				amount,
				..
			}) if amount == usdc_amount_swapped_after_fee * DEFAULT_SWAP_RATE,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(3), .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_request_id: SwapRequestId(3),
				egress_id: (ForeignChain::Ethereum, 3),
				amount,
				..
			}) if amount == usdc_amount_swapped_after_fee * DEFAULT_SWAP_RATE,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(4), .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_request_id: SwapRequestId(4),
				egress_id: (ForeignChain::Ethereum, 4),
				amount,
				..
			}) if amount == usdc_amount_deposited_after_fee * DEFAULT_SWAP_RATE,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
		);
	});
}

#[allow(deprecated)]
#[test]
fn can_handle_ccm_with_zero_swap_outputs() {
	const PRINCIPAL_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const PRINCIPAL_AMOUNT: AssetAmount = 9000;

	// Note: we use a constant to make sure we don't accidentally change the value
	const ZERO_AMOUNT: AssetAmount = 0;

	new_test_ext()
		.execute_with(|| {
			let eth_address = ForeignChainAddress::Eth(Default::default());
			let ccm = generate_ccm_deposit();

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
			SwapRate::set(0.0001f64);
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
			SwapRate::set(0.01f64);
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
					swap_request_id: SwapRequestId(1)
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
					swap_request_id: SwapRequestId(2)
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
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap::new(
				1.into(),
				1.into(),
				from,
				to,
				MAX_SWAP,
				None,
				[FeeType::NetworkFee { min_fee_enforced: true }]
			)]
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
					swap_request_id: SwapRequestId(1)
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(2), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(2),
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(2)
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
					swap_request_id: SwapRequestId(3)
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(4), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(4),
					..
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted {
					swap_request_id: SwapRequestId(4)
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
					execute_at: RETRY_AT_BLOCK
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(2),
					execute_at: RETRY_AT_BLOCK
				})
			);

			assert_eq!(SwapQueue::<Test>::get(RETRY_AT_BLOCK).len(), 2);
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
					swap_request_id: SwapRequestId(3)
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(4), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(4),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(4)
				}),
			);
		})
		.then_process_blocks_until_block(RETRY_AT_BLOCK)
		.then_execute_with(|_| {
			// Re-trying failed swaps originally scheduled for block 3 (which should
			// now be successful):
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(2), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(2),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(2)
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(1),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1)
				}),
			);
		});
}

#[test]
fn deposit_address_ready_event_contains_correct_parameters() {
	new_test_ext().execute_with(|| {
		let refund_parameters = ChannelRefundParametersEncoded {
			retry_duration: 10,
			refund_address: EncodedAddress::Eth([10; 20]),
			min_price: 100.into(),
		};

		let dca_parameters = DcaParameters { number_of_chunks: 5, chunk_interval: 2 };

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
			Some(refund_parameters.clone()),
			Some(dca_parameters.clone()),
		));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapDepositAddressReady {
				boost_fee: BOOST_FEE,
				refund_parameters: Some(ref refund_params_in_event),
				dca_parameters: Some(ref dca_params_in_event),
				..
			}) if *refund_params_in_event == refund_parameters && dca_params_in_event == &dca_parameters
		);
	});
}

#[test]
fn test_get_scheduled_swap_legs() {
	new_test_ext().execute_with(|| {
		const INIT_AMOUNT: AssetAmount = 1000;

		let swaps = vec![
			create_test_swap(1, Asset::Flip, Asset::Usdc, INIT_AMOUNT, None),
			create_test_swap(2, Asset::Usdc, Asset::Flip, INIT_AMOUNT, None),
			create_test_swap(3, Asset::Btc, Asset::Eth, INIT_AMOUNT, None),
			create_test_swap(4, Asset::Flip, Asset::Btc, INIT_AMOUNT, None),
			create_test_swap(5, Asset::Eth, Asset::Flip, INIT_AMOUNT, None),
		];

		SwapRate::set(2f64);
		// The amount of USDC in the middle of swap (5):
		const INTERMEDIATE_AMOUNT: AssetAmount = 2000;

		// The test is more useful when these aren't equal:
		assert_ne!(INIT_AMOUNT, INTERMEDIATE_AMOUNT);

		assert_eq!(
			Swapping::get_scheduled_swap_legs(swaps, Asset::Flip, None),
			vec![
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
				SwapLegInfo {
					swap_id: SwapId(5),
					swap_request_id: SwapRequestId(5),
					base_asset: Asset::Flip,
					quote_asset: Asset::Usdc,
					side: Side::Buy,
					amount: INTERMEDIATE_AMOUNT,
					source_asset: Some(Asset::Eth),
					source_amount: Some(INIT_AMOUNT),
					remaining_chunks: 0,
					chunk_interval: SWAP_DELAY_BLOCKS,
				},
			]
		);
	});
}

#[test]
fn test_get_scheduled_swap_legs_fallback() {
	new_test_ext().execute_with(|| {
		const INIT_AMOUNT: AssetAmount = 1000000000000000000000;
		const PRICE: u128 = 2;

		let swaps = vec![
			create_test_swap(1, Asset::Flip, Asset::Eth, INIT_AMOUNT, None),
			create_test_swap(2, Asset::Eth, Asset::Usdc, INIT_AMOUNT, None),
		];

		// Setting the swap rate to something different from the price so that if the fallback is
		// not used, it will give a different result, avoiding a false positive.
		SwapRate::set(PRICE.checked_add(1).unwrap() as f64);

		// The swap simulation must fail for it to use the fallback price estimation
		MockSwappingApi::set_swaps_should_fail(true);

		let sqrt_price = price_to_sqrt_price((U256::from(PRICE)) << PRICE_FRACTIONAL_BITS);

		assert_eq!(
			Swapping::get_scheduled_swap_legs(swaps, Asset::Eth, Some(sqrt_price)),
			vec![
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
				}
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
		SwapRate::set(1_f64);

		let dca_params =
			DcaParameters { number_of_chunks: NUMBER_OF_CHUNKS, chunk_interval: CHUNK_INTERVAL };

		let swaps =
			vec![create_test_swap(1, Asset::Flip, Asset::Eth, INIT_AMOUNT, Some(dca_params))];

		assert_eq!(
			Swapping::get_scheduled_swap_legs(swaps, Asset::Eth, None),
			vec![SwapLegInfo {
				swap_id: SwapId(1),
				swap_request_id: SwapRequestId(1),
				base_asset: Asset::Eth,
				quote_asset: Asset::Usdc,
				side: Side::Buy,
				amount: INIT_AMOUNT,
				source_asset: Some(Asset::Flip),
				source_amount: Some(INIT_AMOUNT),
				// This is the first chunk, so there are 2 remaining
				remaining_chunks: NUMBER_OF_CHUNKS - 1,
				chunk_interval: CHUNK_INTERVAL,
			},]
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
fn swap_output_amounts_correctly_account_for_fees() {
	for (from, to) in
		// non-stable to non-stable, non-stable to stable, stable to non-stable
		[(Asset::Btc, Asset::Eth), (Asset::Btc, Asset::Usdc), (Asset::Usdc, Asset::Eth)]
	{
		new_test_ext().execute_with(|| {
			const INPUT_AMOUNT: AssetAmount = 1000;

			let network_fee = Permill::from_percent(1);
			NetworkFee::set(network_fee);

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
							output_address: ForeignChainAddress::Eth(H160::zero()),
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
		const SWAP_AMOUNT: AssetAmount = 1000;
		const NETWORK_FEE: Permill = Permill::from_percent(2);

		NetworkFee::set(NETWORK_FEE);

		// Get some network fees, just like we did a swap.
		let FeeTaken { remaining_amount, fee: network_fee } =
			Swapping::take_network_fee(SWAP_AMOUNT, false);

		// Sanity check the network fee.
		assert_eq!(network_fee, CollectedNetworkFee::<Test>::get());
		assert_eq!(network_fee, 20);
		assert_eq!(remaining_amount + network_fee, SWAP_AMOUNT);

		// The default buy interval is zero. Check that buy back is disabled & on_initialize does
		// not panic.
		assert_eq!(FlipBuyInterval::<Test>::get(), 0);
		Swapping::on_initialize(1);
		assert_eq!(network_fee, CollectedNetworkFee::<Test>::get());

		// Set a non-zero buy interval
		FlipBuyInterval::<Test>::set(INTERVAL);

		// Nothing is bought if we're not at the interval.
		Swapping::on_initialize(INTERVAL * 3 - 1);
		assert_eq!(network_fee, CollectedNetworkFee::<Test>::get());

		// If we're at an interval, we should buy flip.
		Swapping::on_initialize(INTERVAL * 3);
		assert_eq!(0, CollectedNetworkFee::<Test>::get());

		// Note that the network fee will not be charged in this case:
		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS))
				.first()
				.expect("Should have scheduled a swap usdc -> flip"),
			&Swap::new(1.into(), 1.into(), STABLE_ASSET, Asset::Flip, network_fee, None, [],)
		);
	});
}

#[test]
fn test_calculate_input_for_gas_output() {
	use cf_chains::assets::eth::Asset as EthereumAsset;
	const FLIP: EthereumAsset = EthereumAsset::Flip;

	new_test_ext().execute_with(|| {
		// If swap simulation fails -> no conversion.
		MockSwappingApi::set_swaps_should_fail(true);
		assert!(Swapping::calculate_input_for_gas_output::<Ethereum>(FLIP, 1000).is_none());

		// Set swap rate to 2 and turn swaps back on.
		SwapRate::set(2_f64);
		MockSwappingApi::set_swaps_should_fail(false);

		// Desired output is zero -> trivially ok.
		assert_eq!(Swapping::calculate_input_for_gas_output::<Ethereum>(FLIP, 0), Some(0));

		// Desired output requires 2 swap legs, each with a swap rate of 2. So output should be
		// 1/4th of input.
		assert_eq!(Swapping::calculate_input_for_gas_output::<Ethereum>(FLIP, 1000), Some(250));

		// Desired output is gas asset, requires 1 swap leg. So output should be 1/2 of input.
		assert_eq!(
			Swapping::calculate_input_for_gas_output::<Ethereum>(EthereumAsset::Usdc, 1000),
			Some(500)
		);

		// Input is gas asset -> trivially ok.
		assert_eq!(
			Swapping::calculate_input_for_gas_output::<Ethereum>(
				cf_chains::assets::eth::GAS_ASSET,
				1000
			),
			Some(1000)
		);
	});
}

#[test]
fn test_fee_estimation_basis() {
	for asset in Asset::all() {
		if !asset.is_gas_asset() {
			assert!(
				utilities::fee_estimation_basis(asset).is_some(),
	             "No fee estimation cap defined for {:?}. Add one to the fee_estimation_basis function definition.",
	             asset,
	         );
		}
	}
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
			}
		}
	}

	#[test]
	fn single_swap() {
		let swap1 = Swap::new(0.into(), 0.into(), Asset::Btc, Asset::Usdc, 1000, None, []);
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
		let swap1 = Swap::new(0.into(), 0.into(), Asset::Btc, Asset::Usdc, 500, None, []);
		let swap2 = Swap::new(1.into(), 1.into(), Asset::Btc, Asset::Eth, 1000, None, []);
		let swap3 = Swap::new(2.into(), 2.into(), Asset::Eth, Asset::Usdc, 1000, None, []);

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
		let swap1 = Swap::new(1.into(), 1.into(), Asset::Btc, Asset::Eth, 1, None, []);
		let swap2 = Swap::new(2.into(), 2.into(), Asset::Usdc, Asset::Eth, 1000, None, []);
		let swap3 = Swap::new(3.into(), 3.into(), Asset::Eth, Asset::Usdc, 100, None, []);

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
				NetworkFee::set(Permill::from_percent(1));

				let swap = |input_asset: Asset, output_asset: Asset, input_amount: AssetAmount| {
					TestSwapParams {
						input_asset,
						output_asset,
						input_amount,
						refund_params: None,
						dca_params: None,
						output_address: ForeignChainAddress::Eth([2; 20].into()),
						is_ccm: false,
					}
				};

				let swap1 = swap(Asset::Btc, Asset::Eth, 100_000);
				let swap2 = swap(Asset::Usdc, Asset::Eth, 150_000);

				insert_swaps(&[swap1, swap2]);

				// This amount of liquidity would only be enough for one of the swaps
				// (USDC liquidity is not set is thus will not be checked):
				MockSwappingApi::add_liquidity(Asset::Eth, 500_000);
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
						execute_at: SWAP_RESCHEDULED_BLOCK
					}),
				);

				// Ensure that storage has been reverted from the first (failed) attempt
				// by checking the network fee (which should only be collected
				// from swap 2):
				assert_eq!(CollectedNetworkFee::<Test>::get(), 1500);

				// Adding some more liquidity to make the other swap succeed:
				MockSwappingApi::add_liquidity(Asset::Eth, 500_000);
			})
			.then_process_blocks_until_block(SWAP_RESCHEDULED_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
				);

				assert_eq!(CollectedNetworkFee::<Test>::get(), 1500 + 2000);
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
				NetworkFee::set(Permill::from_percent(1));
				let swap = |input_asset: Asset, output_asset: Asset, input_amount: AssetAmount| {
					TestSwapParams {
						input_asset,
						output_asset,
						input_amount,
						refund_params: None,
						dca_params: None,
						output_address: ForeignChainAddress::Eth([2; 20].into()),
						is_ccm: false,
					}
				};

				let swap1 = swap(Asset::Btc, Asset::Eth, 100_000);
				let swap2 = swap(Asset::Eth, Asset::Usdc, 150_000);

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
mod on_chain_swapping {

	use cf_traits::{mocks::balance_api::MockBalance, SwapOutputActionEncoded};

	use super::*;

	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Flip;

	const INPUT_AMOUNT: AssetAmount = 1000;

	#[test]
	fn swap_into_internal_balance() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const EXPECTED_OUTPUT_AMOUNT: AssetAmount =
			INPUT_AMOUNT * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE;

		let min_price = U256::from(DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE) << PRICE_FRACTIONAL_BITS;

		new_test_ext()
			.execute_with(|| {
				MockBalance::credit_account(&LP_ACCOUNT, INPUT_ASSET, INPUT_AMOUNT);

				assert_ok!(Swapping::internal_swap(
					RuntimeOrigin::signed(LP_ACCOUNT),
					INPUT_AMOUNT,
					INPUT_ASSET,
					OUTPUT_ASSET,
					0,
					min_price,
					None,
				));

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequested {
						input_asset: INPUT_ASSET,
						input_amount: INPUT_AMOUNT,
						output_asset: OUTPUT_ASSET,
						origin: SwapOrigin::Internal,
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
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { .. }),
				);

				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, INPUT_ASSET), 0);
				assert_eq!(
					MockBalance::get_balance(&LP_ACCOUNT, OUTPUT_ASSET),
					EXPECTED_OUTPUT_AMOUNT
				);
			});
	}

	#[test]
	fn swap_on_chain_with_refund() {
		const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + SWAP_DELAY_BLOCKS as u64;

		const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / 2;

		let min_price = U256::from(DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE) << PRICE_FRACTIONAL_BITS;

		new_test_ext()
			.execute_with(|| {
				MockBalance::credit_account(&LP_ACCOUNT, INPUT_ASSET, INPUT_AMOUNT);

				assert_ok!(Swapping::internal_swap(
					RuntimeOrigin::signed(LP_ACCOUNT),
					INPUT_AMOUNT,
					INPUT_ASSET,
					OUTPUT_ASSET,
					0,
					min_price,
					Some(DcaParameters { number_of_chunks: 2, chunk_interval: 2 }),
				));

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequested {
						input_asset: INPUT_ASSET,
						input_amount: INPUT_AMOUNT,
						output_asset: OUTPUT_ASSET,
						origin: SwapOrigin::Internal,
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
				SwapRate::set(DEFAULT_SWAP_RATE as f64 / 2.0);
			})
			.then_process_blocks_until_block(CHUNK_2_BLOCK)
			.then_execute_with(|_| {
				// There is no "SwapRefunded" event...

				// Only one chunk is expected to be swapped:
				const EXPECTED_OUTPUT_AMOUNT: AssetAmount =
					CHUNK_AMOUNT * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE;

				assert_eq!(MockBalance::get_balance(&LP_ACCOUNT, INPUT_ASSET), CHUNK_AMOUNT);
				assert_eq!(
					MockBalance::get_balance(&LP_ACCOUNT, OUTPUT_ASSET),
					EXPECTED_OUTPUT_AMOUNT
				);
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
	fn register_affiliate() {
		new_test_ext().execute_with(|| {
			const SHORT_ID: AffiliateShortId = AffiliateShortId(0);

			// Only brokers can register affiliates
			assert_noop!(
				Swapping::register_affiliate(OriginTrait::signed(ALICE), BOB, SHORT_ID),
				BadOrigin
			);
			assert_eq!(Swapping::get_short_id(&BROKER, &BOB), None);

			// Registering an affiliate for the first time (no existing records)
			{
				assert_ok!(Swapping::register_affiliate(
					OriginTrait::signed(BROKER),
					ALICE,
					SHORT_ID,
				));

				System::assert_has_event(RuntimeEvent::Swapping(
					Event::<Test>::AffiliateRegistrationUpdated {
						broker_id: BROKER,
						affiliate_short_id: SHORT_ID,
						affiliate_id: ALICE,
						previous_affiliate_id: None,
					},
				));
				assert_eq!(Swapping::get_short_id(&BROKER, &ALICE), Some(SHORT_ID));
			}

			// Overwriting an existing affiliate registration entry.
			{
				assert_ok!(Swapping::register_affiliate(
					OriginTrait::signed(BROKER),
					BOB,
					SHORT_ID,
				));

				System::assert_has_event(RuntimeEvent::Swapping(
					Event::<Test>::AffiliateRegistrationUpdated {
						broker_id: BROKER,
						affiliate_short_id: SHORT_ID,
						affiliate_id: BOB,
						previous_affiliate_id: Some(ALICE),
					},
				));
				assert_eq!(Swapping::get_short_id(&BROKER, &BOB), Some(SHORT_ID));
				assert_eq!(Swapping::get_short_id(&BROKER, &ALICE), None);
			}
		});
	}

	#[test]
	fn default_broker_bond() {
		new_test_ext().execute_with(|| {
			assert_eq!(BrokerBond::<Test>::get(), FLIPPERINOS_PER_FLIP * 100);
		});
	}
}
