#![cfg(test)]
use cf_chains::btc::{api::UtxoSelectionType, deposit_address::DepositAddress, Utxo};
use cf_traits::SafeMode;
use frame_support::{assert_ok, traits::OriginTrait};

use crate::{RuntimeSafeMode, SafeModeUpdate};

use crate::mock::*;

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		assert_eq!(STATE_CHAIN_GATEWAY_ADDRESS, Environment::state_chain_gateway_address());
		assert_eq!(KEY_MANAGER_ADDRESS, Environment::key_manager_address());
		assert_eq!(ETH_CHAIN_ID, Environment::ethereum_chain_id());
	});
}

fn add_utxo_amount(amount: crate::BtcAmount) {
	Environment::add_bitcoin_utxo_to_list(
		amount,
		Default::default(),
		DepositAddress::new(Default::default(), Default::default()),
	);
}

#[test]
fn test_btc_utxo_selection() {
	let utxo = |amount| Utxo {
		amount,
		id: Default::default(),
		deposit_address: DepositAddress::new(Default::default(), Default::default()),
	};

	new_test_ext().execute_with(|| {
		// returns none when there are no utxos available for selection
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectAllForRotation),
			None
		);

		// add some UTXOs to the available utxos list.
		add_utxo_amount(10000);
		add_utxo_amount(5000);
		add_utxo_amount(100000);
		add_utxo_amount(5000000);
		add_utxo_amount(25000);
		// dust amount should be ignored in all cases
		let dust_amount = {
			use cf_traits::GetBitcoinFeeInfo;
			<Test as crate::Config>::BitcoinFeeInfo::bitcoin_fee_info().fee_per_input_utxo()
		};
		add_utxo_amount(dust_amount);

		// select some utxos for a tx

		// the default fee is 10 satoshi per byte.
		// inputs are 78 bytes
		// outputs are 51 bytes
		// transactions have a 12 byte base size
		// the fee for 3 inputs and 2 outputs is thus:
		// 10*(16 + 3*78 + 2*51) = 3520 satoshi
		// the expected change is:
		// 5000 + 10000 + 25000 - 12000 - 3520 = 24480 satoshi
		const EXPECTED_CHANGE_AMOUNT: crate::BtcAmount = 24480;
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::Some {
				output_amount: 12000,
				number_of_outputs: 2
			})
			.unwrap(),
			(vec![utxo(5000), utxo(10000), utxo(25000)], EXPECTED_CHANGE_AMOUNT)
		);

		// add the change utxo back to the available utxo list
		add_utxo_amount(EXPECTED_CHANGE_AMOUNT);

		// select all remaining utxos
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectAllForRotation)
				.unwrap(),
			(vec![utxo(5000000), utxo(100000), utxo(EXPECTED_CHANGE_AMOUNT),], 5121470)
		);

		// add some more utxos to the list
		add_utxo_amount(5000);
		add_utxo_amount(15000);

		// request a larger amount than what is available
		assert!(Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::Some {
			output_amount: 20100,
			number_of_outputs: 1
		})
		.is_none());

		// Ensure the previous failure didn't wipe the utxo list
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectAllForRotation)
				.unwrap(),
			(vec![utxo(5000), utxo(15000),], 17770)
		);
	});
}

#[test]
fn test_btc_utxo_consolidation() {
	new_test_ext().execute_with(|| {
		let utxo = |amount| Utxo {
			amount,
			id: Default::default(),
			deposit_address: DepositAddress::new(Default::default(), Default::default()),
		};

		// Reduce consolidation parameters to make testing easier
		assert_ok!(Environment::update_consolidation_parameters(
			OriginTrait::root(),
			cf_chains::btc::ConsolidationParameters {
				consolidation_threshold: 2,
				consolidation_size: 2,
			}
		));

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None
		);

		let dust_amount = {
			use cf_traits::GetBitcoinFeeInfo;
			<Test as crate::Config>::BitcoinFeeInfo::bitcoin_fee_info().fee_per_input_utxo()
		};

		add_utxo_amount(10000);
		// Some utxos exist, but it won't be enough for consolidation:
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None
		);
		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(1));

		// Dust utxo does not count:
		add_utxo_amount(dust_amount);
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None
		);
		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(2));

		add_utxo_amount(20000);
		add_utxo_amount(30000);

		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(4));

		// Should select two UTXOs, with all funds (minus fees) going back to us as change
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			Some((vec![utxo(10000), utxo(20000)], 27770))
		);

		// Any utxo that didn't get consolidated should still be available:
		assert_eq!(
			crate::BitcoinAvailableUtxos::<Test>::get(),
			vec![utxo(30000), utxo(dust_amount)]
		);
	});
}

#[test]
fn updating_consolidation_parameters() {
	new_test_ext().execute_with(|| {
		// Should work with valid parameters
		assert_ok!(Environment::update_consolidation_parameters(
			OriginTrait::root(),
			cf_chains::btc::ConsolidationParameters {
				consolidation_threshold: 2,
				consolidation_size: 2,
			}
		));

		// Should fail with invalid parameters
		assert!(Environment::update_consolidation_parameters(
			OriginTrait::root(),
			cf_chains::btc::ConsolidationParameters {
				consolidation_threshold: 1,
				consolidation_size: 2,
			}
		)
		.is_err());
	});
}

#[test]
fn update_safe_mode() {
	new_test_ext().execute_with(|| {
		// Default to GREEN
		assert_eq!(RuntimeSafeMode::<Test>::get(), SafeMode::CODE_GREEN);
		assert_ok!(Environment::update_safe_mode(OriginTrait::root(), SafeModeUpdate::CodeRed));
		assert_eq!(RuntimeSafeMode::<Test>::get(), SafeMode::CODE_RED);
		System::assert_last_event(RuntimeEvent::Environment(
			crate::Event::<Test>::RuntimeSafeModeUpdated { safe_mode: SafeModeUpdate::CodeRed },
		));

		assert_ok!(Environment::update_safe_mode(OriginTrait::root(), SafeModeUpdate::CodeGreen,));
		assert_eq!(RuntimeSafeMode::<Test>::get(), SafeMode::CODE_GREEN);
		System::assert_last_event(RuntimeEvent::Environment(
			crate::Event::<Test>::RuntimeSafeModeUpdated { safe_mode: SafeModeUpdate::CodeGreen },
		));
		let mock_code_amber =
			MockRuntimeSafeMode { mock: MockPalletSafeMode { flag1: true, flag2: false } };
		assert_ok!(Environment::update_safe_mode(
			OriginTrait::root(),
			SafeModeUpdate::CodeAmber(mock_code_amber.clone())
		));
		assert_eq!(RuntimeSafeMode::<Test>::get(), mock_code_amber);
		System::assert_last_event(RuntimeEvent::Environment(
			crate::Event::<Test>::RuntimeSafeModeUpdated {
				safe_mode: SafeModeUpdate::CodeAmber(mock_code_amber),
			},
		));
	});
}
