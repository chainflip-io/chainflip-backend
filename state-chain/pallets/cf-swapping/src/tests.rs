mod ccm;
mod config;
mod dca;
mod fees;
mod fill_or_kill;

use std::sync::LazyLock;

use super::*;
use crate::{
	mock::{RuntimeEvent, *},
	CcmFailReason, CollectedRejectedFunds, Error, Event, MaximumSwapAmount, Pallet, Swap,
	SwapOrigin, SwapQueue, SwapType,
};
use cf_amm::common::{price_to_sqrt_price, PRICE_FRACTIONAL_BITS};
use cf_chains::{
	address::{to_encoded_address, AddressConverter, EncodedAddress, ForeignChainAddress},
	btc::{BitcoinNetwork, ScriptPubkey},
	dot::PolkadotAccountId,
	AnyChain, CcmChannelMetadata, CcmDepositMetadata, Ethereum,
};
use cf_primitives::{
	Asset, AssetAmount, BasisPoints, Beneficiary, DcaParameters, ForeignChain, NetworkEnvironment,
};
use cf_test_utilities::{assert_event_sequence, assert_events_eq, assert_has_matching_event};
use cf_traits::{
	mocks::{
		address_converter::MockAddressConverter,
		egress_handler::{MockEgressHandler, MockEgressParameter},
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

const GAS_BUDGET: AssetAmount = 1_000u128;
const INPUT_AMOUNT: AssetAmount = 40_000;
const SWAP_REQUEST_ID: u64 = 1;
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
	refund_params: Option<ChannelRefundParameters>,
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
			refund_params: refund_params.map(|params| params.into_channel_params(INPUT_AMOUNT)),
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
	retry_duration: u32,
	min_output: AssetAmount,
}

impl TestRefundParams {
	fn into_channel_params(self, input_amount: AssetAmount) -> ChannelRefundParameters {
		use cf_amm::common::{bounded_sqrt_price, sqrt_price_to_price};

		ChannelRefundParameters {
			retry_duration: self.retry_duration,
			refund_address: ForeignChainAddress::Eth([10; 20].into()),
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
		id,
		SwapRequest {
			id,
			input_asset,
			output_asset,
			refund_params: None,
			state: SwapRequestState::UserSwap {
				output_address: ForeignChainAddress::Eth(H160::zero()),
				dca_state: DcaState::create_with_first_chunk(amount, dca_params).0,
				ccm: None,
				broker_fees: Default::default(),
			},
		},
	);

	Swap::new(id, id, input_asset, output_asset, amount, None, [FeeType::NetworkFee])
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

#[track_caller]
fn assert_failed_ccm(
	from: Asset,
	amount: AssetAmount,
	output: Asset,
	destination_address: ForeignChainAddress,
	ccm: CcmDepositMetadata,
	reason: CcmFailReason,
) {
	assert!(Swapping::init_swap_request(
		from,
		amount,
		output,
		SwapRequestType::Ccm {
			ccm_deposit_metadata: ccm.clone(),
			output_address: destination_address.clone()
		},
		Default::default(),
		None,
		None,
		SwapOrigin::Vault { tx_hash: Default::default() },
	)
	.is_err());

	assert_event_sequence!(
		Test,
		RuntimeEvent::Swapping(Event::SwapRequested { .. }),
		RuntimeEvent::Swapping(Event::CcmFailed {
			reason: ref reason_in_event,
			destination_address: ref address_in_event,
			deposit_metadata: ref metadata_in_event,
			..
		}) if reason_in_event == &reason && address_in_event == &MockAddressConverter::to_encoded_address(destination_address) && metadata_in_event == &ccm.to_encoded::<<Test as pallet::Config>::AddressConverter>(),
		RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
	);
}

fn insert_swaps(swaps: &[TestSwapParams]) {
	for (broker_id, swap) in swaps.iter().enumerate() {
		let request_type = if swap.is_ccm {
			SwapRequestType::Ccm {
				output_address: swap.output_address.clone(),
				ccm_deposit_metadata: CcmDepositMetadata {
					source_chain: ForeignChain::Ethereum,
					source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
					channel_metadata: generate_ccm_channel(),
				},
			}
		} else {
			SwapRequestType::Regular { output_address: swap.output_address.clone() }
		};

		assert_ok!(Swapping::init_swap_request(
			swap.input_asset,
			swap.input_amount,
			swap.output_asset,
			request_type,
			bounded_vec![Beneficiary { account: broker_id as u64, bps: BROKER_FEE_BPS }],
			swap.refund_params.clone(),
			swap.dca_params.clone(),
			SwapOrigin::Vault { tx_hash: Default::default() },
		));
	}
}

fn generate_ccm_channel() -> CcmChannelMetadata {
	CcmChannelMetadata {
		message: vec![0x01].try_into().unwrap(),
		gas_budget: GAS_BUDGET,
		cf_parameters: Default::default(),
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

fn credit_broker_account<T: Config>(who: &T::AccountId, asset: Asset, amount: AssetAmount) {
	assert_ok!(T::BalanceApi::try_credit_account(who, asset, amount));
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
	assert_ok!(Swapping::init_swap_request(
		from,
		amount,
		to,
		SwapRequestType::Regular { output_address: ForeignChainAddress::Eth(Default::default()) },
		broker_fees,
		None,
		None,
		SwapOrigin::DepositChannel {
			deposit_address: MockAddressConverter::to_encoded_address(ForeignChainAddress::Eth(
				[0; 20].into(),
			)),
			channel_id: 1,
			deposit_block_height: 0,
		},
	));
}

#[test]
fn request_swap_success_with_valid_parameters() {
	new_test_ext().execute_with(|| {
		assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
			RuntimeOrigin::signed(ALICE),
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
		assert_ok!(Swapping::init_swap_request(
			Asset::Eth,
			10,
			Asset::Dot,
			SwapRequestType::Regular { output_address: ForeignChainAddress::Eth([2; 20].into()) },
			Default::default(),
			None,
			None,
			SwapOrigin::Vault { tx_hash: Default::default() },
		));

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
				RuntimeOrigin::signed(ALICE),
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
				RuntimeEvent::Swapping(Event::SwapRequested { swap_request_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: 1,
					swap_id: 1,
					input_amount: AMOUNT,
					..
				})
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: 1,
					egress_id: (ForeignChain::Ethereum, 1),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 1 }),
			);
		});
}

#[test]
fn reject_invalid_ccm_deposit() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let ccm = generate_ccm_deposit();

		assert_noop!(
			Swapping::ccm_deposit(
				RuntimeOrigin::root(),
				Asset::Btc,
				1_000_000,
				Asset::Eth,
				EncodedAddress::Dot(Default::default()),
				ccm.clone(),
				Default::default(),
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);

		assert_noop!(
			Swapping::ccm_deposit(
				RuntimeOrigin::root(),
				Asset::Btc,
				1_000_000,
				Asset::Eth,
				EncodedAddress::Dot(Default::default()),
				ccm.clone(),
				Default::default(),
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);

		assert_failed_ccm(
			Asset::Eth,
			1_000_000,
			Asset::Dot,
			ForeignChainAddress::Dot(Default::default()),
			ccm.clone(),
			CcmFailReason::UnsupportedForTargetChain,
		);

		System::reset_events();

		assert_failed_ccm(
			Asset::Eth,
			1_000_000,
			Asset::Btc,
			ForeignChainAddress::Btc(cf_chains::btc::ScriptPubkey::P2PKH(Default::default())),
			ccm.clone(),
			CcmFailReason::UnsupportedForTargetChain,
		);

		System::reset_events();

		assert_failed_ccm(
			Asset::Eth,
			gas_budget - 1,
			Asset::Eth,
			ForeignChainAddress::Eth(Default::default()),
			ccm,
			CcmFailReason::InsufficientDepositAmount,
		);
	});
}

#[test]
fn rejects_invalid_swap_deposit() {
	new_test_ext().execute_with(|| {
		let ccm = generate_ccm_channel();

		assert_noop!(
			Swapping::request_swap_deposit_address_with_affiliates(
				RuntimeOrigin::signed(ALICE),
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
				RuntimeOrigin::signed(ALICE),
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
fn rejects_invalid_swap_by_witnesser() {
	new_test_ext().execute_with(|| {
		let script_pubkey = ScriptPubkey::try_from_address(
			"BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
			&BitcoinNetwork::Mainnet,
		)
		.unwrap();

		let btc_encoded_address =
			to_encoded_address(ForeignChainAddress::Btc(script_pubkey), || {
				NetworkEnvironment::Mainnet
			});

		// Is valid Bitcoin address, but asset is Dot, so not compatible
		assert_noop!(
			Swapping::schedule_swap_from_contract(
				RuntimeOrigin::root(),
				Asset::Eth,
				Asset::Dot,
				10000,
				btc_encoded_address,
				Default::default()
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);

		assert_noop!(
			Swapping::schedule_swap_from_contract(
				RuntimeOrigin::root(),
				Asset::Eth,
				Asset::Btc,
				10000,
				EncodedAddress::Btc(vec![0x41, 0x80, 0x41]),
				Default::default()
			),
			Error::<Test>::InvalidDestinationAddress
		);
	});
}

#[test]
fn swap_by_witnesser_happy_path() {
	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	const AMOUNT: AssetAmount = 1_000u128;
	const ORIGIN: SwapOrigin = SwapOrigin::Vault { tx_hash: [0; 32] };

	let encoded_output_address = EncodedAddress::Eth(Default::default());

	new_test_ext().execute_with(|| {
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::root(),
			INPUT_ASSET,
			OUTPUT_ASSET,
			AMOUNT,
			encoded_output_address.clone(),
			Default::default(),
		));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		// Verify this swap is accepted and scheduled
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(1, 1, INPUT_ASSET, OUTPUT_ASSET, AMOUNT, None, [FeeType::NetworkFee],)]
		);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
			swap_request_id: 1,
			swap_id: 1,
			swap_type: SwapType::Swap,
			input_amount: AMOUNT,
			execute_at,
		}));

		System::assert_has_event(RuntimeEvent::Swapping(Event::SwapRequested {
			swap_request_id: SWAP_REQUEST_ID,
			input_asset: INPUT_ASSET,
			output_asset: OUTPUT_ASSET,
			input_amount: AMOUNT,
			request_type: SwapRequestTypeEncoded::Regular {
				output_address: encoded_output_address,
			},
			refund_parameters: None,
			dca_parameters: None,
			origin: ORIGIN,
		}));

		// Confiscated fund is unchanged
		assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
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
					1,
					1,
					INPUT_ASSET,
					OUTPUT_ASSET,
					AMOUNT,
					None,
					[FeeType::NetworkFee],
				)]
			);

			assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
				swap_request_id: 1,
				swap_id: 1,
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
	new_test_ext()
		.execute_with(|| {
			NetworkFee::set(Permill::from_parts(100));

			[Asset::Flip, Asset::Btc, Asset::Dot, Asset::Usdc]
				.into_iter()
				.for_each(|asset| {
					assert_ok!(Swapping::schedule_swap_from_contract(
						RuntimeOrigin::root(),
						asset,
						Asset::Eth,
						AMOUNT,
						EncodedAddress::Eth(Default::default()),
						Default::default(),
					));
				});

			assert_eq!(
				SwapQueue::<Test>::get(SWAP_EXECUTION_BLOCK),
				vec![
					Swap::new(1, 1, Asset::Flip, Asset::Eth, AMOUNT, None, [FeeType::NetworkFee]),
					Swap::new(2, 2, Asset::Btc, Asset::Eth, AMOUNT, None, [FeeType::NetworkFee]),
					Swap::new(3, 3, Asset::Dot, Asset::Eth, AMOUNT, None, [FeeType::NetworkFee]),
					Swap::new(4, 4, Asset::Usdc, Asset::Eth, AMOUNT, None, [FeeType::NetworkFee]),
				]
			);
		})
		.then_process_blocks_until_block(SWAP_EXECUTION_BLOCK)
		.then_execute_with(|_| {
			assert_swaps_queue_is_empty();

			let usdc_amount_swapped_after_fee =
				Swapping::take_network_fee(AMOUNT * DEFAULT_SWAP_RATE).remaining_amount;
			let usdc_amount_deposited_after_fee =
				Swapping::take_network_fee(AMOUNT).remaining_amount;

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
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: 1,
					egress_id: (ForeignChain::Ethereum, 1),
					amount,
					..
				}) if amount == usdc_amount_swapped_after_fee * DEFAULT_SWAP_RATE,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: 2,
					egress_id: (ForeignChain::Ethereum, 2),
					amount,
					..
				}) if amount == usdc_amount_swapped_after_fee * DEFAULT_SWAP_RATE,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: 3,
					egress_id: (ForeignChain::Ethereum, 3),
					amount,
					..
				}) if amount == usdc_amount_swapped_after_fee * DEFAULT_SWAP_RATE,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 4, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: 4,
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
	const GAS_SWAP_BLOCK: u64 = PRINCIPAL_SWAP_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const PRINCIPAL_AMOUNT: AssetAmount = 9000;

	// Note: we use a constant to make sure we don't accidentally change the value
	const ZERO_AMOUNT: AssetAmount = 0;

	new_test_ext()
		.execute_with(|| {
			let eth_address = ForeignChainAddress::Eth(Default::default());
			let ccm = generate_ccm_deposit();

			assert_ok!(Swapping::init_swap_request(
				Asset::Usdc,
				PRINCIPAL_AMOUNT + GAS_BUDGET,
				Asset::Eth,
				SwapRequestType::Ccm {
					ccm_deposit_metadata: ccm.clone(),
					output_address: eth_address
				},
				Default::default(),
				None,
				None,
				SwapOrigin::Vault { tx_hash: Default::default() },
			));

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
					swap_request_id: 1,
					swap_id: 1,
					network_fee: 0,
					broker_fee: 0,
					input_amount: PRINCIPAL_AMOUNT,
					input_asset: Asset::Usdc,
					output_asset: Asset::Eth,
					output_amount: ZERO_AMOUNT,
					intermediate_amount: None,
				}),
			);
		})
		.then_process_blocks_until_block(GAS_SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_request_id: 1,
					swap_id: 2,
					network_fee: 0,
					broker_fee: 0,
					input_amount: GAS_BUDGET,
					input_asset: Asset::Usdc,
					output_asset: Asset::Eth,
					output_amount: ZERO_AMOUNT,
					intermediate_amount: None,
				}),
			);

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
					swap_id: 1,
					output_asset: Asset::Eth,
					output_amount: 0,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressIgnored { swap_request_id: 1, .. }),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted { swap_request_id: 1 }),
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_id: 2,
					output_asset: Asset::Eth,
					output_amount: 0,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressIgnored { swap_request_id: 2, .. }),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted { swap_request_id: 2 }),
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
fn swap_excess_are_confiscated_ccm_via_deposit() {
	const MAX_SWAP: AssetAmount = 2_000;
	const PRINCIPAL_AMOUNT: AssetAmount = 10_000;
	const INPUT_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT + GAS_BUDGET;
	const CONFISCATED_AMOUNT: AssetAmount = INPUT_AMOUNT - MAX_SWAP;

	new_test_ext().execute_with(|| {
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;
		let request_ccm = generate_ccm_channel();
		let ccm = generate_ccm_deposit();

		set_maximum_swap_amount(from, Some(MAX_SWAP));

		// Register CCM via Swap deposit
		assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			EncodedAddress::Eth(Default::default()),
			0,
			Some(request_ccm),
			0,
			Default::default(),
			None,
			None,
		));

		assert_ok!(Swapping::init_swap_request(
			from,
			INPUT_AMOUNT,
			to,
			SwapRequestType::Ccm {
				ccm_deposit_metadata: ccm.clone(),
				output_address: ForeignChainAddress::Eth(Default::default())
			},
			Default::default(),
			None,
			None,
			SwapOrigin::Vault { tx_hash: Default::default() },
		));

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_request_id: 1,
			asset: from,
			total_amount: INPUT_AMOUNT,
			confiscated_amount: CONFISCATED_AMOUNT,
		}));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(1, 1, from, to, MAX_SWAP - GAS_BUDGET, None, [FeeType::NetworkFee]),]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), CONFISCATED_AMOUNT);
	});
}

#[test]
fn swap_excess_are_confiscated_ccm_via_extrinsic() {
	const MAX_SWAP: AssetAmount = GAS_BUDGET + 100;
	const PRINCIPAL_AMOUNT: AssetAmount = 1_000;
	const INPUT_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT + GAS_BUDGET;
	const CONFISCATED_AMOUNT: AssetAmount = INPUT_AMOUNT - MAX_SWAP;

	new_test_ext().execute_with(|| {
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;
		let ccm = generate_ccm_deposit();

		set_maximum_swap_amount(from, Some(MAX_SWAP));

		// Register CCM via Swap deposit
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			INPUT_AMOUNT,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm,
			Default::default(),
		));

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_request_id: 1,
			asset: from,
			total_amount: INPUT_AMOUNT,
			confiscated_amount: CONFISCATED_AMOUNT,
		}));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(1, 1, from, to, MAX_SWAP - GAS_BUDGET, None, [FeeType::NetworkFee]),]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), CONFISCATED_AMOUNT);
	});
}

#[test]
fn swap_excess_are_confiscated_for_swap_via_extrinsic() {
	const MAX_SWAP: AssetAmount = 100;
	const AMOUNT: AssetAmount = 1_000;
	const CONFISCATED_AMOUNT: AssetAmount = AMOUNT - MAX_SWAP;

	new_test_ext().execute_with(|| {
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		set_maximum_swap_amount(from, Some(MAX_SWAP));

		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			AMOUNT,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_request_id: 1,
			asset: from,
			total_amount: AMOUNT,
			confiscated_amount: CONFISCATED_AMOUNT,
		}));

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap::new(1, 1, from, to, MAX_SWAP, None, [FeeType::NetworkFee])]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), CONFISCATED_AMOUNT);
	});
}

#[test]
fn swap_excess_are_confiscated_for_swap_via_deposit() {
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
			swap_request_id: 1,
			asset: from,
			total_amount: AMOUNT,
			confiscated_amount: CONFISCATED_AMOUNT,
		}));

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap::new(1, 1, from, to, MAX_SWAP, None, [FeeType::NetworkFee])]
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
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 1, execute_at: 3, .. }),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 2, execute_at: 3, .. }),
			);
		})
		.then_execute_at_next_block(|_| {
			// Block 2, swaps should be scheduled at block 4
			assert_eq!(System::block_number(), 2);
			insert_swaps(&later_swaps);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 3, execute_at: 4, .. }),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 4, execute_at: 4, .. }),
			);
		})
		.then_execute_at_next_block(|_| {
			// First group of swaps will be processed at the end of this block
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 3);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 1, .. }),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted { swap_request_id: 1 }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 2, .. }),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted { swap_request_id: 2 }),
			);
		})
		.then_execute_at_next_block(|_| {
			// Second group of swaps will be processed at the end of this block
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 4);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 3, .. }),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted { swap_request_id: 3 }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 4, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 4, .. }),
				RuntimeEvent::Swapping(Event::<Test>::SwapRequestCompleted { swap_request_id: 4 }),
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
					swap_id: 1,
					execute_at: EXECUTE_AT_BLOCK,
					..
				}),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: 2,
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
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 3, execute_at: 4, .. }),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 4, execute_at: 4, .. }),
			);
		})
		.then_execute_at_next_block(|_| {
			// First group of swaps will be processed at the end of this block,
			// but we force them to fail:
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 3);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: 1,
					execute_at: RETRY_AT_BLOCK
				}),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: 2,
					execute_at: RETRY_AT_BLOCK
				}),
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
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 3 }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 4, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 4, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 4 }),
			);
		})
		.then_process_blocks_until_block(RETRY_AT_BLOCK)
		.then_execute_with(|_| {
			// Re-trying failed swaps originally scheduled for block 3 (which should
			// now be successful):
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 1 }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 2 }),
			);
		});
}

#[test]
fn deposit_address_ready_event_contains_correct_parameters() {
	new_test_ext().execute_with(|| {
		let refund_parameters = ChannelRefundParameters {
			retry_duration: 10,
			refund_address: ForeignChainAddress::Eth([10; 20].into()),
			min_price: 100.into(),
		};

		let dca_parameters = DcaParameters { number_of_chunks: 5, chunk_interval: 2 };

		const BOOST_FEE: u16 = 100;
		assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
			RuntimeOrigin::signed(ALICE),
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
			}) if refund_params_in_event == &refund_parameters.map_address(MockAddressConverter::to_encoded_address) && dca_params_in_event == &dca_parameters
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
					swap_id: 1,
					swap_request_id: 1,
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
					swap_id: 2,
					swap_request_id: 2,
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
					swap_id: 4,
					swap_request_id: 4,
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
					swap_id: 5,
					swap_request_id: 5,
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
					swap_id: 1,
					swap_request_id: 1,
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
					swap_id: 2,
					swap_request_id: 2,
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
				swap_id: 1,
				swap_request_id: 1,
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
fn register_and_deregister_account() {
	new_test_ext().execute_with(|| {
		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_broker(
			OriginTrait::signed(ALICE),
		)
		.expect("ALICE was registered in test setup.");

		// Earn some fees.
		credit_broker_account::<Test>(&ALICE, Asset::Eth, 100);

		assert_noop!(
			Swapping::deregister_as_broker(OriginTrait::signed(ALICE)),
			Error::<Test>::EarnedFeesNotWithdrawn,
		);

		assert_ok!(Swapping::withdraw(
			OriginTrait::signed(ALICE),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
		assert_ok!(Swapping::deregister_as_broker(OriginTrait::signed(ALICE)),);

		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_broker(
			OriginTrait::signed(ALICE),
		)
		.expect_err("ALICE should be deregistered.");
	});
}
