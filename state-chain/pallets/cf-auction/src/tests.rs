mod test {
	use crate::*;
	use crate::{mock::*};
	fn events() -> Vec<mock::Event> {
		let evt = System::events().into_iter().map(|evt| evt.event).collect::<Vec<_>>();
		System::reset_events();
		evt
	}

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
	}

	#[test]
	fn tester() {
		new_test_ext().execute_with(|| {
			assert!(true);
		});
	}
}
