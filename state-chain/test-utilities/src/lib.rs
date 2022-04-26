use frame_system::Config;

pub fn last_event<Runtime: Config>() -> <Runtime as Config>::Event {
	frame_system::Pallet::<Runtime>::events().pop().expect("Event expected").event
}

/// Checks the deposited events, in reverse order (reverse order mainly because it makes the macro
/// easier to write).
#[macro_export]
macro_rules! assert_event_sequence {
	($($pat:pat $( => $test:block )? ),*) => {
		let mut events = frame_system::Pallet::<Test>::events()
		.into_iter()
		.rev()
		.map(|e| e.event)
			.collect::<Vec<_>>();

		$(
			let actual = events.pop().expect("Expected an event.");
			#[allow(irrefutable_let_patterns)]
			if let $pat = actual {
				$(
					$test
				)?
			} else {
				assert!(false, "Expected event {:?}. Got {:?}", stringify!($pat), actual);
			}
		)*
	};
}
