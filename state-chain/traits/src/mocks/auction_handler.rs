use crate::{AuctionHandler, AuctionError};
use std::cell::RefCell;
use std::marker::PhantomData;

thread_local! {
	pub static TO_CONFIRM: RefCell<Result<(), AuctionError>> = RefCell::new(Err(AuctionError::NotConfirmed));
}

pub struct Mock<ValidatorId, Amount> {
	_a: PhantomData<ValidatorId>,
	_b: PhantomData<Amount>,
}
// Helper function to clear the confirmation result
pub fn clear_confirmation() {
	TO_CONFIRM.with(|l| *l.borrow_mut() = Ok(()));
}

impl<ValidatorId, Amount> AuctionHandler<ValidatorId, Amount> for Mock<ValidatorId, Amount> {
	fn on_completed(_winners: Vec<ValidatorId>, _min_bid: Amount) -> Result<(), AuctionError> {
		TO_CONFIRM.with(|l| *l.borrow_mut() = Err(AuctionError::NotConfirmed));
		Ok(())
	}

	fn try_confirmation() -> Result<(), AuctionError> {
		TO_CONFIRM.with(|l| {
			(*l.borrow()).clone()
		})
	}
}