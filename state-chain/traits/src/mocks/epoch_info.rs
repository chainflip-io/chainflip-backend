pub type Mock = MockEpochInfo;
crate::impl_mock_epoch_info!(u64, u128, u32);

#[macro_export]
macro_rules! impl_mock_epoch_info {
	($account_id:ty, $balance:ty, $epoch_index:ty) => {
		use std::cell::RefCell;
		use $crate::EpochInfo;

		pub struct MockEpochInfo;
		use std::collections::HashMap;

		thread_local! {
			pub static CURRENT_VALIDATORS: RefCell<Vec<$account_id>> = RefCell::new(vec![]);
			pub static VALIDATOR_INDEX: RefCell<HashMap<$epoch_index, HashMap<$account_id, u16>>> = RefCell::new(HashMap::new());
			pub static BOND: RefCell<$balance> = RefCell::new(0);
			pub static EPOCH: RefCell<$epoch_index> = RefCell::new(0);
			pub static LAST_EXPIRED_EPOCH: RefCell<$epoch_index> = RefCell::new(Default::default());
			pub static AUCTION_PHASE: RefCell<bool> = RefCell::new(false);
			pub static EPOCH_VALIDATOR_COUNT: RefCell<HashMap<$epoch_index, u32>> = RefCell::new(Default::default());
		}

		impl MockEpochInfo {

			/// Get the current number of validators.
			pub fn set_validators(validators: Vec<$account_id>) {
				CURRENT_VALIDATORS.with(|cell| {
					*cell.borrow_mut() = validators;
				})
			}

			/// Add a validator to the current validators.
			pub fn add_validator(account: $account_id) {
				CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().push(account))
			}

			/// Set the bond amount.
			pub fn set_bond(bond: $balance) {
				BOND.with(|cell| *(cell.borrow_mut()) = bond);
			}

			/// Set the epoch.
			pub fn set_epoch(epoch: $epoch_index) {
				EPOCH.with(|cell| *(cell.borrow_mut()) = epoch);
			}

			pub fn set_is_auction_phase(is_auction: bool) {
				AUCTION_PHASE.with(|cell| *(cell.borrow_mut()) = is_auction);
			}

			pub fn set_last_expired_epoch(epoch_index: $epoch_index) {
				LAST_EXPIRED_EPOCH.with(|cell| *(cell.borrow_mut()) = epoch_index);
			}

			pub fn set_epoch_validator_count(epoch_index: $epoch_index, count: u32) {
				EPOCH_VALIDATOR_COUNT.with(|cell| {
					cell.borrow_mut().insert(epoch_index, count);
				})
			}

			pub fn set_validator_indices(epoch_index: $epoch_index, account_ids: Vec<$account_id>) {
				VALIDATOR_INDEX.with(|cell| {
					let mut map = cell.borrow_mut();
					let validator_index = map.entry(epoch_index).or_insert(HashMap::new());
					for (i, account_id) in account_ids.iter().enumerate() {
						validator_index.insert(account_id.clone(), i as u16);
					}
				})
			}

			pub fn next_epoch(validators: Vec<$account_id>) -> $epoch_index {
				let new_epoch_index = EPOCH.with(|cell| *(cell.borrow_mut()) + 1);
				MockEpochInfo::set_epoch(new_epoch_index);
				MockEpochInfo::set_epoch_validator_count(new_epoch_index, validators.len() as u32);
				MockEpochInfo::set_validators(validators.to_vec());
				MockEpochInfo::set_validator_indices(new_epoch_index, validators);
				new_epoch_index
			}
		}

		impl EpochInfo for MockEpochInfo {
			type ValidatorId = $account_id;
			type Amount = $balance;

			fn last_expired_epoch() -> $epoch_index {
				LAST_EXPIRED_EPOCH.with(|cell| *cell.borrow())
			}

			fn current_validators() -> Vec<Self::ValidatorId> {
				CURRENT_VALIDATORS.with(|cell| cell.borrow().clone())
			}

			fn current_validator_count() -> u32 {
				CURRENT_VALIDATORS.with(|cell| cell.borrow().len() as u32)
			}

			fn validator_index(
				epoch_index: $epoch_index,
				account: &Self::ValidatorId,
			) -> Option<u16> {
				VALIDATOR_INDEX.with(|cell| {
					let map = cell.borrow();
					map.get(&epoch_index).and_then(|validator_index| validator_index.get(account).cloned())
				})
			}

			fn validator_count_at_epoch(epoch: $epoch_index) -> Option<u32> {
				EPOCH_VALIDATOR_COUNT.with(|cell| {
					cell.borrow().get(&epoch).cloned()
				})
			}

			fn bond() -> Self::Amount {
				BOND.with(|cell| *cell.borrow())
			}

			fn epoch_index() -> $epoch_index {
				EPOCH.with(|cell| *cell.borrow())
			}

			fn is_auction_phase() -> bool {
				AUCTION_PHASE.with(|cell| *cell.borrow())
			}

			#[cfg(feature = "runtime-benchmarks")]
			fn set_validator_index(epoch_index: $epoch_index, account: &Self::ValidatorId, index: u16) {
				VALIDATOR_INDEX.with(|cell| {
					let mut map = cell.borrow_mut();
					let validator_index = map.entry(epoch_index).or_insert(HashMap::new());
					validator_index.insert(account.clone(), index);
				})
			}

			#[cfg(feature = "runtime-benchmarks")]
			fn set_validator_count_for_epoch(epoch: $epoch_index, count: u32) {
				EPOCH_VALIDATOR_COUNT.with(|cell| {
					cell.borrow_mut().insert(epoch, count);
				})
			}
		}
	};
}
