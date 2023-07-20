#![cfg(test)]
use cf_chains::{
	btc::{api::UtxoSelectionType, deposit_address::DepositAddress, ScriptPubkey, Utxo},
	dot::{RuntimeVersion, TEST_RUNTIME_VERSION},
};
use cf_primitives::chains::assets::eth::Asset;
use cf_traits::SafeMode;
use frame_support::{assert_noop, assert_ok, traits::OriginTrait};

use crate::{EthereumSupportedAssets, RuntimeSafeMode, SafeModeUpdate};

use crate::{mock::*, Error};

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		assert_eq!(STATE_CHAIN_GATEWAY_ADDRESS, Environment::state_chain_gateway_address());
		assert_eq!(KEY_MANAGER_ADDRESS, Environment::key_manager_address());
		assert_eq!(ETH_CHAIN_ID, Environment::ethereum_chain_id());
	});
}

#[test]
fn update_supported_eth_assets() {
	new_test_ext().execute_with(|| {
		// Expect the FLIP token address to be set after genesis
		assert!(EthereumSupportedAssets::<Test>::contains_key(Asset::Flip));
		// Update the address for Usdc
		assert_ok!(Environment::update_supported_eth_assets(
			RuntimeOrigin::root(),
			Asset::Usdc,
			[2; 20]
		));
		assert_eq!(EthereumSupportedAssets::<Test>::get(Asset::Usdc), Some([2; 20]));
		assert_eq!(
			frame_system::Pallet::<Test>::events()
				.pop()
				.expect("Event should be emitted!")
				.event,
			crate::mock::RuntimeEvent::Environment(crate::Event::UpdatedEthAsset(
				Asset::Usdc,
				[2; 20]
			),)
		);
		// Last but not least - verify we can not add an address for ETH
		assert_noop!(
			Environment::update_supported_eth_assets(RuntimeOrigin::root(), Asset::Eth, [3; 20]),
			<Error<Test>>::EthAddressNotUpdateable
		);
	});
}

#[test]
fn test_update_polkadot_runtime_version() {
	new_test_ext().execute_with(|| {
		assert_eq!(Environment::polkadot_runtime_version(), TEST_RUNTIME_VERSION);

		// This should be a noop since the version is the same as the genesis version
		assert_noop!(
			Environment::update_polkadot_runtime_version(
				RuntimeOrigin::root(),
				TEST_RUNTIME_VERSION,
			),
			Error::<Test>::InvalidPolkadotRuntimeVersion
		);

		let update_to = RuntimeVersion {
			spec_version: TEST_RUNTIME_VERSION.spec_version + 1,
			transaction_version: 1,
		};
		assert_ok!(Environment::update_polkadot_runtime_version(RuntimeOrigin::root(), update_to));
		assert_eq!(Environment::polkadot_runtime_version(), update_to);
	});
}

#[test]
fn test_btc_utxo_selection() {
	const SCRIPT_PUBKEY: ScriptPubkey = ScriptPubkey::Taproot([0u8; 32]);

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
		Environment::add_bitcoin_utxo_to_list(10000, Default::default(), SCRIPT_PUBKEY);
		Environment::add_bitcoin_utxo_to_list(5000, Default::default(), SCRIPT_PUBKEY);
		Environment::add_bitcoin_utxo_to_list(100000, Default::default(), SCRIPT_PUBKEY);
		Environment::add_bitcoin_utxo_to_list(5000000, Default::default(), SCRIPT_PUBKEY);
		Environment::add_bitcoin_utxo_to_list(25000, Default::default(), SCRIPT_PUBKEY);

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
		Environment::add_bitcoin_utxo_to_list(120080, Default::default(), SCRIPT_PUBKEY);

		// select all remaining utxos
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectAllForRotation)
				.unwrap(),
			(vec![utxo(5000000), utxo(120080),], 5116060)
		);

		// add some more utxos to the list
		Environment::add_bitcoin_utxo_to_list(5000, Default::default(), SCRIPT_PUBKEY);
		Environment::add_bitcoin_utxo_to_list(15000, Default::default(), SCRIPT_PUBKEY);

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
