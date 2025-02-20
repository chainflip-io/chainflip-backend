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
fn test_network_fee_calculation() {
	const MIN_FEE: u128 = 0;

	// Show we can never overflow and panic
	utilities::calculate_network_fee(Permill::from_percent(100), MIN_FEE, AssetAmount::MAX);
	// 200 bps (2%) of 1000 = 20
	assert_eq!(
		utilities::calculate_network_fee(Permill::from_percent(2u32), MIN_FEE, 1000),
		(980, 20)
	);
	// 2220 bps = 22 % of 199 = 43,78
	assert_eq!(
		utilities::calculate_network_fee(Permill::from_rational(2220u32, 10000u32), MIN_FEE, 199),
		(155, 44)
	);
	// 2220 bps = 22 % of 234 = 51,26
	assert_eq!(
		utilities::calculate_network_fee(Permill::from_rational(2220u32, 10000u32), MIN_FEE, 233),
		(181, 52)
	);
	// 10 bps = 0,1% of 30000 = 30
	assert_eq!(
		utilities::calculate_network_fee(Permill::from_rational(1u32, 1000u32), MIN_FEE, 30000),
		(29970, 30)
	);

	{
		const MIN_FEE: u128 = 10;
		// Minimum fee is enforced (0.1% of 3000 should be 3, but we epxect to charge 10)
		assert_eq!(
			utilities::calculate_network_fee(Permill::from_perthousand(1), MIN_FEE, 3000),
			(2990, 10)
		);
		// If input is (somehow) smaller than min network fee the behaviour is still reasonable
		// (all of itput is consumed as network fee):
		assert_eq!(
			utilities::calculate_network_fee(Permill::from_perthousand(1), MIN_FEE, 5),
			(0, 5)
		);
	}
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

#[test]
fn network_fee_swap_gets_burnt() {
	const INPUT_ASSET: Asset = Asset::Usdc;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const AMOUNT: AssetAmount = 100;

	new_test_ext()
		.execute_with(|| {
			Swapping::init_swap_request(
				INPUT_ASSET,
				AMOUNT,
				OUTPUT_ASSET,
				SwapRequestType::NetworkFee,
				Default::default(),
				None,
				None,
				SwapOrigin::Internal,
			);

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

	NetworkFee::set(NETWORK_FEE);

	new_test_ext()
		.execute_with(|| {
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
	NetworkFee::set(Permill::from_percent(NETWORK_FEE_PERCENT));

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
			}));

			assert_eq!(
				get_broker_balance::<Test>(&ALICE, Asset::Usdc),
				ALICE_FEE_1 + ALICE_FEE_2 + ALICE_FEE_3
			);
			assert_eq!(get_broker_balance::<Test>(&BOB, Asset::Usdc), BOB_FEE_1);
		});
}

#[test]
fn min_network_fee_is_enforced_in_regular_swaps() {
	const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const FEE_SWAP_BLOCK: u64 = SWAP_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const NETWORK_FEE: Permill = Permill::from_perthousand(1);
	const INPUT_AMOUNT: u128 = 1000;

	const MIN_NETWORK_FEE: u128 = 10;

	const FROM: Asset = Asset::Usdc;
	const TO: Asset = Asset::ArbEth;

	// Make sure that the amount is small enough that the min network fee
	// will be in effect:
	assert!(MIN_NETWORK_FEE > NETWORK_FEE * INPUT_AMOUNT);

	new_test_ext()
		.execute_with(|| {
			NetworkFee::set(NETWORK_FEE);
			MinimumNetworkFeePerChunk::<Test>::set(MIN_NETWORK_FEE);

			// Check that min fee applies to regular swaps:
			Swapping::init_swap_request(
				FROM,
				INPUT_AMOUNT,
				TO,
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

			assert_eq!(CollectedNetworkFee::<Test>::get(), 0);
		})
		.then_process_blocks_until_block(SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(CollectedNetworkFee::<Test>::get(), MIN_NETWORK_FEE);

			// Check that min fee does NOT apply to fee swaps
			Swapping::init_swap_request(
				FROM,
				INPUT_AMOUNT,
				TO,
				SwapRequestType::IngressEgressFee,
				Default::default(),
				None,
				None,
				SwapOrigin::Vault {
					tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(BROKER),
				},
			);
		})
		.then_process_blocks_until_block(FEE_SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(
				CollectedNetworkFee::<Test>::get(),
				MIN_NETWORK_FEE + NETWORK_FEE * INPUT_AMOUNT
			);
		});
}
