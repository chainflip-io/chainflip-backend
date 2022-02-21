crate::impl_mock_keygen_exclusion!(u64);

#[macro_export]
macro_rules! impl_mock_keygen_exclusion {
	($validator_id:ty) => {
		use $crate::KeygenExclusionSet;

		pub struct MockKeygenExclusion;

		thread_local! {
			pub static EXCLUDED: std::cell::RefCell<std::collections::HashMap<$validator_id, ()>> = Default::default();
		}

		impl KeygenExclusionSet for MockKeygenExclusion {
			type ValidatorId = $validator_id;

			fn add_to_set(validator_id: Self::ValidatorId) {
				EXCLUDED.with(|cell| {
					(*cell.borrow_mut()).insert(validator_id, ());
				});
			}

			fn is_excluded(validator_id: &Self::ValidatorId) -> bool {
				EXCLUDED.with(|cell| {
					(*cell.borrow()).contains_key(validator_id)
				})
			}

			fn forgive_all() {
				EXCLUDED.with(|cell| {
					*cell.borrow_mut() = Default::default();
				});
			}
		}
	};
}
