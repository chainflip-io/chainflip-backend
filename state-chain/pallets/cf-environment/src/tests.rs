#![cfg(test)]

use cf_chains::btc::{
	api::UtxoSelectionType, deposit_address::DepositAddress, utxo_selection, AggKey,
	BitcoinFeeInfo, BtcAmount, Utxo, CHANGE_ADDRESS_SALT,
};
use cf_traits::SafeMode;
use frame_support::{assert_ok, traits::OriginTrait};

use crate::{
	mock::*, BitcoinAvailableUtxos, ConsolidationParameters, RuntimeSafeMode, SafeModeUpdate,
};

fn utxo(amount: BtcAmount, salt: u32, pub_key: Option<[u8; 32]>) -> Utxo {
	Utxo {
		amount,
		id: Default::default(),
		deposit_address: DepositAddress::new(pub_key.unwrap_or_default(), salt),
	}
}

fn utxo_with_key(pub_key: [u8; 32]) -> Utxo {
	utxo(1_000_000, CHANGE_ADDRESS_SALT, Some(pub_key))
}

fn add_utxo_amount(amount: BtcAmount, salt: u32) {
	Environment::add_bitcoin_utxo_to_list(
		amount,
		Default::default(),
		DepositAddress::new(Default::default(), salt),
	);
}

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		assert_eq!(STATE_CHAIN_GATEWAY_ADDRESS, Environment::state_chain_gateway_address());
		assert_eq!(ETH_KEY_MANAGER_ADDRESS, Environment::key_manager_address());
		assert_eq!(ARB_KEY_MANAGER_ADDRESS, Environment::arb_key_manager_address());
		assert_eq!(ETH_CHAIN_ID, Environment::ethereum_chain_id());
		assert_eq!(ARB_CHAIN_ID, Environment::arbitrum_chain_id());
	});
}

#[test]
fn test_btc_utxo_selection() {
	new_test_ext().execute_with(|| {
		// add some UTXOs to the available utxos list.
		add_utxo_amount(10000, 0);
		add_utxo_amount(5000, 1);
		add_utxo_amount(100000, 2);
		add_utxo_amount(5000000, 3);
		add_utxo_amount(25000, 4);
		// dust amount should be ignored in all cases
		let dust_amount = {
			use cf_traits::GetBitcoinFeeInfo;
			<Test as crate::Config>::BitcoinFeeInfo::bitcoin_fee_info().fee_per_input_utxo()
		};
		add_utxo_amount(dust_amount, 5);

		// select some utxos for a tx

		// the default fee is 10 satoshi per byte.
		// inputs are 78 bytes
		// vault inputs are 58 bytes
		// outputs are 51 bytes
		// transactions have a 16 byte base size
		// the fee for these 3 inputs and 2 outputs is thus:
		// 10*(16 + 58 + 2*78 + 2*51) = 3320 satoshi
		// the expected change is:
		// 5000 + 10000 + 25000 - 12000 - 3320 = 24680 satoshi
		const EXPECTED_CHANGE_AMOUNT: crate::BtcAmount = 24680;
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::Some {
				output_amount: 12000,
				number_of_outputs: 2
			})
			.unwrap(),
			(
				vec![utxo(5000, 1, None), utxo(10000, 0, None), utxo(25000, 4, None)],
				EXPECTED_CHANGE_AMOUNT
			)
		);

		// request a larger amount than what is available
		assert!(Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::Some {
			output_amount: 5100001,
			number_of_outputs: 1
		})
		.is_none());

		// Ensure the previous failure didn't wipe the utxo list
		assert_eq!(
			BitcoinAvailableUtxos::<Test>::get(),
			vec![utxo(dust_amount, 5, None), utxo(100000, 2, None), utxo(5000000, 3, None)],
		);
	});
}

#[test]
fn updating_consolidation_parameters() {
	new_test_ext().execute_with(|| {
		let valid_param = utxo_selection::ConsolidationParameters {
			consolidation_threshold: 2,
			consolidation_size: 2,
		};
		// Should work with valid parameters
		assert_ok!(Environment::update_consolidation_parameters(OriginTrait::root(), valid_param,));

		System::assert_last_event(RuntimeEvent::Environment(
			crate::Event::<Test>::UtxoConsolidationParametersUpdated { params: valid_param },
		));

		// Should fail with invalid parameters
		assert!(Environment::update_consolidation_parameters(
			OriginTrait::root(),
			utxo_selection::ConsolidationParameters {
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

#[test]
fn can_discard_stale_utxos() {
	let epoch_1 = [0xFE; 32];
	let epoch_2 = [0xAA; 32];
	let epoch_3 = [0xBB; 32];
	let epoch_4 = [0xDD; 32];
	new_test_ext().execute_with(|| {
		ConsolidationParameters::<Test>::set(utxo_selection::ConsolidationParameters {
			consolidation_threshold: 5,
			consolidation_size: 2,
		});

		// Does not discard UTXOs if previous key not set:
		MockBitcoinKeyProvider::set_key(AggKey { current: epoch_2, previous: None });

		BitcoinAvailableUtxos::<Test>::set(vec![
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_2),
			utxo_with_key(epoch_3),
		]);

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None
		);

		// Have not reached threshold, but will still move previous epoch UTXO into the new vault
		// as part of "consolidation". Epoch 1 utxos are discarded.
		MockBitcoinKeyProvider::set_key(AggKey { current: epoch_3, previous: Some(epoch_2) });
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation,)
				.unwrap()
				.0,
			vec![utxo_with_key(epoch_2)]
		);

		System::assert_has_event(RuntimeEvent::Environment(crate::Event::StaleUtxosDiscarded {
			utxos: vec![utxo_with_key(epoch_1), utxo_with_key(epoch_1)],
		}));

		// Can "consolidate" and discard at the same time
		BitcoinAvailableUtxos::<Test>::append(utxo_with_key(epoch_1));

		MockBitcoinKeyProvider::set_key(AggKey { current: epoch_4, previous: Some(epoch_3) });

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation,)
				.unwrap()
				.0,
			vec![utxo_with_key(epoch_3)]
		);

		System::assert_has_event(RuntimeEvent::Environment(crate::Event::StaleUtxosDiscarded {
			utxos: vec![utxo_with_key(epoch_1)],
		}));
	});
}

#[test]
fn can_consolidate_current_and_prev_utxos() {
	let epoch_1 = [0xAA; 32];
	let epoch_2 = [0xBB; 32];
	const CONSOLIDATION_SIZE: u32 = 4;
	new_test_ext().execute_with(|| {
		MockBitcoinKeyProvider::set_key(AggKey { current: epoch_2, previous: Some(epoch_1) });
		ConsolidationParameters::<Test>::set(utxo_selection::ConsolidationParameters {
			consolidation_threshold: 5,
			consolidation_size: CONSOLIDATION_SIZE,
		});

		BitcoinAvailableUtxos::<Test>::set(vec![
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_2),
			utxo_with_key(epoch_2),
		]);

		// Consolidate from storage. Take the first 4 utxos.
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation)
				.unwrap()
				.0
				.len(),
			CONSOLIDATION_SIZE as usize
		);

		assert_eq!(BitcoinAvailableUtxos::<Test>::get(), vec![utxo_with_key(epoch_2),]);

		// Do nothing now that the number of utxos are below threshold.
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None,
		);
	});
}

#[test]
fn can_consolidate_old_utxo_only() {
	let epoch_1 = [0xFE; 32];
	let epoch_2 = [0xAA; 32];
	const CONSOLIDATION_SIZE: u32 = 2;

	new_test_ext().execute_with(|| {
		// Set current key to epoch 2, and transfer limit to 2 utxo at a time.
		MockBitcoinKeyProvider::set_key(AggKey { current: epoch_2, previous: Some(epoch_1) });
		ConsolidationParameters::<Test>::set(utxo_selection::ConsolidationParameters {
			consolidation_threshold: 10,
			consolidation_size: CONSOLIDATION_SIZE,
		});

		BitcoinAvailableUtxos::<Test>::set(vec![
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_2),
		]);

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation)
				.unwrap()
				.0
				.len(),
			CONSOLIDATION_SIZE as usize,
		);

		assert_eq!(
			BitcoinAvailableUtxos::<Test>::get(),
			vec![utxo_with_key(epoch_1), utxo_with_key(epoch_2),]
		);

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation)
				.unwrap()
				.0,
			vec![utxo_with_key(epoch_1)],
		);

		assert_eq!(BitcoinAvailableUtxos::<Test>::get(), vec![utxo_with_key(epoch_2)]);

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None,
		);
	});
}

#[test]
fn do_nothing_with_no_key_set() {
	let epoch_1 = [0xFE; 32];
	let epoch_2 = [0xAA; 32];
	let epoch_3 = [0xBB; 32];
	new_test_ext().execute_with(|| {
		BitcoinAvailableUtxos::<Test>::set(vec![
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_1),
			utxo_with_key(epoch_2),
			utxo_with_key(epoch_2),
			utxo_with_key(epoch_3),
			utxo_with_key(epoch_3),
		]);

		// No changes if vault key is not set
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None,
		);
		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(6));
	});
}

#[test]
fn test_consolidation_change_amount() {
	const INPUT_AMOUNT: BtcAmount = 10_000;
	const NUM_INPUTS: usize = 3;

	let utxos = std::iter::repeat_with(|| utxo(INPUT_AMOUNT, 0, None))
		.take(NUM_INPUTS)
		.collect::<Vec<_>>();

	let fee_info = BitcoinFeeInfo::new(100);
	assert_eq!(
		crate::Pallet::<Test>::consolidation_transaction_change_amount(&utxos[..], &fee_info)
			.unwrap(),
		INPUT_AMOUNT * NUM_INPUTS as BtcAmount - // total available amount
			fee_info.min_fee_required_per_tx() - // base fee
			fee_info.fee_per_output_utxo() - // fee for the change output
			NUM_INPUTS as u64 * fee_info.fee_per_input_utxo() // fee for each input
	);

	// If fees are too high, we cannot consolidate.
	assert!(crate::Pallet::<Test>::consolidation_transaction_change_amount(
		&utxos[..],
		&BitcoinFeeInfo::new(100_000),
	)
	.is_none());
}
