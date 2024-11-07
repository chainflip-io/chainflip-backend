//! Chainflip runtime storage migrations.

pub mod add_liveness_electoral_system_solana;
pub mod housekeeping;
pub mod reap_old_accounts;
pub mod serialize_solana_broadcast;
pub mod solana_egress_success_witness;

#[cfg(feature = "try-runtime")]
use sp_std::{vec, vec::Vec};

#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use crate::System;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::marker::PhantomData;

/// A runtime storage migration that will only be applied if the `SPEC_VERSION` matches the
/// post-upgrade runtime's spec version.
pub struct SpecVersionedMigration<U, const SPEC_VERSION: u32>(PhantomData<U>);

impl<U, const SPEC_VERSION: u32> OnRuntimeUpgrade for SpecVersionedMigration<U, SPEC_VERSION>
where
	U: OnRuntimeUpgrade,
{
	fn on_runtime_upgrade() -> Weight {
		if System::runtime_version().spec_version == SPEC_VERSION {
			U::on_runtime_upgrade()
		} else {
			log::info!(
				"⏭️ Skipping storage migration since spec version is {:?} - consider removing this from the runtime.",
				SPEC_VERSION
			);
			Weight::zero()
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		if System::runtime_version().spec_version == SPEC_VERSION {
			U::pre_upgrade()
		} else {
			Ok(vec![])
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		if System::runtime_version().spec_version == SPEC_VERSION {
			U::post_upgrade(state)
		} else {
			Ok(())
		}
	}
}
