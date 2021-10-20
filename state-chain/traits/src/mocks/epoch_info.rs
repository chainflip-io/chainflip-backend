pub type Mock = MockEpochInfo;
crate::impl_mock_epoch_info!(u64, u128, u32);

#[macro_export]
macro_rules! impl_mock_epoch_info {
	($account_id:ty, $balance:ty, $epoch_index:ty) => {
		use std::cell::RefCell;
		use $crate::EpochInfo;

		pub struct MockEpochInfo;

		thread_local! {
			pub static CURRENT_VALIDATORS: RefCell<Vec<$account_id>> = RefCell::new(vec![]);
			pub static NEXT_VALIDATORS: RefCell<Vec<$account_id>> = RefCell::new(vec![]);
			pub static BOND: RefCell<$balance> = RefCell::new(0);
			pub static EPOCH: RefCell<$epoch_index> = RefCell::new(0);
			pub static IS_AUCTION: RefCell<bool> = RefCell::new(false);
		}

		impl MockEpochInfo {
			/// Get the current number of validators.
			pub fn validator_count() -> usize {
				CURRENT_VALIDATORS.with(|cell| cell.borrow().len())
			}

			/// Add a validator to the current validators.
			pub fn add_validator(account: $account_id) {
				CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().push(account))
			}

			/// Queue a validator. Adds the validator to the set of next validators.
			pub fn queue_validator(account: $account_id) {
				NEXT_VALIDATORS.with(|cell| cell.borrow_mut().push(account))
			}

			/// Clears the current and next validators.
			pub fn clear_validators() {
				CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().clear());
				NEXT_VALIDATORS.with(|cell| cell.borrow_mut().clear());
			}

			/// Set the bond amount.
			pub fn set_bond(bond: $balance) {
				BOND.with(|cell| *(cell.borrow_mut()) = bond);
			}

			/// Set the epoch.
			pub fn set_epoch(epoch: $epoch_index) {
				EPOCH.with(|cell| *(cell.borrow_mut()) = epoch);
			}

			/// Increment the epoch.
			pub fn incr_epoch() {
				EPOCH.with(|cell| *(cell.borrow_mut()) += 1);
			}

			pub fn set_is_auction_phase(is_auction: bool) {
				IS_AUCTION.with(|cell| *(cell.borrow_mut()) = is_auction);
			}
		}

		impl EpochInfo for MockEpochInfo {
			type ValidatorId = $account_id;
			type Amount = $balance;

			fn current_validators() -> Vec<Self::ValidatorId> {
				CURRENT_VALIDATORS.with(|cell| cell.borrow().clone())
			}

			fn is_validator(account: &Self::ValidatorId) -> bool {
				Self::current_validators().as_slice().contains(account)
			}

			fn bond() -> Self::Amount {
				BOND.with(|cell| *cell.borrow())
			}

			fn next_validators() -> Vec<Self::ValidatorId> {
				NEXT_VALIDATORS.with(|cell| cell.borrow().clone())
			}

			fn epoch_index() -> $epoch_index {
				EPOCH.with(|cell| *cell.borrow())
			}

			fn is_auction_phase() -> bool {
				IS_AUCTION.with(|cell| *cell.borrow())
			}
		}
	};
}
