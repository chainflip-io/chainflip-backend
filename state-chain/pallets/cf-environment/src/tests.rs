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

#[test]
fn test_btc_utxo_selection() {
	fn add_utxo_amount(amount: crate::BtcAmount) {
		Environment::add_bitcoin_utxo_to_list(
			amount,
			Default::default(),
			DepositAddress::new(Default::default(), Default::default()),
		);
	}

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
			<Test as crate::Config>::BitcoinFeeInfo::bitcoin_fee_info().fee_per_input_utxo
		};
		add_utxo_amount(dust_amount);

		// select some utxos for a tx
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::Some {
				output_amount: 12000,
				number_of_outputs: 2
			})
			.unwrap(),
			(vec![utxo(5000), utxo(10000), utxo(25000), utxo(100000)], 120080)
		);

		// add the change utxo back to the available utxo list
		add_utxo_amount(120080);

		// select all remaining utxos
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectAllForRotation)
				.unwrap(),
			(vec![utxo(5000000), utxo(120080),], 5116060)
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
			(vec![utxo(5000), utxo(15000),], 15980)
		);
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
