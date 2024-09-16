use crate::FetchesTransfersLimitProvider;
use sp_std::cell::RefCell;

thread_local! {
	pub static USE_LIMITS: RefCell<bool> = RefCell::new(false);
}

pub struct MockFetchesTransfersLimitProvider;

impl FetchesTransfersLimitProvider for MockFetchesTransfersLimitProvider {
	fn maybe_transfers_limit() -> Option<usize> {
		if USE_LIMITS.with(|v| *v.borrow()) {
			Some(20)
		} else {
			None
		}
	}

	fn maybe_ccm_limit() -> Option<usize> {
		if USE_LIMITS.with(|v| *v.borrow()) {
			Some(5)
		} else {
			None
		}
	}

	fn maybe_fetches_limit() -> Option<usize> {
		if USE_LIMITS.with(|v| *v.borrow()) {
			Some(20)
		} else {
			None
		}
	}
}

impl MockFetchesTransfersLimitProvider {
	pub fn enable_limits() {
		USE_LIMITS.with(|v| *v.borrow_mut() = true);
	}
}
