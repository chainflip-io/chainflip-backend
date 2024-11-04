use frame_support::{
	pallet_prelude::ValueQuery, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};

use crate::*;

mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type Timeouts<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		BlockNumberFor<T>,
		BTreeSet<(BroadcastId, <T as Chainflip>::ValidatorId)>,
		ValueQuery,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		// Instead of trying to translate the previous timeout into external chain blocks,
		// we simply reset the remaining timeout duration to the new `BroadcastTimeout` value.
		let new_timeout = T::ChainTracking::get_block_height() + BroadcastTimeout::<T, I>::get();
		for (_, timeouts) in old::Timeouts::<T, I>::drain() {
			for (broadcast_id, nominee) in timeouts {
				Timeouts::<T, I>::append((new_timeout, broadcast_id, nominee))
			}
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let mut timeouts = Vec::new();
		for (_, old_broadcast_ids) in old::Timeouts::<T, I>::iter() {
			for (old_broadcast_id, old_nominee) in old_broadcast_ids {
				timeouts.push((old_broadcast_id, old_nominee))
			}
		}
		let data: MigrationData<T, I> = MigrationData {
			timeouts,
			target_chainblock: T::ChainTracking::get_block_height() +
				BroadcastTimeout::<T, I>::get(),
		};
		Ok(data.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let data = MigrationData::<T, I>::decode(&mut &state[..]).unwrap();
		let new_timeouts = Timeouts::<T, I>::get();

		// We don't know whether the timeout is set to exactly the `new_timeout` value or a higher
		// one, because between getting the current block height in `pre_upgrade` and in
		// `on_runtime_upgrade` some time might have passed.
		for (broadcast_id, nominee) in data.timeouts {
			let (new_timeout, _, _) = new_timeouts
				.iter()
				.find(|(_, id, nom)| (id, nom) == (&broadcast_id, &nominee))
				.unwrap();
			assert!(*new_timeout >= data.target_chainblock);
		}

		// Make sure that the old map is empty
		assert!(old::Timeouts::<T, I>::iter().next().is_none());

		Ok(())
	}
}

#[derive(Encode, Decode)]
pub struct MigrationData<T: Config<I>, I: 'static> {
	pub timeouts: Vec<(BroadcastId, <T as Chainflip>::ValidatorId)>,
	pub target_chainblock: ChainBlockNumberFor<T, I>,
}

#[cfg(test)]
mod migration_tests {
	#[test]
	fn test_migration() {
		use super::*;
		use crate::mock::*;

		new_test_ext().execute_with(|| {
			let target = frame_system::Pallet::<Test>::block_number() +
				BroadcastTimeout::<Test, Instance1>::get();

			// Create a few timeouts to migrate
			old::Timeouts::<Test, _>::set(target, BTreeSet::from([(0, 100), (1, 101), (3, 102)]));
			old::Timeouts::<Test, _>::set(target + 1, BTreeSet::from([(4, 103), (5, 104)]));

			#[cfg(feature = "try-runtime")]
			let state = super::Migration::<Test, Instance1>::pre_upgrade().unwrap();

			// increment block height
			let new_height = <Test as Config<Instance1>>::ChainTracking::get_block_height() + 20;
			<Test as Config<Instance1>>::ChainTracking::set_block_height(new_height);

			// Perform runtime migration.
			super::Migration::<Test, Instance1>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			super::Migration::<Test, Instance1>::post_upgrade(state).unwrap();
		});
	}
}
