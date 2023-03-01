use crate::*;
use cf_traits::EpochInfo;
use sp_runtime::{traits::Zero, AccountId32};
use sp_std::marker::PhantomData;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

#[frame_support::storage_alias]
pub type IncompatibleVoters<T: Config<I>, I: 'static> =
	StorageValue<Pallet<T, I>, Vec<AccountId32>, ValueQuery>;

// Call into each impl for each Chain
impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current_epoch = T::EpochInfo::epoch_index();

		// if we have current vault then we active (since we are also not in rotation)
		let key_state = if Vaults::<T, I>::get(current_epoch).is_some() {
			KeyState::Active
		} else {
			KeyState::Unavailable
		};

		CurrentVaultEpochAndState::<T, I>::put(VaultEpochAndState {
			epoch_index: current_epoch,
			key_state,
		});

		// There should be nothing here anyway, but just in case
		IncompatibleVoters::<T, I>::kill();

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		// NB: No need to migrate PendingVaultRotation despite changes
		// since in order for the migration to run, we must not be in a rotation.
		assert!(PendingVaultRotation::<T, I>::get().is_none());

		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), &'static str> {
		assert!(PendingVaultRotation::<T, I>::get().is_none());

		// Invert what runs in the migration step as a test
		if CurrentVaultEpochAndState::<T, I>::get().key_state == KeyState::Active {
			assert!(Vaults::<T, I>::get(T::EpochInfo::epoch_index()).is_some());
		} else {
			assert!(Vaults::<T, I>::get(T::EpochInfo::epoch_index()).is_none());
		}

		assert!(KeygenResponseTimeout::<T, I>::get() > Zero::zero());

		Ok(())
	}
}
