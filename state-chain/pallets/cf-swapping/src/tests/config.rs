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
fn can_update_all_config_items() {
	new_test_ext().execute_with(|| {
		const NEW_MAX_SWAP_AMOUNT_BTC: Option<AssetAmount> = Some(100);
		const NEW_MAX_SWAP_AMOUNT_DOT: Option<AssetAmount> = Some(69);
		let new_swap_retry_delay = BlockNumberFor::<Test>::from(1234u32);
		let new_flip_buy_interval = BlockNumberFor::<Test>::from(5678u32);
		const NEW_MAX_SWAP_RETRY_DURATION: u32 = 69_u32;
		const MAX_SWAP_REQUEST_DURATION: u32 = 420_u32;
		const NEW_MINIMUM_CHUNK_SIZE: AssetAmount = 10_000;
		const NEW_NETWORK_FEE: FeeRateAndMinimum =
			FeeRateAndMinimum { rate: Permill::from_percent(5), minimum: 10 };
		const NEW_INTERNAL_SWAP_FEE: FeeRateAndMinimum =
			FeeRateAndMinimum { rate: Permill::from_percent(10), minimum: 50 };
		const NEW_NETWORK_FEE_FOR_ASSET: Permill = Permill::from_percent(3);
		const NEW_INTERNAL_SWAP_NETWORK_FEE_FOR_ASSET: Permill = Permill::from_percent(7);

		// Check that the default values are different from the new ones
		assert!(MaximumSwapAmount::<Test>::get(Asset::Btc).is_none());
		assert!(MaximumSwapAmount::<Test>::get(Asset::Dot).is_none());
		assert_ne!(SwapRetryDelay::<Test>::get(), new_swap_retry_delay);
		assert_ne!(FlipBuyInterval::<Test>::get(), new_flip_buy_interval);
		assert_ne!(MaxSwapRetryDurationBlocks::<Test>::get(), NEW_MAX_SWAP_RETRY_DURATION);
		assert_ne!(MaxSwapRequestDurationBlocks::<Test>::get(), MAX_SWAP_REQUEST_DURATION);
		assert_ne!(MinimumChunkSize::<Test>::get(Asset::Eth), NEW_MINIMUM_CHUNK_SIZE);
		assert_ne!(NetworkFee::<Test>::get(), NEW_NETWORK_FEE);
		assert_ne!(InternalSwapNetworkFee::<Test>::get(), NEW_INTERNAL_SWAP_FEE);
		assert_ne!(NetworkFeeForAsset::<Test>::get(Asset::Usdc), Some(NEW_NETWORK_FEE_FOR_ASSET));
		assert_ne!(
			InternalSwapNetworkFeeForAsset::<Test>::get(Asset::Usdc),
			Some(NEW_INTERNAL_SWAP_NETWORK_FEE_FOR_ASSET)
		);

		// Define the updates in a reusable vec
		let updates = vec![
			PalletConfigUpdate::MaximumSwapAmount {
				asset: Asset::Btc,
				amount: NEW_MAX_SWAP_AMOUNT_BTC,
			},
			PalletConfigUpdate::MaximumSwapAmount {
				asset: Asset::Dot,
				amount: NEW_MAX_SWAP_AMOUNT_DOT,
			},
			PalletConfigUpdate::SwapRetryDelay { delay: new_swap_retry_delay },
			PalletConfigUpdate::FlipBuyInterval { interval: new_flip_buy_interval },
			PalletConfigUpdate::SetMaxSwapRetryDuration { blocks: NEW_MAX_SWAP_RETRY_DURATION },
			PalletConfigUpdate::SetMaxSwapRequestDuration { blocks: MAX_SWAP_REQUEST_DURATION },
			PalletConfigUpdate::SetMinimumChunkSize {
				asset: Asset::Usdc,
				size: NEW_MINIMUM_CHUNK_SIZE,
			},
			PalletConfigUpdate::SetNetworkFee {
				rate: Some(NEW_NETWORK_FEE.rate),
				minimum: Some(NEW_NETWORK_FEE.minimum),
			},
			PalletConfigUpdate::SetInternalSwapNetworkFee {
				rate: Some(NEW_INTERNAL_SWAP_FEE.rate),
				minimum: Some(NEW_INTERNAL_SWAP_FEE.minimum),
			},
			PalletConfigUpdate::SetNetworkFeeForAsset {
				asset: Asset::Usdc,
				rate: Some(NEW_NETWORK_FEE_FOR_ASSET),
			},
			PalletConfigUpdate::SetInternalSwapNetworkFeeForAsset {
				asset: Asset::Usdc,
				rate: Some(NEW_INTERNAL_SWAP_NETWORK_FEE_FOR_ASSET),
			},
		];

		// Update all config items at the same time
		assert_ok!(Swapping::update_pallet_config(
			OriginTrait::root(),
			updates.clone().try_into().unwrap()
		));

		// Check that the new values were set
		assert_eq!(MaximumSwapAmount::<Test>::get(Asset::Btc), NEW_MAX_SWAP_AMOUNT_BTC);
		assert_eq!(MaximumSwapAmount::<Test>::get(Asset::Dot), NEW_MAX_SWAP_AMOUNT_DOT);
		assert_eq!(SwapRetryDelay::<Test>::get(), new_swap_retry_delay);
		assert_eq!(FlipBuyInterval::<Test>::get(), new_flip_buy_interval);
		assert_eq!(MaxSwapRetryDurationBlocks::<Test>::get(), NEW_MAX_SWAP_RETRY_DURATION);
		assert_eq!(MaxSwapRequestDurationBlocks::<Test>::get(), MAX_SWAP_REQUEST_DURATION);
		assert_eq!(MinimumChunkSize::<Test>::get(Asset::Usdc), NEW_MINIMUM_CHUNK_SIZE);
		assert_eq!(NetworkFee::<Test>::get(), NEW_NETWORK_FEE);
		assert_eq!(InternalSwapNetworkFee::<Test>::get(), NEW_INTERNAL_SWAP_FEE);
		assert_eq!(NetworkFeeForAsset::<Test>::get(Asset::Usdc), Some(NEW_NETWORK_FEE_FOR_ASSET));
		assert_eq!(
			InternalSwapNetworkFeeForAsset::<Test>::get(Asset::Usdc),
			Some(NEW_INTERNAL_SWAP_NETWORK_FEE_FOR_ASSET)
		);

		// Check that the PalletConfigUpdate event was emitted for each update
		for update in updates {
			cf_test_utilities::assert_has_event::<Test>(RuntimeEvent::Swapping(
				Event::PalletConfigUpdated { update },
			));
		}

		// Check that we can remove a custom network fee for an asset
		assert_ok!(Swapping::update_pallet_config(
			OriginTrait::root(),
			vec![PalletConfigUpdate::SetNetworkFeeForAsset { asset: Asset::Usdc, rate: None }]
				.try_into()
				.unwrap()
		));
		assert_eq!(NetworkFeeForAsset::<Test>::get(Asset::Usdc), None);

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

		let initiate_swap = || {
			Swapping::init_swap_request(
				from,
				amount,
				to,
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
		};

		initiate_swap();

		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900u128);

		// Reset event and confiscated funds.
		CollectedRejectedFunds::<Test>::set(from, 0u128);
		System::reset_events();

		// Max is removed.
		set_maximum_swap_amount(from, None);

		initiate_swap();

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap::new(1.into(), 1.into(), from, to, max_swap, None, vec![ZERO_NETWORK_FEES],),
				// New swap takes the full amount.
				Swap::new(2.into(), 2.into(), from, to, amount, None, vec![ZERO_NETWORK_FEES],),
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

		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::PalletConfigUpdated {
			update: PalletConfigUpdate::MaximumSwapAmount { asset, amount },
		}));

		// Can remove maximum swap amount
		set_maximum_swap_amount(asset, None);
		assert!(MaximumSwapAmount::<Test>::get(asset).is_none());
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::PalletConfigUpdated {
			update: PalletConfigUpdate::MaximumSwapAmount { asset, amount: None },
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

		Swapping::init_swap_request(
			from,
			amount,
			to,
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

		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0u128);

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap::new(1.into(), 1.into(), from, to, amount, None, vec![ZERO_NETWORK_FEES],),]
		);
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
				RuntimeOrigin::signed(BROKER),
				Asset::Eth,
				Asset::Usdc,
				EncodedAddress::Eth(Default::default()),
				1001,
				None,
				0,
				Default::default(),
				REFUND_PARAMS,
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
		<Test as Config>::BalanceApi::credit_account(&BROKER, Asset::Eth, 200);

		// Activate code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();

		// Cannot withdraw
		assert_noop!(
			Swapping::withdraw(
				RuntimeOrigin::signed(BROKER),
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			Error::<Test>::WithdrawalsDisabled
		);

		assert_eq!(get_broker_balance::<Test>(&BROKER, Asset::Eth), 200);

		// Change back to code green
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();

		// withdraws are now allowed
		assert_ok!(Swapping::withdraw(
			RuntimeOrigin::signed(BROKER),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
		assert_eq!(get_broker_balance::<Test>(&BROKER, Asset::Eth), 0);
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
