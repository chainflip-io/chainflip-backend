mod dca;
mod fill_or_kill;

use std::sync::LazyLock;

use super::*;
use crate::{
	mock::{RuntimeEvent, *},
	CcmFailReason, CollectedRejectedFunds, EarnedBrokerFees, Error, Event, MaximumSwapAmount,
	Pallet, Swap, SwapOrigin, SwapQueue, SwapType,
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
use itertools::Itertools;
use sp_arithmetic::Permill;
use sp_core::{H160, U256};
use sp_std::iter;

const GAS_BUDGET: AssetAmount = 1_000u128;
const SWAP_REQUEST_ID: u64 = 1;
const INIT_BLOCK: u64 = 1;
const BROKER_FEE_BPS: u16 = 2;

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
	refund_params: Option<ChannelRefundParameters<ForeignChainAddress>>,
	dca_params: Option<DcaParameters>,
	output_address: ForeignChainAddress,
	is_ccm: bool,
}

// Convenience struct used in tests allowing to specify refund parameters
// with min output rather than min price:
struct TestRefundParams {
	retry_duration: u32,
	min_output: AssetAmount,
}

impl TestRefundParams {
	fn into_channel_params(
		self,
		input_amount: AssetAmount,
	) -> ChannelRefundParameters<ForeignChainAddress> {
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
		}) if reason_in_event == &reason && address_in_event == &MockAddressConverter::to_encoded_address(destination_address) && metadata_in_event == &ccm,
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

#[track_caller]
fn assert_swaps_queue_is_empty() {
	assert_eq!(SwapQueue::<Test>::iter_keys().count(), 0);
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
			.map(|swap| MockEgressParameter::<AnyChain>::Swap {
				asset: swap.output_asset,
				amount: if swap.input_asset == Asset::Usdc || swap.output_asset == Asset::Usdc {
					swap.input_amount * DEFAULT_SWAP_RATE
				} else {
					swap.input_amount * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE
				},
				destination_address: swap.output_address.clone(),
				fee: 0,
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
fn expect_earned_fees_to_be_recorded() {
	new_test_ext().execute_with(|| {
		const ALICE: u64 = 2_u64;
		const BOB: u64 = 3_u64;

		swap_with_custom_broker_fee(
			Asset::Flip,
			Asset::Usdc,
			100,
			bounded_vec![Beneficiary { account: ALICE, bps: 200 }],
		);

		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 2);

		swap_with_custom_broker_fee(
			Asset::Flip,
			Asset::Usdc,
			100,
			bounded_vec![Beneficiary { account: ALICE, bps: 200 }],
		);

		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 4);

		swap_with_custom_broker_fee(
			Asset::Eth,
			Asset::Usdc,
			100,
			bounded_vec![
				Beneficiary { account: ALICE, bps: 200 },
				Beneficiary { account: BOB, bps: 200 }
			],
		);
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Eth), 2);
		assert_eq!(EarnedBrokerFees::<Test>::get(BOB, cf_primitives::Asset::Eth), 2);
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
	new_test_ext()
		.execute_with(|| {
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

			const AMOUNT: AssetAmount = 500;
			// 2. Schedule the swap -> SwapScheduled

			swap_with_custom_broker_fee(Asset::Eth, Asset::Usdc, AMOUNT, bounded_vec![]);
			// 3. Process swaps -> SwapExecuted, SwapEgressScheduled
			Swapping::on_finalize(1);
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
		.then_process_blocks_until(|_| System::block_number() == 3)
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
fn withdraw_broker_fees() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Swapping::withdraw(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			<Error<Test>>::NoFundsAvailable
		);
		EarnedBrokerFees::<Test>::insert(ALICE, Asset::Eth, 200);
		assert_ok!(Swapping::withdraw(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
		let mut egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
		assert!(egresses.len() == 1);
		assert_eq!(egresses.pop().expect("must exist").amount(), 200);
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::WithdrawalRequested {
			egress_id: (ForeignChain::Ethereum, 1),
			egress_asset: Asset::Eth,
			egress_amount: 200,
			destination_address: EncodedAddress::Eth(Default::default()),
			egress_fee: 0,
		}));
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

mod ccm {

	const GAS_ASSET: Asset = Asset::Eth;

	use super::*;

	#[track_caller]
	fn init_ccm_swap_request(input_asset: Asset, output_asset: Asset, input_amount: AssetAmount) {
		let ccm_deposit_metadata = generate_ccm_deposit();
		let output_address = (*EVM_OUTPUT_ADDRESS).clone();
		let encoded_output_address =
			MockAddressConverter::to_encoded_address(output_address.clone());
		let origin = SwapOrigin::Vault { tx_hash: Default::default() };
		assert_ok!(Swapping::init_swap_request(
			input_asset,
			input_amount,
			output_asset,
			SwapRequestType::Ccm {
				ccm_deposit_metadata: ccm_deposit_metadata.clone(),
				output_address
			},
			Default::default(),
			None,
			None,
			origin.clone(),
		));

		System::assert_has_event(RuntimeEvent::Swapping(Event::SwapRequested {
			swap_request_id: SWAP_REQUEST_ID,
			input_asset,
			output_asset,
			input_amount,
			broker_fee: 0,
			request_type: SwapRequestTypeEncoded::Ccm {
				ccm_deposit_metadata,
				output_address: encoded_output_address,
			},
			origin,
		}));
	}

	#[track_caller]
	pub(super) fn assert_ccm_egressed(
		asset: Asset,
		principal_amount: AssetAmount,
		gas_budget: AssetAmount,
	) {
		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::<Test>::SwapEgressScheduled {
				swap_request_id: SWAP_REQUEST_ID,
				..
			})
		);

		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset,
				amount: principal_amount,
				destination_address: (*EVM_OUTPUT_ADDRESS).clone(),
				message: vec![0x01].try_into().unwrap(),
				cf_parameters: vec![].try_into().unwrap(),
				gas_budget,
			},]
		);
	}

	#[test]
	fn can_process_ccms_via_swap_deposit_address() {
		const PRINCIPAL_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const GAS_SWAP_BLOCK: u64 = PRINCIPAL_SWAP_BLOCK + SWAP_DELAY_BLOCKS as u64;

		const DEPOSIT_AMOUNT: AssetAmount = 10_000;

		new_test_ext()
			.execute_with(|| {
				let request_ccm = generate_ccm_channel();
				let ccm = generate_ccm_deposit();

				// Can process CCM via Swap deposit
				assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
					RuntimeOrigin::signed(ALICE),
					Asset::Dot,
					Asset::Eth,
					MockAddressConverter::to_encoded_address((*EVM_OUTPUT_ADDRESS).clone()),
					0,
					Some(request_ccm),
					0,
					Default::default(),
					None,
					None,
				));

				assert_ok!(Swapping::init_swap_request(
					Asset::Dot,
					DEPOSIT_AMOUNT,
					Asset::Eth,
					SwapRequestType::Ccm {
						ccm_deposit_metadata: ccm.clone(),
						output_address: (*EVM_OUTPUT_ADDRESS).clone()
					},
					Default::default(),
					None,
					None,
					SwapOrigin::Vault { tx_hash: Default::default() },
				));

				// Principal swap is scheduled first
				assert_eq!(
					SwapQueue::<Test>::get(PRINCIPAL_SWAP_BLOCK),
					vec![Swap::new(
						1,
						1,
						Asset::Dot,
						Asset::Eth,
						DEPOSIT_AMOUNT - GAS_BUDGET,
						None,
						[FeeType::NetworkFee],
					),]
				);
			})
			.then_execute_at_block(PRINCIPAL_SWAP_BLOCK, |_| {})
			.then_execute_with(|_| {
				// Gas swap should only be scheduled after principal is executed
				assert_eq!(
					SwapQueue::<Test>::get(GAS_SWAP_BLOCK),
					vec![Swap::new(
						2,
						1,
						Asset::Dot,
						Asset::Eth,
						GAS_BUDGET,
						None,
						[FeeType::NetworkFee],
					),]
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				);
			})
			.then_execute_at_block(GAS_SWAP_BLOCK, |_| {})
			.then_execute_with(|_| {
				// CCM is scheduled for egress
				assert_ccm_egressed(
					Asset::Eth,
					(DEPOSIT_AMOUNT - GAS_BUDGET) * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
					GAS_BUDGET * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 2, .. }),
				);
			});
	}

	#[test]
	fn ccm_no_swap() {
		const PRINCIPAL_AMOUNT: AssetAmount = 10_000;
		const SWAP_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT + GAS_BUDGET;

		// Both input and output assets are Eth, so no swap is needed:
		const INPUT_ASSET: Asset = Asset::Eth;
		const OUTPUT_ASSET: Asset = Asset::Eth;
		new_test_ext().execute_with(|| {
			init_ccm_swap_request(INPUT_ASSET, OUTPUT_ASSET, SWAP_AMOUNT);

			// No need to store the request in this case:
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			// CCM should be immediately egressed:
			assert_ccm_egressed(OUTPUT_ASSET, PRINCIPAL_AMOUNT, GAS_BUDGET);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID,
					..
				}),
			);

			assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
			assert_eq!(CollectedRejectedFunds::<Test>::get(OUTPUT_ASSET), 0);
		});
	}

	#[test]
	fn ccm_principal_swap_only() {
		const PRINCIPAL_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const PRINCIPAL_AMOUNT: AssetAmount = 10_000;
		const SWAP_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT + GAS_BUDGET;

		// Gas asset is Eth, so no gas swap is necessary
		const INPUT_ASSET: Asset = Asset::Eth;
		const OUTPUT_ASSET: Asset = Asset::Flip;
		new_test_ext()
			.execute_with(|| {
				init_ccm_swap_request(INPUT_ASSET, OUTPUT_ASSET, SWAP_AMOUNT);

				assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

				// Principal swap should be immediately scheduled
				assert_eq!(
					SwapQueue::<Test>::get(PRINCIPAL_SWAP_BLOCK),
					vec![Swap::new(
						1,
						SWAP_REQUEST_ID,
						INPUT_ASSET,
						OUTPUT_ASSET,
						PRINCIPAL_AMOUNT,
						None,
						[FeeType::NetworkFee],
					),]
				);
			})
			.then_execute_at_block(PRINCIPAL_SWAP_BLOCK, |_| {})
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID,
						..
					}),
				);

				assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

				assert_ccm_egressed(
					OUTPUT_ASSET,
					PRINCIPAL_AMOUNT * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
					GAS_BUDGET,
				);

				assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
				assert_eq!(CollectedRejectedFunds::<Test>::get(OUTPUT_ASSET), 0);
			});
	}

	#[test]
	fn ccm_gas_swap_only() {
		const GAS_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

		const INPUT_ASSET: Asset = Asset::Flip;
		const OUTPUT_ASSET: Asset = Asset::Usdc;
		new_test_ext()
			.execute_with(|| {
				// Ccm with principal asset = 0
				init_ccm_swap_request(INPUT_ASSET, OUTPUT_ASSET, GAS_BUDGET);

				assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

				// Gas swap should be immediately scheduled
				assert_eq!(
					SwapQueue::<Test>::get(GAS_SWAP_BLOCK),
					vec![Swap::new(
						1,
						SWAP_REQUEST_ID,
						INPUT_ASSET,
						GAS_ASSET,
						GAS_BUDGET,
						None,
						[FeeType::NetworkFee]
					),]
				);
			})
			.then_execute_at_block(GAS_SWAP_BLOCK, |_| {})
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID,
						..
					}),
				);

				assert_ccm_egressed(
					OUTPUT_ASSET,
					0,
					GAS_BUDGET * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
				);

				assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

				assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
				assert_eq!(CollectedRejectedFunds::<Test>::get(OUTPUT_ASSET), 0);
			});
	}

	#[test]
	fn can_process_ccms_via_extrinsic() {
		const PRINCIPAL_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const GAS_SWAP_BLOCK: u64 = PRINCIPAL_SWAP_BLOCK + SWAP_DELAY_BLOCKS as u64;

		const INPUT_ASSET: Asset = Asset::Btc;
		const OUTPUT_ASSET: Asset = Asset::Usdc;
		const PRINCIPAL_AMOUNT: AssetAmount = 10_000;

		new_test_ext()
			.execute_with(|| {
				let ccm = generate_ccm_deposit();

				// Can process CCM directly via Pallet Extrinsic.
				assert_ok!(Swapping::ccm_deposit(
					RuntimeOrigin::root(),
					INPUT_ASSET,
					PRINCIPAL_AMOUNT + GAS_BUDGET,
					OUTPUT_ASSET,
					MockAddressConverter::to_encoded_address((*EVM_OUTPUT_ADDRESS).clone()),
					ccm.clone(),
					Default::default(),
				));

				assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

				assert_eq!(
					SwapQueue::<Test>::get(PRINCIPAL_SWAP_BLOCK),
					vec![Swap::new(
						1,
						SWAP_REQUEST_ID,
						INPUT_ASSET,
						OUTPUT_ASSET,
						PRINCIPAL_AMOUNT,
						None,
						[FeeType::NetworkFee]
					),]
				);
			})
			.then_execute_at_block(PRINCIPAL_SWAP_BLOCK, |_| {})
			.then_execute_with(|_| {
				assert_eq!(
					SwapQueue::<Test>::get(GAS_SWAP_BLOCK),
					vec![Swap::new(
						2,
						SWAP_REQUEST_ID,
						INPUT_ASSET,
						GAS_ASSET,
						GAS_BUDGET,
						None,
						[FeeType::NetworkFee]
					),]
				);
			})
			.then_execute_at_block(GAS_SWAP_BLOCK, |_| {})
			.then_execute_with(|_| {
				assert_ccm_egressed(
					OUTPUT_ASSET,
					PRINCIPAL_AMOUNT * DEFAULT_SWAP_RATE,
					GAS_BUDGET * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
				);
				assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			});
	}
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
			broker_fee: 0,
			request_type: SwapRequestTypeEncoded::Regular {
				output_address: encoded_output_address,
			},
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
		.then_execute_at_block(SWAP_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			// Confiscated fund is unchanged
			assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
		});
}

#[test]
fn process_all_into_stable_swaps_first() {
	new_test_ext().execute_with(|| {
		let amount = 1_000_000;
		let encoded_address = EncodedAddress::Eth(Default::default());

		NetworkFee::set(Permill::from_parts(100));

		[Asset::Flip, Asset::Btc, Asset::Dot, Asset::Usdc]
			.into_iter()
			.for_each(|asset| {
				assert_ok!(Swapping::schedule_swap_from_contract(
					RuntimeOrigin::root(),
					asset,
					Asset::Eth,
					amount,
					encoded_address.clone(),
					Default::default(),
				));
			});

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap::new(1, 1, Asset::Flip, Asset::Eth, amount, None, [FeeType::NetworkFee]),
				Swap::new(2, 2, Asset::Btc, Asset::Eth, amount, None, [FeeType::NetworkFee]),
				Swap::new(3, 3, Asset::Dot, Asset::Eth, amount, None, [FeeType::NetworkFee]),
				Swap::new(4, 4, Asset::Usdc, Asset::Eth, amount, None, [FeeType::NetworkFee]),
			]
		);

		System::reset_events();
		// All swaps in the SwapQueue are executed.
		Swapping::on_finalize(execute_at);
		assert_swaps_queue_is_empty();

		let usdc_amount_swapped_after_fee =
			Swapping::take_network_fee(amount * DEFAULT_SWAP_RATE).remaining_amount;
		let usdc_amount_deposited_after_fee = Swapping::take_network_fee(amount).remaining_amount;

		// Verify swap "from" -> STABLE_ASSET, then "to" -> Output Asset
		assert_eq!(
			Swaps::get(),
			vec![
				(Asset::Flip, Asset::Usdc, amount),
				(Asset::Dot, Asset::Usdc, amount),
				(Asset::Btc, Asset::Usdc, amount),
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

#[test]
fn cannot_swap_in_safe_mode() {
	new_test_ext().execute_with(|| {
		let swaps_scheduled_at = System::block_number() + SWAP_DELAY_BLOCKS as u64;

		insert_swaps(&generate_test_swaps());

		assert_eq!(SwapQueue::<Test>::decode_len(swaps_scheduled_at), Some(4));

		// Activate code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();

		// No swap is done
		Swapping::on_finalize(swaps_scheduled_at);

		let retry_at_block = swaps_scheduled_at + SwapRetryDelay::<Test>::get();
		assert_eq!(SwapQueue::<Test>::decode_len(retry_at_block), Some(4));

		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();

		// Swaps are processed
		Swapping::on_finalize(retry_at_block);
		assert_eq!(SwapQueue::<Test>::decode_len(retry_at_block), None);
	});
}

#[test]
fn cannot_withdraw_in_safe_mode() {
	new_test_ext().execute_with(|| {
		EarnedBrokerFees::<Test>::insert(ALICE, Asset::Eth, 200);

		// Activate code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();

		// Cannot withdraw
		assert_noop!(
			Swapping::withdraw(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			Error::<Test>::WithdrawalsDisabled
		);
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, Asset::Eth), 200);

		// Change back to code green
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();

		// withdraws are now alloed
		assert_ok!(Swapping::withdraw(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, Asset::Eth), 0);
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
		.then_execute_at_block(PRINCIPAL_SWAP_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Swap outputs are zero
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_request_id: 1,
					swap_id: 1,
					network_fee: 0,
					input_amount: PRINCIPAL_AMOUNT,
					input_asset: Asset::Usdc,
					output_asset: Asset::Eth,
					output_amount: ZERO_AMOUNT,
					intermediate_amount: None,
				}),
			);
		})
		.then_execute_at_block(GAS_SWAP_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_request_id: 1,
					swap_id: 2,
					network_fee: 0,
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
		.then_execute_at_next_block(|_| {
			swap_with_custom_broker_fee(Asset::Usdc, Asset::Eth, 100, bounded_vec![]);
			swap_with_custom_broker_fee(Asset::Usdc, Asset::Eth, 1, bounded_vec![]);

			// Change the swap rate so swap output will be 0
			SwapRate::set(0.01f64);
			System::reset_events();
		})
		.then_process_blocks_until(|_| System::block_number() == 4)
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
fn can_set_maximum_swap_amount() {
	new_test_ext().execute_with(|| {
		let asset = Asset::Eth;
		let amount = Some(1_000u128);
		assert!(MaximumSwapAmount::<Test>::get(asset).is_none());

		// Set the new maximum swap_amount
		set_maximum_swap_amount(asset, amount);

		assert_eq!(MaximumSwapAmount::<Test>::get(asset), amount);
		assert_eq!(Swapping::maximum_swap_amount(asset), amount);

		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::MaximumSwapAmountSet {
			asset,
			amount,
		}));

		// Can remove maximum swap amount
		set_maximum_swap_amount(asset, None);
		assert!(MaximumSwapAmount::<Test>::get(asset).is_none());
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::MaximumSwapAmountSet {
			asset,
			amount: None,
		}));
	});
}

#[test]
fn swap_excess_are_confiscated_ccm_via_deposit() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let principal_amount = 10_000;
		let max_swap = 2_000;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;
		let request_ccm = generate_ccm_channel();
		let ccm = generate_ccm_deposit();

		let input_amount = principal_amount + gas_budget;

		set_maximum_swap_amount(from, Some(max_swap));

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
			input_amount,
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
			total_amount: input_amount,
			confiscated_amount: input_amount - max_swap,
		}));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(1, 1, from, to, max_swap - gas_budget, None, [FeeType::NetworkFee]),]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), input_amount - max_swap);
	});
}

#[test]
fn swap_excess_are_confiscated_ccm_via_extrinsic() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let principal_amount = 1_000;
		let max_swap = GAS_BUDGET + 100;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;
		let ccm = generate_ccm_deposit();

		let input_amount = principal_amount + gas_budget;

		set_maximum_swap_amount(from, Some(max_swap));

		// Register CCM via Swap deposit
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			input_amount,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm,
			Default::default(),
		));

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_request_id: 1,
			asset: from,
			total_amount: input_amount,
			confiscated_amount: input_amount - max_swap,
		}));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(1, 1, from, to, max_swap - gas_budget, None, [FeeType::NetworkFee]),]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), input_amount - max_swap);
	});
}

#[test]
fn swap_excess_are_confiscated_for_swap_via_extrinsic() {
	new_test_ext().execute_with(|| {
		let max_swap = 100;
		let amount = 1_000;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		set_maximum_swap_amount(from, Some(max_swap));

		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_request_id: 1,
			asset: from,
			total_amount: 1_000,
			confiscated_amount: 900,
		}));

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap::new(1, 1, from, to, max_swap, None, [FeeType::NetworkFee])]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900);
	});
}

#[test]
fn swap_excess_are_confiscated_for_swap_via_deposit() {
	new_test_ext().execute_with(|| {
		let max_swap = 100;
		let amount = 1_000;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		set_maximum_swap_amount(from, Some(max_swap));

		swap_with_custom_broker_fee(from, to, amount, bounded_vec![]);

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_request_id: 1,
			asset: from,
			total_amount: 1_000,
			confiscated_amount: 900,
		}));

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap::new(1, 1, from, to, max_swap, None, [FeeType::NetworkFee])]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900);
	});
}

#[test]
fn max_swap_amount_can_be_removed() {
	new_test_ext().execute_with(|| {
		let max_swap = 100;
		let amount = 1_000;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		// Initial max swap amount is set.
		set_maximum_swap_amount(from, Some(max_swap));
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900u128);

		// Reset event and confiscated funds.
		CollectedRejectedFunds::<Test>::set(from, 0u128);
		System::reset_events();

		// Max is removed.
		set_maximum_swap_amount(from, None);

		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap::new(1, 1, from, to, max_swap, None, [FeeType::NetworkFee]),
				// New swap takes the full amount.
				Swap::new(2, 2, from, to, amount, None, [FeeType::NetworkFee]),
			]
		);
		// No no funds are confiscated.
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn input_amount_excludes_network_fee() {
	const AMOUNT: AssetAmount = 1_000;
	const FROM_ASSET: Asset = Asset::Usdc;
	const TO_ASSET: Asset = Asset::Flip;
	let output_address: ForeignChainAddress = ForeignChainAddress::Eth(Default::default());
	const NETWORK_FEE: Permill = Permill::from_percent(1);

	NetworkFee::set(NETWORK_FEE);

	new_test_ext()
		.execute_with(|| {
			swap_with_custom_broker_fee(FROM_ASSET, TO_ASSET, AMOUNT, bounded_vec![]);

			assert_ok!(<Pallet<Test> as SwapRequestHandler>::init_swap_request(
				FROM_ASSET,
				AMOUNT,
				TO_ASSET,
				SwapRequestType::Regular { output_address: output_address.clone() },
				bounded_vec![],
				None,
				None,
				SwapOrigin::Vault { tx_hash: Default::default() },
			));
		})
		.then_process_blocks_until(|_| System::block_number() == 3)
		.then_execute_with(|_| {
			let network_fee = NETWORK_FEE * AMOUNT;
			let expected_input_amount = AMOUNT - network_fee;

			System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
				swap_request_id: 1,
				swap_id: 1,
				input_asset: FROM_ASSET,
				output_asset: TO_ASSET,
				network_fee,
				input_amount: expected_input_amount,
				output_amount: expected_input_amount * DEFAULT_SWAP_RATE,
				intermediate_amount: None,
			}));
		});

	NetworkFee::set(Default::default());
}

#[test]
fn can_swap_below_max_amount() {
	new_test_ext().execute_with(|| {
		let max_swap = 1_001u128;
		let amount = 1_000u128;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		// Initial max swap amount is set.
		set_maximum_swap_amount(from, Some(max_swap));
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0u128);

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap::new(1, 1, from, to, amount, None, [FeeType::NetworkFee]),]
		);
	});
}

#[test]
fn can_swap_ccm_below_max_amount() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let principal_amount = 999;
		let max_swap = gas_budget + principal_amount;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;
		let ccm = generate_ccm_deposit();

		set_maximum_swap_amount(from, Some(max_swap));

		// Register CCM via Swap deposit
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			gas_budget + principal_amount,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm,
			Default::default(),
		));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(1, 1, from, to, principal_amount, None, [FeeType::NetworkFee]),]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

fn swap_with_custom_broker_fee(
	from: Asset,
	to: Asset,
	amount: AssetAmount,
	broker_fees: Beneficiaries<u64>,
) {
	assert_ok!(<Pallet<Test> as SwapRequestHandler>::init_swap_request(
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
fn swap_broker_fee_calculated_correctly() {
	new_test_ext().execute_with(|| {
		let fees: [BasisPoints; 12] =
			[1, 5, 10, 100, 200, 500, 1000, 1500, 2000, 5000, 7500, 10000];
		const AMOUNT: AssetAmount = 100000;

		// calculate broker fees for each asset available
		Asset::all().for_each(|asset| {
			let total_fees: u128 =
				fees.iter().fold(0, |total_fees: u128, fee_bps: &BasisPoints| {
					swap_with_custom_broker_fee(
						asset,
						Asset::Usdc,
						AMOUNT,
						bounded_vec![Beneficiary { account: ALICE, bps: *fee_bps }],
					);
					total_fees +
						Permill::from_parts(*fee_bps as u32 * BASIS_POINTS_PER_MILLION) * AMOUNT
				});
			assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, asset), total_fees);
		});
	});
}

#[test]
fn swap_broker_fee_cannot_exceed_amount() {
	new_test_ext().execute_with(|| {
		swap_with_custom_broker_fee(
			Asset::Usdc,
			Asset::Flip,
			100,
			bounded_vec![Beneficiary { account: ALICE, bps: 15000 }],
		);
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Usdc), 100);
	});
}

#[test]
fn swap_broker_fee_subtracted_from_swap_amount() {
	new_test_ext().execute_with(|| {
		let amounts: [AssetAmount; 6] = [50, 100, 200, 500, 1000, 10000];
		let fees: [BasisPoints; 4] = [100, 1000, 5000, 10000];

		const OUTPUT_ASSET: Asset = Asset::Flip;

		let combinations = amounts.iter().cartesian_product(fees);

		let execute_at = System::block_number() + SWAP_DELAY_BLOCKS as u64;

		let mut swap_request_id = 1;
		Asset::all().for_each(|asset| {
			let mut total_fees = 0;
			combinations.clone().for_each(|(amount, broker_fee)| {
				swap_with_custom_broker_fee(
					asset,
					OUTPUT_ASSET,
					*amount,
					bounded_vec![Beneficiary { account: ALICE, bps: broker_fee }],
				);
				let broker_fee =
					Permill::from_parts(broker_fee as u32 * BASIS_POINTS_PER_MILLION) * *amount;
				total_fees += broker_fee;
				assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, asset), total_fees);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequested {
						swap_request_id: swap_request_id_in_event,
						input_asset,
						input_amount,
						output_asset: OUTPUT_ASSET,
						broker_fee: broker_fee_in_event,
						..
					}) if swap_request_id_in_event == &swap_request_id &&
						  input_amount == amount &&
						  input_asset == &asset &&
						  broker_fee_in_event == &broker_fee
				);

				System::assert_has_event(RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id,
					swap_id: swap_request_id,
					input_amount: amount - broker_fee,
					swap_type: SwapType::Swap,
					execute_at,
				}));

				swap_request_id += 1;
			})
		});
	});
}

#[test]
fn broker_bps_is_limited() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Swapping::request_swap_deposit_address_with_affiliates(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				Asset::Usdc,
				EncodedAddress::Eth(Default::default()),
				1001,
				None,
				0,
				Default::default(),
				None,
				None,
			),
			Error::<Test>::BrokerCommissionBpsTooHigh
		);
	});
}

#[test]
fn swaps_are_executed_according_to_execute_at_field() {
	let mut swaps = generate_test_swaps();
	let later_swaps = swaps.split_off(2);

	new_test_ext()
		.execute_with(|| {
			// Block 1, swaps should be scheduled at block 3
			assert_eq!(System::block_number(), 1);
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

	const EXECUTE_AT_BLOCK: u64 = 3;
	const RETRY_AT_BLOCK: u64 = EXECUTE_AT_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS;

	new_test_ext()
		.execute_with(|| {
			// Block 1, swaps should be scheduled at block 3
			assert_eq!(System::block_number(), 1);
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
		.then_execute_at_block(RETRY_AT_BLOCK, |_| {})
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

		let swaps: Vec<_> = [
			(1, Asset::Flip, Asset::Usdc),
			(2, Asset::Usdc, Asset::Flip),
			(3, Asset::Btc, Asset::Eth),
			(4, Asset::Flip, Asset::Btc),
			(5, Asset::Eth, Asset::Flip),
		]
		.into_iter()
		.map(|(id, from, to)| Swap::new(id, id, from, to, INIT_AMOUNT, None, [FeeType::NetworkFee]))
		.collect();

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
					base_asset: Asset::Flip,
					quote_asset: Asset::Usdc,
					side: Side::Sell,
					amount: INIT_AMOUNT,
					source_asset: None,
					source_amount: None,
				},
				SwapLegInfo {
					swap_id: 2,
					base_asset: Asset::Flip,
					quote_asset: Asset::Usdc,
					side: Side::Buy,
					amount: INIT_AMOUNT,
					source_asset: None,
					source_amount: None,
				},
				SwapLegInfo {
					swap_id: 4,
					base_asset: Asset::Flip,
					quote_asset: Asset::Usdc,
					side: Side::Sell,
					amount: INIT_AMOUNT,
					source_asset: None,
					source_amount: None,
				},
				SwapLegInfo {
					swap_id: 5,
					base_asset: Asset::Flip,
					quote_asset: Asset::Usdc,
					side: Side::Buy,
					amount: INTERMEDIATE_AMOUNT,
					source_asset: Some(Asset::Eth),
					source_amount: Some(INIT_AMOUNT),
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

		let swaps: Vec<_> = [(1, Asset::Flip, Asset::Eth), (2, Asset::Eth, Asset::Usdc)]
			.into_iter()
			.map(|(id, from, to)| {
				Swap::new(id, id, from, to, INIT_AMOUNT, None, [FeeType::NetworkFee])
			})
			.collect();

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
					base_asset: Asset::Eth,
					quote_asset: Asset::Usdc,
					side: Side::Buy,
					amount: INIT_AMOUNT * PRICE,
					source_asset: Some(Asset::Flip),
					source_amount: Some(INIT_AMOUNT),
				},
				SwapLegInfo {
					swap_id: 2,
					base_asset: Asset::Eth,
					quote_asset: Asset::Usdc,
					side: Side::Sell,
					amount: INIT_AMOUNT,
					source_asset: None,
					source_amount: None,
				}
			]
		);
	});
}

#[test]
fn can_update_all_config_items() {
	new_test_ext().execute_with(|| {
		const NEW_MAX_SWAP_AMOUNT_BTC: Option<AssetAmount> = Some(100);
		const NEW_MAX_SWAP_AMOUNT_DOT: Option<AssetAmount> = Some(69);
		let new_swap_retry_delay = BlockNumberFor::<Test>::from(1234u32);
		let new_flip_buy_interval = BlockNumberFor::<Test>::from(5678u32);

		// Check that the default values are different from the new ones
		assert!(MaximumSwapAmount::<Test>::get(Asset::Btc).is_none());
		assert!(MaximumSwapAmount::<Test>::get(Asset::Dot).is_none());
		assert_ne!(SwapRetryDelay::<Test>::get(), new_swap_retry_delay);
		assert_ne!(FlipBuyInterval::<Test>::get(), new_flip_buy_interval);

		// Update all config items at the same time, and updates 2 separate max swap amounts.
		assert_ok!(Swapping::update_pallet_config(
			OriginTrait::root(),
			vec![
				PalletConfigUpdate::MaximumSwapAmount {
					asset: Asset::Btc,
					amount: NEW_MAX_SWAP_AMOUNT_BTC
				},
				PalletConfigUpdate::MaximumSwapAmount {
					asset: Asset::Dot,
					amount: NEW_MAX_SWAP_AMOUNT_DOT
				},
				PalletConfigUpdate::SwapRetryDelay { delay: new_swap_retry_delay },
				PalletConfigUpdate::FlipBuyInterval { interval: new_flip_buy_interval },
			]
			.try_into()
			.unwrap()
		));

		// Check that the new values were set
		assert_eq!(MaximumSwapAmount::<Test>::get(Asset::Btc), NEW_MAX_SWAP_AMOUNT_BTC);
		assert_eq!(MaximumSwapAmount::<Test>::get(Asset::Dot), NEW_MAX_SWAP_AMOUNT_DOT);
		assert_eq!(SwapRetryDelay::<Test>::get(), new_swap_retry_delay);
		assert_eq!(FlipBuyInterval::<Test>::get(), new_flip_buy_interval);

		// Check that the events were emitted
		assert_events_eq!(
			Test,
			RuntimeEvent::Swapping(crate::Event::MaximumSwapAmountSet {
				asset: Asset::Btc,
				amount: NEW_MAX_SWAP_AMOUNT_BTC,
			}),
			RuntimeEvent::Swapping(crate::Event::MaximumSwapAmountSet {
				asset: Asset::Dot,
				amount: NEW_MAX_SWAP_AMOUNT_DOT,
			}),
			RuntimeEvent::Swapping(crate::Event::SwapRetryDelaySet {
				swap_retry_delay: new_swap_retry_delay
			}),
			RuntimeEvent::Swapping(crate::Event::BuyIntervalSet {
				buy_interval: new_flip_buy_interval
			})
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
			assert_ok!(Swapping::init_swap_request(
				INPUT_ASSET,
				AMOUNT,
				OUTPUT_ASSET,
				SwapRequestType::NetworkFee,
				Default::default(),
				None,
				None,
				SwapOrigin::Internal
			));

			assert_eq!(FlipToBurn::<Test>::get(), 0);

			assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

			System::assert_has_event(RuntimeEvent::Swapping(Event::SwapRequested {
				swap_request_id: SWAP_REQUEST_ID,
				input_asset: INPUT_ASSET,
				input_amount: AMOUNT,
				output_asset: OUTPUT_ASSET,
				broker_fee: 0,
				request_type: SwapRequestTypeEncoded::NetworkFee,
				origin: SwapOrigin::Internal,
			}));
			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapScheduled { .. }),);
		})
		.then_execute_at_block(SWAP_BLOCK, |_| {})
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
			assert_ok!(Swapping::init_swap_request(
				INPUT_ASSET,
				AMOUNT,
				OUTPUT_ASSET,
				SwapRequestType::IngressEgressFee,
				Default::default(),
				None,
				None,
				SwapOrigin::Internal
			));

			System::assert_has_event(RuntimeEvent::Swapping(Event::SwapRequested {
				swap_request_id: SWAP_REQUEST_ID,
				input_asset: INPUT_ASSET,
				input_amount: AMOUNT,
				output_asset: OUTPUT_ASSET,
				broker_fee: 0,
				request_type: SwapRequestTypeEncoded::IngressEgressFee,
				origin: SwapOrigin::Internal,
			}));

			assert_has_matching_event!(Test, RuntimeEvent::Swapping(Event::SwapScheduled { .. }),);

			assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

			assert_eq!(
				MockIngressEgressFeeHandler::<Ethereum>::withheld_assets(
					cf_chains::assets::eth::GAS_ASSET
				),
				0
			);
		})
		.then_execute_at_block(SWAP_BLOCK, |_| {})
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
fn register_and_deregister_account() {
	new_test_ext().execute_with(|| {
		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_broker(
			OriginTrait::signed(ALICE),
		)
		.expect("ALICE was registered in test setup.");

		// Earn some fees.
		EarnedBrokerFees::<Test>::insert(ALICE, Asset::Eth, 100);

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

		assert!(!EarnedBrokerFees::<Test>::contains_key(ALICE, Asset::Eth));
		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_broker(
			OriginTrait::signed(ALICE),
		)
		.expect_err("ALICE should be deregistered.");
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
				assert_ok!(Swapping::init_swap_request(
					from,
					INPUT_AMOUNT,
					to,
					SwapRequestType::Regular {
						output_address: ForeignChainAddress::Eth(H160::zero())
					},
					Default::default(),
					None,
					None,
					SwapOrigin::Vault { tx_hash: Default::default() }
				));

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
		let NetworkFeeTaken { remaining_amount, network_fee } =
			Swapping::take_network_fee(SWAP_AMOUNT);

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
			&Swap::new(1, 1, STABLE_ASSET, Asset::Flip, network_fee, None, [],)
		);
	});
}

#[test]
fn test_network_fee_calculation() {
	new_test_ext().execute_with(|| {
		// Show we can never overflow and panic
		utilities::calculate_network_fee(Permill::from_percent(100), AssetAmount::MAX);
		// 200 bps (2%) of 100 = 2
		assert_eq!(utilities::calculate_network_fee(Permill::from_percent(2u32), 100), (98, 2));
		// 2220 bps = 22 % of 199 = 43,78
		assert_eq!(
			utilities::calculate_network_fee(Permill::from_rational(2220u32, 10000u32), 199),
			(155, 44)
		);
		// 2220 bps = 22 % of 234 = 51,26
		assert_eq!(
			utilities::calculate_network_fee(Permill::from_rational(2220u32, 10000u32), 233),
			(181, 52)
		);
		// 10 bps = 0,1% of 3000 = 3
		assert_eq!(
			utilities::calculate_network_fee(Permill::from_rational(1u32, 1000u32), 3000),
			(2997, 3)
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
