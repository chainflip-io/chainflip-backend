use frame_system::Config;

pub fn last_event<Runtime: Config>() -> <Runtime as Config>::Event {
	frame_system::Pallet::<Runtime>::events().pop().expect("Event expected").event
}
