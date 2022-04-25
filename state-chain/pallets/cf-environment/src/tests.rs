use frame_support::assert_ok;

use crate::{mock::*, SystemState};

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		assert_eq!(STAKE_MANAGER_ADDRESS, Environment::stake_manager_address());
		assert_eq!(KEY_MANAGER_ADDRESS, Environment::key_manager_address());
		assert_eq!(ETH_CHAIN_ID, Environment::ethereum_chain_id());
		assert_eq!(CFE_SETTINGS, Environment::cfe_settings());
		assert_eq!(SystemState::Normal, Environment::system_state());
	});
}

#[test]
fn change_network_state() {
	new_test_ext().execute_with(|| {
		assert_ok!(Environment::set_system_state(Origin::root(), SystemState::Maintenance));
		assert_eq!(SystemState::Maintenance, Environment::system_state());
		assert_eq!(
			frame_system::Pallet::<Test>::events()
				.pop()
				.expect("Event should be emitted!")
				.event,
			crate::mock::Event::Environment(crate::Event::SystemStateHasBeenChanged(
				SystemState::Maintenance
			)),
			"System state is not Maintenance!"
		);
	});
}
