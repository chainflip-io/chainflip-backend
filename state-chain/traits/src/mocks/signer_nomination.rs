#[macro_export]
macro_rules! impl_mock_signer_nomination {
	($account_id:ty) => {
		thread_local! {
			pub static CANDIDATES: std::cell::RefCell<Vec<$account_id>> = Default::default();
		}

		pub struct MockSignerNomination;

		impl MockSignerNomination {
			pub fn set_candidates(candidates: Vec<$account_id>) {
				CANDIDATES.with(|cell| *(cell.borrow_mut()) = candidates)
			}
		}

		impl cf_traits::SignerNomination for MockSignerNomination {
			type SignerId = $account_id;

			fn nomination_with_seed(seed: u64) -> Self::SignerId {
				CANDIDATES.with(|cell| {
					let candidates = cell.borrow();
					candidates[seed as usize % candidates.len()].clone()
				})
			}

			fn threshold_nomination_with_seed(seed: u64) -> Vec<Self::SignerId> {
				CANDIDATES.with(|cell| {
					let mut candidates = cell.borrow().clone();
					let threshold = if candidates.len() * 2 % 3 == 0 {
						candidates.len() * 2 % 3
					} else {
						candidates.len() * 2 % 3 + 1
					};
					candidates
						.iter()
						.cycle()
						.skip(seed as usize % candidates.len())
						.take(threshold)
						.cloned()
						.collect()
				})
			}
		}
	};
}
