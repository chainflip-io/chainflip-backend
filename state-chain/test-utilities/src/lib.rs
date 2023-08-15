use frame_system::Config;

mod rich_test_externalities;

pub use rich_test_externalities::*;

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

#[track_caller]
pub fn assert_has_event<T: frame_system::Config>(event: <T as frame_system::Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events()
		.into_iter()
		.map(|e| e.event)
		.collect::<Vec<_>>();
	assert!(events.iter().any(|e| e == &event), "Event {event:#?} not found in {events:#?}",);
}

#[macro_export]
macro_rules! assert_has_matching_event {
	(( $runtime:ty, $event:pat )) => {
		let events = frame_system::Pallet::<T>::events()
			.into_iter()
			.map(|e| e.event)
			.collect::<Vec<_>>();
		assert!(
			events.iter().any(|e| matches!(e, $event)),
			"No event matching {stringify!($event):#?} found in {events:#?}",
		);
	};
}

/// Checks the deposited events in the order they occur
#[macro_export]
macro_rules! assert_event_sequence {
	($runtime:ty, $( $evt:pat $(if $guard:expr )? ),* $(,)?) => {
		let mut events = frame_system::Pallet::<$runtime>::events()
		.into_iter()
		// We want to be able to input the events into this macro in the order they occurred.
		.rev()
		.map(|e| e.event)
			.collect::<Vec<_>>();

		$(
			let actual = events.pop().unwrap_or_else(|| panic!("No more events. Expected: {:?}", stringify!($evt)));
			assert!(matches!(actual, $evt $(if $guard)?), "Expected: {:?}. Actual: {:?}", stringify!($evt $(if $guard)?), actual);
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

/// Implements test helpers for running tests with [rich_test_externalities::TestExternalities]:
///
/// - `TestRunner` type alias for `TestExternalities` with all pallets and `()` as context.
/// - `with_genesis` function to create a new `TestRunner` with the provided genesis config.
/// - `new_test_ext` function to create a new `TestRunner` with the default genesis config.
#[macro_export]
macro_rules! impl_test_helpers {
	( $runtime:ty ) => {
		/// Test runner wrapping [sp_io::TestExternalities] in a richer api.
		pub type TestRunner<Ctx> = $crate::TestExternalities<$runtime, AllPalletsWithSystem, Ctx>;

		/// Create new test externalities with the provided genesis config.
		pub fn with_genesis(g: RuntimeGenesisConfig) -> TestRunner<()> {
			TestRunner::<()>::new(g)
		}

		/// Create new test externalities with the default genesis config.
		pub fn new_test_ext() -> TestRunner<()> {
			with_genesis(Default::default())
		}
	};
}

#[track_caller]
pub fn assert_within_error<T>(a: T, b: T, err: T)
where
	T: std::ops::Sub<Output = T> + std::cmp::PartialOrd + std::fmt::Debug + Copy,
{
	assert!(
		if a >= b { a - b <= err } else { b - a <= err },
		"assertion failed: (left: `{:?}`, right: `{:?}`, err: `{:?}`)",
		a,
		b,
		err
	);
}
