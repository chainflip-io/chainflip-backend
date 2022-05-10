use frame_system::Config;

pub fn last_event<T: Config>() -> <T as Config>::Event {
	frame_system::Pallet::<T>::events().pop().expect("Event expected").event
}

/// Checks the deposited events in the order they occur
#[macro_export]
macro_rules! assert_event_sequence {
	($runtime:ty, $($evt:expr),*) => {
		let mut events = frame_system::Pallet::<$runtime>::events()
		.into_iter()
		// We want to be able to input the events into this macro in the order they occurred.
		.rev()
		.map(|e| e.event)
			.collect::<Vec<_>>();

		$(
			let actual = events.pop().expect("Expected an event.");
			assert_eq!(actual, $evt);
		)*
	};
}
