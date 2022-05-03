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
			pub static CURRENT_AUTHORITIES: RefCell<Vec<$account_id>> = RefCell::new(vec![]);
			pub static AUTHORITY_INDEX: RefCell<HashMap<$epoch_index, HashMap<$account_id, u16>>> = RefCell::new(HashMap::new());
			pub static BOND: RefCell<$balance> = RefCell::new(0);
			pub static EPOCH: RefCell<$epoch_index> = RefCell::new(0);
			pub static LAST_EXPIRED_EPOCH: RefCell<$epoch_index> = RefCell::new(Default::default());
			pub static AUCTION_PHASE: RefCell<bool> = RefCell::new(false);
			pub static EPOCH_AUTHORITY_COUNT: RefCell<HashMap<$epoch_index, u32>> = RefCell::new(Default::default());
		}

		impl MockEpochInfo {

			/// Set the current authorities.
			pub fn set_authorities(authorities: Vec<$account_id>) {
				CURRENT_AUTHORITIES.with(|cell| {
					*cell.borrow_mut() = authorities;
				})
			}

			/// Add an authority to the current authorities.
			pub fn add_authorities(account: $account_id) {
				CURRENT_AUTHORITIES.with(|cell| cell.borrow_mut().push(account))
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

			pub fn set_epoch_authority_count(epoch_index: $epoch_index, count: u32) {
				EPOCH_AUTHORITY_COUNT.with(|cell| {
					cell.borrow_mut().insert(epoch_index, count);
				})
			}

			pub fn set_authority_indices(epoch_index: $epoch_index, account_ids: Vec<$account_id>) {
				AUTHORITY_INDEX.with(|cell| {
					let mut map = cell.borrow_mut();
					let authority_index = map.entry(epoch_index).or_insert(HashMap::new());
					for (i, account_id) in account_ids.iter().enumerate() {
						authority_index.insert(account_id.clone(), i as u16);
					}
				})
			}

			pub fn next_epoch(authorities: Vec<$account_id>) -> $epoch_index {
				let new_epoch_index = EPOCH.with(|cell| *(cell.borrow_mut()) + 1);
				MockEpochInfo::set_epoch(new_epoch_index);
				MockEpochInfo::set_authorities(authorities.clone());
				MockEpochInfo::inner_add_authority_info_for_epoch(new_epoch_index, authorities);
				new_epoch_index
			}

			pub fn inner_add_authority_info_for_epoch(epoch_index: $epoch_index, new_authorities: Vec<$account_id>) {
				MockEpochInfo::set_epoch_authority_count(epoch_index, new_authorities.len() as u32);
				MockEpochInfo::set_authority_indices(epoch_index, new_authorities);
			}
		}

		impl EpochInfo for MockEpochInfo {
			type ValidatorId = $account_id;
			type Amount = $balance;

			fn last_expired_epoch() -> $epoch_index {
				LAST_EXPIRED_EPOCH.with(|cell| *cell.borrow())
			}

			fn current_authorities() -> Vec<Self::ValidatorId> {
				CURRENT_AUTHORITIES.with(|cell| cell.borrow().clone())
			}

			fn current_authority_count() -> u32 {
				CURRENT_AUTHORITIES.with(|cell| cell.borrow().len() as u32)
			}

			fn authority_index(
				epoch_index: $epoch_index,
				account: &Self::ValidatorId,
			) -> Option<u16> {
				AUTHORITY_INDEX.with(|cell| {
					let map = cell.borrow();
					map.get(&epoch_index).and_then(|authority_index| authority_index.get(account).cloned())
				})
			}

			fn authority_count_at_epoch(epoch: $epoch_index) -> Option<u32> {
				EPOCH_AUTHORITY_COUNT.with(|cell| {
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
			fn add_authority_info_for_epoch(epoch_index: $epoch_index, new_authorities: Vec<Self::ValidatorId>) {
				MockEpochInfo::inner_add_authority_info_for_epoch(epoch_index, new_authorities);
			}
		}
	};
}
