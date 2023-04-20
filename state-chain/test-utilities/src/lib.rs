use frame_system::Config;

pub fn last_event<T: Config>() -> <T as Config>::RuntimeEvent {
	maybe_last_event::<T>().expect("Event expected")
}

pub fn maybe_last_event<T: Config>() -> Option<<T as Config>::RuntimeEvent> {
	frame_system::Pallet::<T>::events().pop().map(|e| e.event)
}

/// Can be used to check that fixed-sized types have the correct implementation of MaxEncodedLen
pub fn ensure_max_encoded_len_is_exact<T: Default + codec::Encode + codec::MaxEncodedLen>() {
	assert_eq!(T::default().encode().len(), T::max_encoded_len());
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

#[macro_export]
macro_rules! assert_events_match {
	($runtime:ty, $($pattern:pat $(if $guard:expr )? => $bind:expr),+ ) => {{
		let mut events = frame_system::Pallet::<$runtime>::events();

		(
			$({
				let (index, bind) = events
					.iter()
					.enumerate()
					.find_map(|(index, record)| match record.event.clone() {
						$pattern $(if $guard)? => Some((index, $bind)),
						_ => None
					})
					.unwrap_or_else(|| panic!("No event that matches {}. Available events: {:#?}", stringify!($pattern), events));
				events.remove(index);
				bind
			}),+
		)
	}};
}

#[macro_export]
macro_rules! assert_events_eq {
	($runtime:ty, $($event:expr),+ ) => {{
		let mut events = frame_system::Pallet::<$runtime>::events();

		$({
			let event = $event;
			let index = events
				.iter()
				.enumerate()
				.find_map(|(index, record)| (record.event == event).then_some(index))
				.unwrap_or_else(|| panic!("No event equal to {:?}. Available events: {:#?}", event, events));
			events.remove(index);
		});+
	}};
}
