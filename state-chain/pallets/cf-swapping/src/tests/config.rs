use super::*;

#[test]
fn can_update_all_config_items() {
	new_test_ext().execute_with(|| {
		const NEW_MAX_SWAP_AMOUNT_BTC: Option<AssetAmount> = Some(100);
		const NEW_MAX_SWAP_AMOUNT_DOT: Option<AssetAmount> = Some(69);
		let new_swap_retry_delay = BlockNumberFor::<Test>::from(1234u32);
		let new_flip_buy_interval = BlockNumberFor::<Test>::from(5678u32);
		const NEW_MAX_SWAP_RETRY_DURATION: u32 = 69_u32;
		const MAX_SWAP_REQUEST_DURATION: u32 = 420_u32;

		// Check that the default values are different from the new ones
		assert!(MaximumSwapAmount::<Test>::get(Asset::Btc).is_none());
		assert!(MaximumSwapAmount::<Test>::get(Asset::Dot).is_none());
		assert_ne!(SwapRetryDelay::<Test>::get(), new_swap_retry_delay);
		assert_ne!(FlipBuyInterval::<Test>::get(), new_flip_buy_interval);
		assert_ne!(MaxSwapRetryDurationBlocks::<Test>::get(), NEW_MAX_SWAP_RETRY_DURATION);
		assert_ne!(MaxSwapRequestDurationBlocks::<Test>::get(), MAX_SWAP_REQUEST_DURATION);

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
				PalletConfigUpdate::SetMaxSwapRetryDuration { blocks: NEW_MAX_SWAP_RETRY_DURATION },
				PalletConfigUpdate::SetMaxSwapRequestDuration { blocks: MAX_SWAP_REQUEST_DURATION },
			]
			.try_into()
			.unwrap()
		));

		// Check that the new values were set
		assert_eq!(MaximumSwapAmount::<Test>::get(Asset::Btc), NEW_MAX_SWAP_AMOUNT_BTC);
		assert_eq!(MaximumSwapAmount::<Test>::get(Asset::Dot), NEW_MAX_SWAP_AMOUNT_DOT);
		assert_eq!(SwapRetryDelay::<Test>::get(), new_swap_retry_delay);
		assert_eq!(FlipBuyInterval::<Test>::get(), new_flip_buy_interval);
		assert_eq!(MaxSwapRetryDurationBlocks::<Test>::get(), NEW_MAX_SWAP_RETRY_DURATION);
		assert_eq!(MaxSwapRequestDurationBlocks::<Test>::get(), MAX_SWAP_REQUEST_DURATION);

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
			}),
			RuntimeEvent::Swapping(crate::Event::MaxSwapRetryDurationSet {
				blocks: NEW_MAX_SWAP_RETRY_DURATION
			}),
			RuntimeEvent::Swapping(crate::Event::MaxSwapRequestDurationSet {
				blocks: MAX_SWAP_REQUEST_DURATION
			})
		);

		// Make sure that only governance can update the config
		assert_noop!(
			Swapping::update_pallet_config(OriginTrait::signed(ALICE), vec![].try_into().unwrap()),
			sp_runtime::traits::BadOrigin
		);
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

#[test]
fn swap_broker_fee_cannot_exceed_amount() {
	new_test_ext()
		.execute_with(|| {
			swap_with_custom_broker_fee(
				Asset::Usdc,
				Asset::Flip,
				100,
				bounded_vec![Beneficiary { account: ALICE, bps: 15000 }],
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
		.then_execute_with(|_| {
			// The broker gets nothing: setting fees >100% isn't actually possible due to
			// parameter validation, so how this is handled isn't really important as long as we
			// don't create money out of thin air and don't panic:
			assert_eq!(get_broker_balance::<Test>(&ALICE, cf_primitives::Asset::Usdc), 0);
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
		credit_broker_account::<Test>(&ALICE, Asset::Eth, 200);

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

		assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Eth), 200);

		// Change back to code green
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();

		// withdraws are now allowed
		assert_ok!(Swapping::withdraw(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
		assert_eq!(get_broker_balance::<Test>(&ALICE, Asset::Eth), 0);
	});
}

#[test]
fn cannot_register_as_broker_in_safe_mode() {
	pub const BROKER: <Test as frame_system::Config>::AccountId = 6969u64;

	new_test_ext().execute_with(|| {
		// Activate code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();

		// Cannot register as broker
		assert_noop!(
			Swapping::register_as_broker(RuntimeOrigin::signed(BROKER)),
			Error::<Test>::BrokerRegistrationDisabled
		);

		// Change back to code green
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();

		// Register as broker is now allowed
		assert_ok!(Swapping::register_as_broker(RuntimeOrigin::signed(BROKER)));
	});
}
