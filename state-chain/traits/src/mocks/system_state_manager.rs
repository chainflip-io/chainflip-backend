use crate::SystemStateManager;

pub enum SystemState {
	Normal,
	Maintenance,
}

// do not know how to solve this mock
pub struct MockSystemStateManager;

impl SystemStateManager for MockSystemStateManager {
	type SystemState = SystemState;

	fn set_system_state(_state: Self::SystemState) {
		todo!()
	}

	fn get_maintenance_state() -> Self::SystemState {
		todo!()
	}
}
