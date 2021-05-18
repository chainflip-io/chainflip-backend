use std::cell::RefCell;
use frame_support::dispatch::Dispatchable;
use super::AccountId;
use super::*;

pub struct Mock;

impl Mock {
	pub fn set_threshold(threshold: u32) {
		WITNESS_THRESHOLD.with(|cell| *(cell.borrow_mut()) = threshold);
	}

	pub fn get_vote_count() -> usize {
		WITNESS_VOTES.with(|cell| cell.borrow().len())
	}
}

thread_local! {
	pub static WITNESS_THRESHOLD: RefCell<u32> = RefCell::new(0);
	pub static WITNESS_VOTES: RefCell<Vec<Call>> = RefCell::new(vec![]);
}

impl cf_traits::Witnesser for Mock {
	type AccountId = AccountId;
	type Call = Call;

	fn witness(_who: Self::AccountId, call: Self::Call) -> frame_support::dispatch::DispatchResultWithPostInfo {
		let count = WITNESS_VOTES.with(|votes| {
			let mut votes = votes.borrow_mut();
			votes.push(call.clone());
			votes.iter().filter(|vote| **vote == call.clone()).count()
		});

		let threshold = WITNESS_THRESHOLD.with(|t| t.borrow().clone());

		if count as u32 == threshold {
			Dispatchable::dispatch(call, Origin::root())
		} else {
			Ok(().into())
		}
	}
}