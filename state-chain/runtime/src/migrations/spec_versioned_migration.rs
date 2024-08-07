use sp_std::marker::PhantomData;

use frame_support::traits::OnRuntimeUpgrade;
#[cfg(feature = "try-runtime")]
use sp_std::{vec, vec::Vec};

#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use crate::*;

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
				"⏭️ Skipping storage migration for version {:?} - consider removing this from the runtime.",
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
