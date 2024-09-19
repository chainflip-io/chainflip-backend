use cf_traits::mocks::block_height_provider::BlockHeightProvider;
use frame_support::{pallet_prelude::ValueQuery, traits::OnRuntimeUpgrade, weights::Weight};

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

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		// Instead of trying to translate the previous timeout into external chain blocks,
		// we simply reset the remaining timeout duration to the new `BroadcastTimeout` value.
		let new_timeout = BlockHeightProvider::<T::TargetChain>::get_block_height() +
			BroadcastTimeout::<T, I>::get();
		let mut old_keys = Vec::new();
		for (key, timeouts) in old::Timeouts::<T, I>::iter() {
			old_keys.push(key);
			for (broadcast_id, nominee) in timeouts {
				Timeouts::<T, I>::append((new_timeout, broadcast_id, nominee))
			}
		}

		for key in old_keys {
			old::Timeouts::<T, I>::remove(key);
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
			new_timeout_chainblock: BlockHeightProvider::<T::TargetChain>::get_block_height() +
				BroadcastTimeout::<T, I>::get(),
		};
		Ok(data.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let data = MigrationData::<T, I>::decode(&mut &state[..]).unwrap();
		let new_timeouts = Timeouts::<T, I>::get();

		for (broadcast_id, nominee) in data.timeouts {
			assert!(new_timeouts.contains(&(data.new_timeout_chainblock, broadcast_id, nominee)))
		}
		Ok(())
	}
}

#[derive(Encode, Decode)]
pub struct MigrationData<T: Config<I>, I: 'static> {
	pub timeouts: Vec<(BroadcastId, <T as Chainflip>::ValidatorId)>,
	pub new_timeout_chainblock: ChainBlockNumberFor<T, I>,
}

#[cfg(test)]
mod migration_tests {

	#[test]
	fn test_migration() {
		use super::*;
		use crate::mock::*;

		new_test_ext().execute_with(|| {
			let state = super::Migration::<Test, _>::pre_upgrade().unwrap();
			// Perform runtime migration.
			super::Migration::<Test, _>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			super::Migration::<Test, _>::post_upgrade(state).unwrap();
		});
	}
}
