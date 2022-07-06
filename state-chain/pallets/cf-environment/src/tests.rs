use cf_traits::SystemStateInfo;
use frame_support::{assert_noop, assert_ok};

use crate::{mock::*, Error, SystemState, SystemStateProvider};

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
		assert_eq!(frame_system::Pallet::<Test>::events().len(), 0);
		assert_ok!(Environment::set_system_state(Origin::root(), SystemState::Maintenance));
		assert_eq!(SystemState::Maintenance, Environment::system_state());
		assert_eq!(
			frame_system::Pallet::<Test>::events()
				.pop()
				.expect("Event should be emitted!")
				.event,
			crate::mock::Event::Environment(crate::Event::SystemStateUpdated {
				new_system_state: SystemState::Maintenance
			}),
			"System state is not Maintenance!"
		);
		assert_eq!(frame_system::Pallet::<Test>::events().len(), 1);
		// Try to set the same state again
		assert_ok!(Environment::set_system_state(Origin::root(), SystemState::Maintenance));
		// Expect no event to be emitted if the state is already set to Maintenance - unfortunately
		// we cannot remove events from the queue in tests therfore we have to check if the queue
		// has grown or not :/
		assert_eq!(frame_system::Pallet::<Test>::events().len(), 1);
		assert_ok!(Environment::set_system_state(Origin::root(), SystemState::Normal));
		assert_eq!(
			frame_system::Pallet::<Test>::events()
				.pop()
				.expect("Event should be emitted!")
				.event,
			crate::mock::Event::Environment(crate::Event::SystemStateUpdated {
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
		assert_ok!(Environment::set_system_state(Origin::root(), SystemState::Normal));
		assert_ok!(SystemStateProvider::<Test>::ensure_no_maintenance());
		assert_ok!(Environment::set_system_state(Origin::root(), SystemState::Maintenance));
		assert_noop!(
			SystemStateProvider::<Test>::ensure_no_maintenance(),
			<Error<Test>>::NetworkIsInMaintenance
		);
	});
}
