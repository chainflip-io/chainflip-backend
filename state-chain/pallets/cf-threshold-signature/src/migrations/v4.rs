use cf_traits::{Chainflip, EpochInfo};
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
mod try_runtime_includes {
	pub use frame_support::{ensure, pallet_prelude::DispatchError};
	pub use sp_std::prelude::*;
}
#[cfg(feature = "try-runtime")]
use try_runtime_includes::*;

use crate::{CurrentKeyEpoch, KeyRotationStatus, PendingKeyRotation};

mod old {
	use cf_primitives::EpochIndex;
	use frame_support::{storage_alias, Blake2_128Concat};

	use crate::{AggKeyFor, Config, Pallet};

	#[storage_alias]
	pub type Vaults<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Blake2_128Concat, EpochIndex, AggKeyFor<T, I>>;
}

/// The V4 migration is partly implemented in the runtime/lib.rs
/// `ThresholdSignatureRefactorMigration` struct.
pub struct Migration<T, I>(PhantomData<(T, I)>);

impl<T: crate::Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("Running V4 threshold migration");
		for (k, v) in old::Vaults::<T, I>::drain() {
			crate::Keys::<T, I>::insert(k, v);
		}
		CurrentKeyEpoch::<T, I>::put(<T as Chainflip>::EpochInfo::epoch_index());
		// Assume we don't migrate during a rotation.
		PendingKeyRotation::<T, I>::put(KeyRotationStatus::Complete);
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		// The old vault should have been moved here from the vaults pallet.
		ensure!(old::Vaults::<T, I>::iter().count() > 0, "Vaults should exist!");
		ensure!(
			old::Vaults::<T, I>::contains_key(<T as Chainflip>::EpochInfo::epoch_index()),
			"Current Epoch Vault should exist!"
		);
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		// NOTE Most of the below migrations are run by the vaults pallet or in the
		// ThresholdSignatureRefactorMigration in the runtime.
		ensure!(crate::CeremonyIdCounter::<T, I>::exists(), "CeremonyIdCounter was not migrated!");
		ensure!(
			crate::KeyHandoverFailureVoters::<T, I>::decode_len().is_none(),
			"KeyHandoverFailureVoters should be empty!"
		);
		ensure!(
			crate::KeyHandoverSuccessVoters::<T, I>::iter().count() == 0,
			"KeyHandoverSuccessVoters should be empty!"
		);
		ensure!(
			crate::KeygenFailureVoters::<T, I>::decode_len().is_none(),
			"KeygenFailureVoters should be empty!"
		);
		ensure!(
			crate::KeygenSuccessVoters::<T, I>::iter().count() == 0,
			"KeygenSuccessVoters should be empty!"
		);
		ensure!(
			!crate::KeygenResolutionPendingSince::<T, I>::exists(),
			"KeygenResolutionPendingSince should be empty!"
		);
		ensure!(
			crate::KeygenResponseTimeout::<T, I>::get() > 0u32.into(),
			"KeygenResponseTimeout should be set!"
		);
		ensure!(crate::KeygenSlashAmount::<T, I>::get() > 0, "KeygenSlashAmount should be set!");
		ensure!(old::Vaults::<T, I>::iter().count() == 0, "Vaults should be deleted!");
		ensure!(
			!old::Vaults::<T, I>::contains_key(<T as Chainflip>::EpochInfo::epoch_index()),
			"Current Epoch Vault should not exist!"
		);
		ensure!(
			CurrentKeyEpoch::<T, I>::get().unwrap() == <T as Chainflip>::EpochInfo::epoch_index(),
			"CurrentKeyEpoch was not migrated!"
		);
		ensure!(
			crate::Keys::<T, I>::contains_key(<T as Chainflip>::EpochInfo::epoch_index()),
			"Keys were not migrated!"
		);
		Ok(())
	}
}
