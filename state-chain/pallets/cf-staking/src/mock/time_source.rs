use std::cell::RefCell;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct Mock;

thread_local! {
	pub static FAKE_TIME: RefCell<Option<Duration>> = RefCell::new(None);
}

impl Mock {
	pub fn set_fake_time(d: Duration) {
		FAKE_TIME.with(|cell| (*cell.borrow_mut()) = Some(d));
	}

	pub fn reset() {
		FAKE_TIME.with(|cell| (*cell.borrow_mut()) = None);
	}
}

impl frame_support::traits::UnixTime for Mock {
	fn now() -> Duration {
		FAKE_TIME.with(|cell| match *cell.borrow() {
			Some(d) => d,
			None => SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
		})
	}
}