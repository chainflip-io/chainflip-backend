#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::{
	pallet_prelude::GetStorageVersion,
	traits::{OnRuntimeUpgrade, PalletInfoAccess, StorageVersion},
	weights::RuntimeDbWeight,
};
use sp_std::marker::PhantomData;

/// A Runtime upgrade for a pallet that migrates the pallet from version `FROM` to verion `TO`.
///
/// In order for the runtime upgrade `U` to proceed, two conditions should be satisfied:
///   1. `P`'s stored version should be equal to `FROM`.
///   2.  The version supported by the pallet is greater or equal to `TO`.
///
/// As long as both conditions are met, the upgrade `U` will run and then the pallet's stored
/// version is set to `TO`.
pub struct VersionedMigration<
	P: PalletInfoAccess + GetStorageVersion,
	U: OnRuntimeUpgrade,
	const FROM: u16,
	const TO: u16,
>(PhantomData<(P, U)>);

#[cfg(feature = "try-runtime")]
mod try_runtime_helpers {
	use frame_support::{traits::PalletInfoAccess, Twox64Concat};

	frame_support::generate_storage_alias!(
		RuntimeUpgradeUtils, ExpectMigration => Map<(Vec<u8>, Twox64Concat), bool>
	);

	pub fn expect_migration<T: PalletInfoAccess>() {
		ExpectMigration::insert(T::name().as_bytes(), true);
	}

	pub fn migration_expected<T: PalletInfoAccess>() -> bool {
		ExpectMigration::get(T::name().as_bytes()).unwrap_or_default()
	}
}

impl<P, U, const FROM: u16, const TO: u16> OnRuntimeUpgrade for VersionedMigration<P, U, FROM, TO>
where
	P: PalletInfoAccess + GetStorageVersion,
	U: OnRuntimeUpgrade,
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if <P as GetStorageVersion>::on_chain_storage_version() == FROM &&
			<P as GetStorageVersion>::current_storage_version() >= TO
		{
			log::info!(
				"✅ {:?}: Applying storage migration from version {:?} to {:?}.",
				P::name(),
				FROM,
				TO
			);
			let w = U::on_runtime_upgrade();
			StorageVersion::new(TO).put::<P>();
			w + RuntimeDbWeight::default().reads_writes(1, 1)
		} else {
			log::info!(
				"⏭ {:?}: Skipping storage migration from version {:?} to {:?} - consider removing this from the pallet.",
				P::name(),
				FROM,
				TO
			);
			RuntimeDbWeight::default().reads(1)
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		if <P as GetStorageVersion>::on_chain_storage_version() == FROM {
			try_runtime_helpers::expect_migration::<P>();
			U::pre_upgrade()
		} else {
			Ok(())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		if !try_runtime_helpers::migration_expected::<P>() {
			return Ok(())
		}

		if <P as GetStorageVersion>::on_chain_storage_version() == TO {
			U::post_upgrade()
		} else {
			log::error!("Expected post-upgrade storage version {:?}, found {:?}.", FROM, TO);
			Err("Pallet storage migration version mismatch.")
		}
	}
}
