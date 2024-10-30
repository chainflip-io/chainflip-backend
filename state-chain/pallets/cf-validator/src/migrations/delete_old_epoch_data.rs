use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

use crate::*;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		pub fn delete_all_old<Iter, Index, Item, Relevant, IterRes, Remove>(
			iter: Iter,
			remove: Remove,
			relevant: Relevant,
		) where
			Iter: Fn() -> IterRes,
			IterRes: Iterator<Item = Item>,
			Relevant: Fn(Item) -> Option<Index>,
			Remove: Fn(Index),
		{
			let mut old_indices = Vec::new();
			for item in iter() {
				if let Some(index) = relevant(item) {
					old_indices.push(index);
				}
			}
			for index in old_indices {
				remove(index);
			}
		}

		let epoch = LastExpiredEpoch::<T>::get();

		delete_all_old(
			HistoricalAuthorities::<T>::iter,
			HistoricalAuthorities::<T>::remove,
			|(e, _)| if e <= epoch { Some(e) } else { None },
		);
		delete_all_old(HistoricalBonds::<T>::iter, HistoricalBonds::<T>::remove, |(e, _)| {
			if e <= epoch {
				Some(e)
			} else {
				None
			}
		});
		delete_all_old(
			AuthorityIndex::<T>::iter,
			|(e1, e2)| AuthorityIndex::<T>::remove(e1, e2),
			|(e, e2, _)| if e <= epoch { Some((e, e2)) } else { None },
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let epoch = LastExpiredEpoch::<T>::get();

		assert!(!HistoricalAuthorities::<T>::iter().any(|(e, _)| e <= epoch));
		assert!(!HistoricalBonds::<T>::iter().any(|(e, _)| e <= epoch));
		assert!(!AuthorityIndex::<T>::iter().any(|(e, _, _)| e <= epoch));

		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	#[test]
	fn test_migration() {
		use super::*;
		use crate::mock::*;

		new_test_ext().execute_with(|| {
			let last_expired_epoch = 1000;
			LastExpiredEpoch::<Test>::set(last_expired_epoch);

			// create some test values
			HistoricalAuthorities::<Test>::set(last_expired_epoch - 2, vec![1, 2, 3]);
			HistoricalAuthorities::<Test>::set(last_expired_epoch - 1, vec![4, 5]);
			HistoricalAuthorities::<Test>::set(last_expired_epoch, vec![6, 7, 8, 9]);
			HistoricalAuthorities::<Test>::set(last_expired_epoch + 1, vec![10, 11]);

			HistoricalBonds::<Test>::set(last_expired_epoch - 2, 100);
			HistoricalBonds::<Test>::set(last_expired_epoch - 1, 101);
			HistoricalBonds::<Test>::set(last_expired_epoch, 102);
			HistoricalBonds::<Test>::set(last_expired_epoch + 1, 103);

			AuthorityIndex::<Test>::set(last_expired_epoch - 2, 1, Some(1));
			AuthorityIndex::<Test>::set(last_expired_epoch - 2, 2, Some(2));
			AuthorityIndex::<Test>::set(last_expired_epoch - 2, 3, Some(3));
			AuthorityIndex::<Test>::set(last_expired_epoch - 1, 1, Some(1));
			AuthorityIndex::<Test>::set(last_expired_epoch - 1, 2, Some(2));
			AuthorityIndex::<Test>::set(last_expired_epoch, 3, Some(1));
			AuthorityIndex::<Test>::set(last_expired_epoch, 1, Some(2));
			AuthorityIndex::<Test>::set(last_expired_epoch + 1, 2, Some(1));
			AuthorityIndex::<Test>::set(last_expired_epoch + 2, 3, Some(2));

			#[cfg(feature = "try-runtime")]
			let state = super::Migration::<Test>::pre_upgrade().unwrap();

			// Perform runtime migration.
			super::Migration::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			super::Migration::<Test>::post_upgrade(state).unwrap();

			// ensure that data which is not expired is kept
			assert_eq!(HistoricalAuthorities::<Test>::get(last_expired_epoch + 1), vec![10, 11]);
			assert_eq!(HistoricalBonds::<Test>::get(last_expired_epoch + 1), 103);
			assert_eq!(AuthorityIndex::<Test>::get(last_expired_epoch + 1, 2), Some(1));
			assert_eq!(AuthorityIndex::<Test>::get(last_expired_epoch + 2, 3), Some(2));
		});
	}
}
