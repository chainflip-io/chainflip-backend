use crate::BidInfo;
use sp_std::cell::RefCell;

thread_local! {
	pub static MIN_BID: RefCell<u128> = RefCell::new(0);
}

pub struct MockBidInfo;

impl BidInfo for MockBidInfo {
	type Balance = u128;
	fn get_min_backup_bid() -> Self::Balance {
		Self::Balance::from(MIN_BID.with(|cell| cell.borrow().clone()))
	}
}

impl MockBidInfo {
	pub fn set_min_bid(bid: u128) {
		MIN_BID.with(|cell| {
			*cell.borrow_mut() = bid;
		})
	}
}
