#![cfg(test)]

use cf_chains::btc::{
	api::UtxoSelectionType, deposit_address::DepositAddress, utxo_selection, AggKey, BtcAmount,
	Utxo, CHANGE_ADDRESS_SALT,
};
use cf_traits::{EpochKey, SafeMode};
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

fn agg_key(current: [u8; 32], previous: Option<[u8; 32]>) -> EpochKey<AggKey> {
	EpochKey { key: AggKey { previous, current }, epoch_index: 1 }
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
fn test_btc_utxo_consolidation() {
	new_test_ext().execute_with(|| {
		// Reduce consolidation parameters to make testing easier
		assert_ok!(Environment::update_consolidation_parameters(
			OriginTrait::root(),
			utxo_selection::ConsolidationParameters {
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

		add_utxo_amount(10000, 0);
		// Some utxos exist, but it won't be enough for consolidation:
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None
		);
		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(1));

		// Dust utxo does not count:
		add_utxo_amount(dust_amount, 1);
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None
		);
		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(2));

		add_utxo_amount(20000, 2);
		add_utxo_amount(30000, 3);

		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(4));

		// Should select two UTXOs, with all funds (minus fees) going back to us as change
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			Some((vec![utxo(10000, 0, None), utxo(20000, 2, None)], 27970))
		);

		// Any utxo that didn't get consolidated should still be available:
		assert_eq!(
			crate::BitcoinAvailableUtxos::<Test>::get(),
			vec![utxo(30000, 3, None), utxo(dust_amount, 1, None)]
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
fn can_consolidate_utxo_to_current_vault_and_discard_stale_utxos() {
	let epoch_1 = [0xFE; 32];
	let epoch_2 = [0xAA; 32];
	let epoch_3 = [0xBB; 32];

	let epoch_2_utxos = vec![
		utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
		utxo(22_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
		utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
	];
	let epoch_3_utxos = vec![
		utxo(31_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
		utxo(32_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
		utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
		utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
		utxo(35_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
	];
	let to_discard_utxo = vec![utxo(1_000_000, 1, None), utxo(2_000_000, 2, None)];

	new_test_ext().execute_with(|| {
		// Set current key to epoch 2, and transfer limit to 2 utxo at a time.
		CurrentBitcoinKey::set(Some(agg_key(epoch_2, Some(epoch_1))));
		ConsolidationParameters::<Test>::set(utxo_selection::ConsolidationParameters {
			consolidation_threshold: 5,
			consolidation_size: 2,
		});

		BitcoinAvailableUtxos::<Test>::set(epoch_2_utxos.clone());

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None
		);

		// no changes to utxo since all utxo are part of the current vault.
		assert_eq!(BitcoinAvailableUtxos::<Test>::get(), epoch_2_utxos);

		// Rotate key into the next vault
		CurrentBitcoinKey::set(Some(agg_key(epoch_3, Some(epoch_2))));

		BitcoinAvailableUtxos::<Test>::mutate(|utxos| {
			utxos.append(&mut epoch_3_utxos.clone());

			// These should be discarded.
			utxos.append(&mut to_discard_utxo.clone());
		});
		assert_eq!(BitcoinAvailableUtxos::<Test>::decode_len(), Some(10));

		// Consolidate from current vault takes priority. No consolidation can happen this block.
		// Only 2 Utxos from previous vault are sent to the current Vault. Remaining are
		// appended to the back.
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			Environment::calculate_utxos_and_change(vec![
				utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
				utxo(22_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
			])
		);

		System::assert_has_event(RuntimeEvent::Environment(crate::Event::StaleUtxosDiscarded {
			utxos: to_discard_utxo,
		}));

		assert_eq!(
			BitcoinAvailableUtxos::<Test>::get(),
			vec![
				utxo(31_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(32_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(35_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
			]
		);

		// Transfer old utxo and consolidate within the same transaction.
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			Environment::calculate_utxos_and_change(vec![
				utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
				utxo(31_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
			]),
		);

		assert_eq!(
			BitcoinAvailableUtxos::<Test>::get(),
			vec![
				utxo(32_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(35_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
			]
		);

		// Utxos from epoch 3 is now below the threshold.
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

	new_test_ext().execute_with(|| {
		// Set current key to epoch 2, and transfer limit to 2 utxo at a time.
		CurrentBitcoinKey::set(Some(agg_key(epoch_2, Some(epoch_1))));
		ConsolidationParameters::<Test>::set(utxo_selection::ConsolidationParameters {
			consolidation_threshold: 5,
			consolidation_size: 2,
		});

		BitcoinAvailableUtxos::<Test>::set(vec![
			utxo(1_000_000, CHANGE_ADDRESS_SALT, Some(epoch_1)),
			utxo(2_000_000, CHANGE_ADDRESS_SALT, Some(epoch_1)),
			utxo(3_000_000, CHANGE_ADDRESS_SALT, Some(epoch_1)),
			utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
		]);

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			Environment::calculate_utxos_and_change(vec![
				utxo(1_000_000, CHANGE_ADDRESS_SALT, Some(epoch_1)),
				utxo(2_000_000, CHANGE_ADDRESS_SALT, Some(epoch_1)),
			])
		);

		assert_eq!(
			BitcoinAvailableUtxos::<Test>::get(),
			vec![
				utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
				utxo(3_000_000, CHANGE_ADDRESS_SALT, Some(epoch_1)),
			]
		);

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			Environment::calculate_utxos_and_change(vec![utxo(
				3_000_000,
				CHANGE_ADDRESS_SALT,
				Some(epoch_1)
			),]),
		);

		assert_eq!(
			BitcoinAvailableUtxos::<Test>::get(),
			vec![utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),]
		);

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
			utxo(1_000_000, CHANGE_ADDRESS_SALT, Some(epoch_1)),
			utxo(2_000_000, CHANGE_ADDRESS_SALT, Some(epoch_1)),
			utxo(3_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
			utxo(4_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
			utxo(5_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
			utxo(6_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
		]);

		// No changes if vault key is not set
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None,
		);
		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(6));

		// Set key for current vault.
		CurrentBitcoinKey::set(Some(agg_key(epoch_2, None)));

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			None,
		);

		assert_eq!(crate::BitcoinAvailableUtxos::<Test>::decode_len(), Some(6));

		// Only transfer and discard stale utxos when previous key is available.
		CurrentBitcoinKey::set(Some(agg_key(epoch_3, Some(epoch_2))));

		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectForConsolidation),
			Environment::calculate_utxos_and_change(vec![
				utxo(3_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
				utxo(4_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
			]),
		);

		assert_eq!(
			BitcoinAvailableUtxos::<Test>::get(),
			vec![
				utxo(5_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				utxo(6_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
			]
		);
	});
}
