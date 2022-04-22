use frame_support::assert_ok;

use crate::{mock::*, NetworkState};

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		assert_eq!(STAKE_MANAGER_ADDRESS, Environment::stake_manager_address());
		assert_eq!(KEY_MANAGER_ADDRESS, Environment::key_manager_address());
		assert_eq!(ETH_CHAIN_ID, Environment::ethereum_chain_id());
		assert_eq!(CFE_SETTINGS, Environment::cfe_settings());
		assert_eq!(NetworkState::Running, Environment::network_state());
	});
}

#[test]
fn change_network_state() {
	new_test_ext().execute_with(|| {
		assert_ok!(Environment::set_network_state(Origin::root(), NetworkState::Paused));
		assert_eq!(NetworkState::Paused, Environment::network_state());
		assert_eq!(
			frame_system::Pallet::<Test>::events()
				.pop()
				.expect("Event should be emitted!")
				.event,
			crate::mock::Event::Environment(crate::Event::NetworkStateHasBeenChanged(
				NetworkState::Paused
			)),
			"NetworkStateHasBeenChanged is not paused!"
		);
	});
}
