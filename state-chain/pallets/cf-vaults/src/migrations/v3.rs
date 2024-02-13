use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
mod try_runtime_includes {
	pub use codec::{Decode, DecodeLength, Encode};
	pub use frame_support::{ensure, pallet_prelude::DispatchError};
	pub use sp_std::prelude::*;
}
#[cfg(feature = "try-runtime")]
use try_runtime_includes::*;

use crate::{PendingVaultActivation, VaultActivationStatus, VaultStartBlockNumbers};

mod old {
	use cf_chains::{Chain, ChainCrypto};
	use cf_primitives::EpochIndex;
	use codec::{Decode, Encode};
	use frame_support::{storage_alias, Blake2_128Concat};

	use crate::{Config, Pallet};

	/// A single vault.
	#[derive(Default, PartialEq, Eq, Clone, Encode, Decode)]
	pub struct Vault<T: Chain> {
		/// The vault's public key.
		pub public_key: <<T as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		/// The first active block for this vault
		pub active_from_block: T::ChainBlockNumber,
	}

	#[storage_alias]
	pub type Vaults<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Blake2_128Concat, EpochIndex, Vault<<T as Config<I>>::Chain>>;
}

mod new {
	use cf_chains::{Chain, ChainCrypto};
	use cf_primitives::EpochIndex;
	use frame_support::{storage_alias, Blake2_128Concat};

	use crate::{Config, Pallet};

	// Temporary - should be erased after the threshols signature part of the migration.
	#[storage_alias]
	pub type Vaults<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Blake2_128Concat,
		EpochIndex,
		<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	>;
}

/// The V3 migration is partly implemented in the runtime/lib.rs
/// `ThresholdSignatureRefactorMigration` struct.
pub struct Migration<T, I>(PhantomData<(T, I)>);

impl<T: crate::Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		PendingVaultActivation::<T, I>::put(VaultActivationStatus::Complete);
		// We don't drain the old storage, it's required for the other part of the migration
		// (threhsold signer pallet).

		for (epoch_index, old::Vault { active_from_block, .. }) in old::Vaults::<T, I>::iter() {
			VaultStartBlockNumbers::<T, I>::insert(epoch_index, active_from_block);
		}
		new::Vaults::<T, I>::translate::<old::Vault<T::Chain>, _>(|_, old_vault| {
			Some(old_vault.public_key)
		});

		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
