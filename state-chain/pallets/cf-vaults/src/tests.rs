mod test {
	use crate::mock::*;
	use crate::*;
	use frame_support::{assert_noop, assert_ok};

	fn last_event() -> mock::Event {
		frame_system::Pallet::<MockRuntime>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	#[test]
	fn genesis() {
		new_test_ext().execute_with(|| {

		});
	}
}
