//! Chainflip runtime storage migrations.
use crate::System;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

mod delete_rewards;
pub use delete_rewards::DeleteRewardsPallet;
mod unify_ceremony_ids;
pub use unify_ceremony_ids::UnifyCeremonyIds;

/// A runtime storage migration that will only be applied if the `SPEC_VERSION` matches the
/// post-upgrade runtime's spec version.
pub struct VersionedMigration<U, const SPEC_VERSION: u32>(PhantomData<U>);

impl<U, const SPEC_VERSION: u32> OnRuntimeUpgrade for VersionedMigration<U, SPEC_VERSION>
where
	U: OnRuntimeUpgrade,
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if System::runtime_version().spec_version == SPEC_VERSION {
			U::on_runtime_upgrade()
		} else {
			log::info!(
				"Skipping storage migration for version {:?} - consider removing this from the runtime.",
				SPEC_VERSION
			);
			0
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		if System::runtime_version().spec_version == SPEC_VERSION {
			U::pre_upgrade()
		} else {
			Ok(())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		if System::runtime_version().spec_version == SPEC_VERSION {
			U::post_upgrade()
		} else {
			Ok(())
		}
	}
}
