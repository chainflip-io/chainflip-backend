#[macro_export]
macro_rules! impl_mock_signer_nomination {
	($account_id:ty) => {
		thread_local! {
			pub static CANDIDATES: std::cell::RefCell<Vec<$account_id>> = Default::default();
		}

		use cf_traits::EpochIndex;

		pub struct MockSignerNomination;

		impl MockSignerNomination {
			pub fn set_candidates(candidates: Vec<$account_id>) {
				CANDIDATES.with(|cell| *(cell.borrow_mut()) = candidates)
			}
		}

		impl cf_traits::SignerNomination for MockSignerNomination {
			type SignerId = $account_id;

			fn nomination_with_seed<H: frame_support::Hashable>(
				_seed: H,
			) -> Option<Self::SignerId> {
				CANDIDATES.with(|cell| cell.borrow().iter().next().cloned())
			}

			fn threshold_nomination_with_seed<H: frame_support::Hashable>(
				_seed: H,
				_epoch_index: EpochIndex,
			) -> Option<Vec<Self::SignerId>> {
				Some(CANDIDATES.with(|cell| cell.borrow().clone())).and_then(|v| {
					if v.is_empty() {
						None
					} else {
						Some(v)
					}
				})
			}
		}
	};
}
