#[macro_export]
macro_rules! impl_mock_epoch_info {
	($account_id:ty, $balance:ty, $epoch_index:ty, $authority_count:ty $(,)? ) => {
		use $crate::EpochInfo;
		pub struct MockEpochInfo;

		thread_local! {
			pub static CURRENT_AUTHORITIES: std::cell::RefCell<sp_std::collections::btree_set::BTreeSet<$account_id>> = std::cell::RefCell::new(Default::default());
			pub static PAST_AUTHORITIES: std::cell::RefCell<sp_std::collections::btree_set::BTreeSet<$account_id>> = std::cell::RefCell::new(Default::default());
			pub static AUTHORITY_INDEX: std::cell::RefCell<std::collections::HashMap<$epoch_index, std::collections::HashMap<$account_id, $authority_count>>> = std::cell::RefCell::new(std::collections::HashMap::new());
			pub static BOND: std::cell::RefCell<$balance> = std::cell::RefCell::new(0);
			pub static EPOCH: std::cell::RefCell<$epoch_index> = std::cell::RefCell::new(0);
			pub static LAST_EXPIRED_EPOCH: std::cell::RefCell<$epoch_index> = std::cell::RefCell::new(Default::default());
			pub static AUCTION_PHASE: std::cell::RefCell<bool> = std::cell::RefCell::new(false);
			pub static EPOCH_AUTHORITY_COUNT: std::cell::RefCell<std::collections::HashMap<$epoch_index, $authority_count>> = std::cell::RefCell::new(Default::default());
		}

		impl MockEpochInfo {

			/// Set the current authorities.
			pub fn set_authorities(authorities: sp_std::collections::btree_set::BTreeSet<$account_id>) {
				CURRENT_AUTHORITIES.with(|cell| {
					*cell.borrow_mut() = authorities;
				})
			}

			pub fn set_past_authorities(authorities: sp_std::collections::btree_set::BTreeSet<$account_id>) {
				PAST_AUTHORITIES.with(|cell| {
					*cell.borrow_mut() = authorities;
				})
			}

			/// Add an authority to the current authorities.
			pub fn add_authorities(account: $account_id) {
				CURRENT_AUTHORITIES.with(|cell| cell.borrow_mut().insert(account));
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

			pub fn set_epoch_authority_count(epoch_index: $epoch_index, count: $authority_count) {
				EPOCH_AUTHORITY_COUNT.with(|cell| {
					cell.borrow_mut().insert(epoch_index, count);
				})
			}

			pub fn set_authority_indices(epoch_index: $epoch_index, account_ids: sp_std::collections::btree_set::BTreeSet<$account_id>) {
				AUTHORITY_INDEX.with(|cell| {
					let mut map = cell.borrow_mut();
					let authority_index = map.entry(epoch_index).or_insert(std::collections::HashMap::new());
					for (i, account_id) in account_ids.iter().enumerate() {
						authority_index.insert(account_id.clone(), i as $authority_count);
					}
				})
			}

			pub fn next_epoch(authorities: sp_std::collections::btree_set::BTreeSet<$account_id>) -> $epoch_index {
				let new_epoch_index = EPOCH.with(|cell| *(cell.borrow_mut()) + 1);
				MockEpochInfo::set_epoch(new_epoch_index);
				MockEpochInfo::set_authorities(authorities.clone());
				MockEpochInfo::inner_add_authority_info_for_epoch(new_epoch_index, authorities);
				new_epoch_index
			}

			pub fn inner_add_authority_info_for_epoch(epoch_index: $epoch_index, new_authorities: sp_std::collections::btree_set::BTreeSet<$account_id>) {
				MockEpochInfo::set_epoch_authority_count(epoch_index, new_authorities.len() as $authority_count);
				MockEpochInfo::set_authority_indices(epoch_index, new_authorities);
			}
		}

		impl EpochInfo for MockEpochInfo {
			type ValidatorId = $account_id;
			type Amount = $balance;

			fn last_expired_epoch() -> $epoch_index {
				LAST_EXPIRED_EPOCH.with(|cell| *cell.borrow())
			}

			fn current_authorities() -> sp_std::collections::btree_set::BTreeSet<Self::ValidatorId> {
				CURRENT_AUTHORITIES.with(|cell| cell.borrow().clone())
			}

			fn current_authority_count() -> $authority_count {
				CURRENT_AUTHORITIES.with(|cell| cell.borrow().len() as $authority_count)
			}

			fn authority_index(
				epoch_index: $epoch_index,
				account: &Self::ValidatorId,
			) -> Option<$authority_count> {
				AUTHORITY_INDEX.with(|cell| {
					let map = cell.borrow();
					map.get(&epoch_index).and_then(|authority_index| authority_index.get(account).cloned())
				})
			}

			fn authority_count_at_epoch(epoch: $epoch_index) -> Option<$authority_count> {
				EPOCH_AUTHORITY_COUNT.with(|cell| {
					cell.borrow().get(&epoch).cloned()
				})
			}

			fn authorities_at_epoch(epoch: $epoch_index) -> sp_std::collections::btree_set::BTreeSet<Self::ValidatorId> {
				if epoch == Self::epoch_index() {
					CURRENT_AUTHORITIES.with(|cell| cell.borrow().clone())
				} else {
					PAST_AUTHORITIES.with(|cell| cell.borrow().clone())
				}
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
			fn add_authority_info_for_epoch(epoch_index: $epoch_index, new_authorities: sp_std::collections::btree_set::BTreeSet<Self::ValidatorId>) {
				MockEpochInfo::inner_add_authority_info_for_epoch(epoch_index, new_authorities);
			}

			#[cfg(feature = "runtime-benchmarks")]
			fn set_authorities(
				authorities: sp_std::collections::btree_set::BTreeSet<Self::ValidatorId>,
			) {
				Self::set_authorities(authorities);
			}
		}
	};
}
