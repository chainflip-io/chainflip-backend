use sp_std::collections::btree_set::BTreeSet;

use crate::{BroadcastNomination, EpochIndex, EpochInfo, ThresholdSignerNomination};

thread_local! {
	pub static THRESHOLD_NOMINEES: std::cell::RefCell<Option<BTreeSet<u64>>> = Default::default();
	pub static LAST_NOMINATED_INDEX: std::cell::RefCell<Option<usize>> = Default::default();
}

pub struct MockNominator;

impl BroadcastNomination for MockNominator {
	type BroadcasterId = u64;

	fn nominate_broadcaster<S>(
		_seed: S,
		_exclude_ids: impl IntoIterator<Item = Self::BroadcasterId>,
	) -> Option<Self::BroadcasterId> {
		let next_nomination_index = LAST_NOMINATED_INDEX.with(|cell| {
			let mut last_nomination = cell.borrow_mut();
			let next_nomination_index =
				if let Some(last_nomination) = *last_nomination { last_nomination + 1 } else { 0 };
			*last_nomination = Some(next_nomination_index);
			next_nomination_index
		});

		Self::get_nominees().unwrap().iter().nth(next_nomination_index).copied()
	}
}

impl ThresholdSignerNomination for MockNominator {
	type SignerId = u64;

	fn threshold_nomination_with_seed<S>(
		_seed: S,
		_epoch_index: EpochIndex,
	) -> Option<BTreeSet<Self::SignerId>> {
		Self::get_nominees()
	}
}

// Remove some threadlocal + refcell complexity from test code
impl MockNominator {
	pub fn get_nominees() -> Option<BTreeSet<u64>> {
		THRESHOLD_NOMINEES.with(|cell| cell.borrow().clone())
	}

	pub fn reset_last_nominee() {
		LAST_NOMINATED_INDEX.with(|cell| *cell.borrow_mut() = None);
	}

	pub fn set_nominees(nominees: Option<BTreeSet<u64>>) {
		THRESHOLD_NOMINEES.with(|cell| *cell.borrow_mut() = nominees);
	}

	pub fn get_last_nominee() -> Option<u64> {
		Self::get_nominees()
			.unwrap()
			.iter()
			.nth(LAST_NOMINATED_INDEX.with(|cell| cell.borrow().expect("No one nominated yet")))
			.copied()
	}

	pub fn use_current_authorities_as_nominees<
		E: EpochInfo<ValidatorId = <Self as BroadcastNomination>::BroadcasterId>,
	>() {
		Self::set_nominees(Some(BTreeSet::from_iter(E::current_authorities())));
	}
}
