use std::{cell::RefCell, ops::AddAssign, time::Duration};

pub struct Mock;

const START: Duration = Duration::from_secs(0);

thread_local! {
	pub static FAKE_TIME: RefCell<Duration> = const { RefCell::new(START) };
}

impl Mock {
	pub fn tick(duration: Duration) {
		FAKE_TIME.with(|cell| cell.borrow_mut().add_assign(duration));
	}

	pub fn reset_to(start: Duration) {
		FAKE_TIME.with(|cell| *cell.borrow_mut() = start);
	}
}

impl frame_support::traits::UnixTime for Mock {
	fn now() -> Duration {
		FAKE_TIME.with(|cell| *cell.borrow())
	}
}
