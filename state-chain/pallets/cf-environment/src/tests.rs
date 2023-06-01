#![cfg(test)]
use cf_chains::{
	btc::{api::UtxoSelectionType, Utxo},
	dot::{RuntimeVersion, TEST_RUNTIME_VERSION},
};
use cf_primitives::chains::assets::eth::Asset;
use cf_traits::SystemStateInfo;
use frame_support::{assert_noop, assert_ok};

use crate::EthereumSupportedAssets;

use crate::{mock::*, Error, SystemState, SystemStateProvider};

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		assert_eq!(STATE_CHAIN_GATEWAY_ADDRESS, Environment::state_chain_gateway_address());
		assert_eq!(KEY_MANAGER_ADDRESS, Environment::key_manager_address());
		assert_eq!(ETH_CHAIN_ID, Environment::ethereum_chain_id());
		assert_eq!(CFE_SETTINGS, Environment::cfe_settings());
		assert_eq!(SystemState::Normal, Environment::system_state());
	});
}

#[test]
fn change_network_state() {
	new_test_ext().execute_with(|| {
		assert_eq!(frame_system::Pallet::<Test>::events().len(), 0);
		assert_ok!(Environment::set_system_state(RuntimeOrigin::root(), SystemState::Maintenance));
		assert_eq!(SystemState::Maintenance, Environment::system_state());
		assert_eq!(
			frame_system::Pallet::<Test>::events()
				.pop()
				.expect("Event should be emitted!")
				.event,
			crate::mock::RuntimeEvent::Environment(crate::Event::SystemStateUpdated {
				new_system_state: SystemState::Maintenance
			}),
			"System state is not Maintenance!"
		);
		assert_eq!(frame_system::Pallet::<Test>::events().len(), 1);
		// Try to set the same state again
		assert_ok!(Environment::set_system_state(RuntimeOrigin::root(), SystemState::Maintenance));
		// Expect no event to be emitted if the state is already set to Maintenance - unfortunately
		// we cannot remove events from the queue in tests therfore we have to check if the queue
		// has grown or not :/
		assert_eq!(frame_system::Pallet::<Test>::events().len(), 1);
		assert_ok!(Environment::set_system_state(RuntimeOrigin::root(), SystemState::Normal));
		assert_eq!(
			frame_system::Pallet::<Test>::events()
				.pop()
				.expect("Event should be emitted!")
				.event,
			crate::mock::RuntimeEvent::Environment(crate::Event::SystemStateUpdated {
				new_system_state: SystemState::Normal
			}),
			"System state is not Normal!"
		);
		assert_eq!(frame_system::Pallet::<Test>::events().len(), 2);
	});
}

#[test]
fn ensure_no_maintenance() {
	new_test_ext().execute_with(|| {
		assert_ok!(Environment::set_system_state(RuntimeOrigin::root(), SystemState::Normal));
		assert_ok!(SystemStateProvider::<Test>::ensure_no_maintenance());
		assert_ok!(Environment::set_system_state(RuntimeOrigin::root(), SystemState::Maintenance));
		assert_noop!(
			SystemStateProvider::<Test>::ensure_no_maintenance(),
			<Error<Test>>::NetworkIsInMaintenance
		);
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
	new_test_ext().execute_with(|| {
		// returns none when there are no utxos available for selection
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectAllForRotation),
			None
		);

		// add some UTXOs to the available utxos list.
		Environment::add_bitcoin_utxo_to_list(10000, Default::default(), Default::default());
		Environment::add_bitcoin_utxo_to_list(5000, Default::default(), Default::default());
		Environment::add_bitcoin_utxo_to_list(100000, Default::default(), Default::default());
		Environment::add_bitcoin_utxo_to_list(5000000, Default::default(), Default::default());
		Environment::add_bitcoin_utxo_to_list(25000, Default::default(), Default::default());

		// select some utxos for a tx
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::Some {
				output_amount: 12000,
				number_of_outputs: 2
			})
			.unwrap(),
			(
				vec![
					Utxo { amount: 5000, ..Default::default() },
					Utxo { amount: 10000, ..Default::default() },
					Utxo { amount: 25000, ..Default::default() },
					Utxo { amount: 100000, ..Default::default() }
				],
				120080
			)
		);

		// add the change utxo back to the available utxo list
		Environment::add_bitcoin_utxo_to_list(120080, Default::default(), Default::default());

		// select all remaining utxos
		assert_eq!(
			Environment::select_and_take_bitcoin_utxos(UtxoSelectionType::SelectAllForRotation)
				.unwrap(),
			(
				vec![
					Utxo { amount: 5000000, ..Default::default() },
					Utxo { amount: 120080, ..Default::default() },
				],
				5116060
			)
		);
	});
}
