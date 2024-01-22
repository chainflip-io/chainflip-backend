use crate::*;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

/// v2 migrating storage item KeygenSlashRate -> KeygenSlashAmount
/// Percent -> FlipBalance
pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

mod old {

	use super::*;

	#[derive(Debug, TypeInfo, Decode, Encode, Clone, Copy, PartialEq, Eq)]
	pub enum KeyState {
		Unlocked,
		/// Key is only available to sign this request id.
		Locked(ThresholdSignatureRequestId),
	}
	#[derive(Encode, Decode, TypeInfo)]
	pub struct VaultEpochAndState {
		pub epoch_index: EpochIndex,
		pub key_state: KeyState,
	}

	#[frame_support::storage_alias]
	pub type CurrentVaultEpochAndState<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, VaultEpochAndState>;
}

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// remove the old storage containing Percent
		if let Some(old::VaultEpochAndState { epoch_index, key_state: _ }) =
			old::CurrentVaultEpochAndState::<T, I>::take()
		{
			// set the new storage using the EpochIndex from the old storage item above.
			CurrentVaultEpoch::<T, I>::put(epoch_index);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let state = old::CurrentVaultEpochAndState::<T, I>::get().unwrap();

		Ok(state.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_state = <old::VaultEpochAndState>::decode(&mut &state[..])
			.map_err(|_| "Failed to decode pre-upgrade state.")?;

		assert!(!old::CurrentVaultEpochAndState::<T, I>::exists());
		assert!(CurrentVaultEpoch::<T, I>::exists());
		assert_eq!(old_state.epoch_index, CurrentVaultEpoch::<T, I>::get().unwrap());

		Ok(())
	}
}
