use crate::{EpochIndex, EpochInfo, SignerNomination};

thread_local! {
	pub static THRESHOLD_NOMINEES: std::cell::RefCell<Option<Vec<u64>>> = Default::default();
	pub static LAST_NOMINATED_INDEX: std::cell::RefCell<usize> = Default::default();
}

pub struct MockNominator;

impl SignerNomination for MockNominator {
	type SignerId = u64;

	fn nomination_with_seed<S>(
		_seed: S,
		_exclude_ids: &[Self::SignerId],
	) -> Option<Self::SignerId> {
		Self::get_nominees()
			.unwrap()
			.get(LAST_NOMINATED_INDEX.with(|cell| *cell.borrow()))
			.copied()
	}

	fn threshold_nomination_with_seed<S>(
		_seed: S,
		_epoch_index: EpochIndex,
	) -> Option<Vec<Self::SignerId>> {
		Self::get_nominees()
	}
}

// Remove some threadlocal + refcell complexity from test code
impl MockNominator {
	pub fn get_nominees() -> Option<Vec<u64>> {
		THRESHOLD_NOMINEES.with(|cell| cell.borrow().clone())
	}

	pub fn set_nominees(nominees: Option<Vec<u64>>) {
		THRESHOLD_NOMINEES.with(|cell| *cell.borrow_mut() = nominees);
	}

	pub fn use_current_authorities_as_nominees<
		E: EpochInfo<ValidatorId = <Self as SignerNomination>::SignerId>,
	>() {
		Self::set_nominees(Some(E::current_authorities()));
	}

	/// Increments nominee, if it's a Some
	pub fn increment_nominee() {
		LAST_NOMINATED_INDEX.with(|cell| {
			let mut nomination = cell.borrow_mut();
			*nomination += 1;
		});
	}
}
