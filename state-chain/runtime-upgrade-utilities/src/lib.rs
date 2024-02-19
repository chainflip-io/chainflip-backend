#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::{
	pallet_prelude::{Decode, Encode, GetStorageVersion},
	traits::{OnRuntimeUpgrade, PalletInfoAccess, StorageVersion},
	weights::RuntimeDbWeight,
};
use sp_std::marker::PhantomData;

mod helper_functions;
pub use helper_functions::*;

pub mod migration_template;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// A Runtime upgrade for a pallet that migrates the pallet from version `FROM` to version `TO`.
///
/// In order for the runtime upgrade `U` to proceed, two conditions should be satisfied:
///   1. `P`'s stored version should be equal to `FROM`.
///   2. The version supported by the pallet is greater or equal to `TO`.
///
/// As long as both conditions are met, the upgrade `U` will run and then the pallet's stored
/// version is set to `TO`.
pub struct VersionedMigration<
	P: PalletInfoAccess + GetStorageVersion<CurrentStorageVersion = StorageVersion>,
	U: OnRuntimeUpgrade,
	const FROM: u16,
	const TO: u16,
>(PhantomData<(P, U)>);

/// A helper enum to wrap the pre_upgrade bytes like an Option before passing them to post_upgrade.
/// This enum is used rather than an Option to make the API clearer to the developer.
#[derive(Encode, Decode)]
pub enum VersionedPostUpgradeData {
	/// The migration ran, inner vec contains pre_upgrade data.
	MigrationExecuted(sp_std::vec::Vec<u8>),
	/// This migration is a noop, do not run post_upgrade checks.
	Noop,
}

fn should_upgrade<
	P: GetStorageVersion<CurrentStorageVersion = StorageVersion>,
	const FROM: u16,
	const TO: u16,
>() -> bool {
	<P as GetStorageVersion>::on_chain_storage_version() == FROM &&
		<P as GetStorageVersion>::current_storage_version() >= TO
}

// TODO: Replace this with the `VersionedMigration` that will be merged to polkadot-sdk soon.
// This is close to a copy of that code from `liam-migrations-reference-docs` branch on the
// `polkadot-sdk` repo.
impl<P, Inner, const FROM: u16, const TO: u16> OnRuntimeUpgrade
	for VersionedMigration<P, Inner, FROM, TO>
where
	P: PalletInfoAccess + GetStorageVersion<CurrentStorageVersion = StorageVersion>,
	Inner: OnRuntimeUpgrade,
{
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		if should_upgrade::<P, FROM, TO>() {
			let pre_upgrade_state = Inner::pre_upgrade()?;

			log::info!(
				"✅ {}: Pre-upgrade checks for migration from version {:?} to {:?} ok.",
				P::name(),
				FROM,
				TO
			);

			Ok(VersionedPostUpgradeData::MigrationExecuted(pre_upgrade_state).encode())
		} else {
			Ok(VersionedPostUpgradeData::Noop.encode())
		}
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if should_upgrade::<P, FROM, TO>() {
			log::info!(
				"✅ {}: Applying storage migration from version {:?} to {:?}.",
				P::name(),
				FROM,
				TO
			);
			let w = Inner::on_runtime_upgrade();
			StorageVersion::new(TO).put::<P>();
			w.saturating_add(RuntimeDbWeight::default().reads_writes(1, 1))
		} else {
			log::info!(
				"⏭ {}: Skipping storage migration from version {:?} to {:?} - consider removing this from the pallet.",
				P::name(),
				FROM,
				TO
			);
			RuntimeDbWeight::default().reads(1)
		}
	}

	/// Executes `Inner::post_upgrade` if the migration just ran.
	///
	/// pre_upgrade passes [`VersionedPostUpgradeData::MigrationExecuted`] to post_upgrade if
	/// the migration ran, and [`VersionedPostUpgradeData::Noop`] otherwise.
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(
		versioned_post_upgrade_data_bytes: sp_std::vec::Vec<u8>,
	) -> Result<(), DispatchError> {
		use codec::DecodeAll;
		match <VersionedPostUpgradeData>::decode_all(&mut &versioned_post_upgrade_data_bytes[..])
			.map_err(|_| "VersionedMigration post_upgrade failed to decode PreUpgradeData")?
		{
			VersionedPostUpgradeData::MigrationExecuted(inner_bytes) =>
				Inner::post_upgrade(inner_bytes),
			VersionedPostUpgradeData::Noop => Ok(()),
		}
	}
}

#[cfg(feature = "try-runtime")]
#[cfg(test)]
mod test_versioned_upgrade {
	use super::*;
	use frame_support::weights::Weight;
	use sp_io::TestExternalities;
	use sp_std::cell::RefCell;

	struct Pallet;

	const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

	impl PalletInfoAccess for Pallet {
		fn index() -> usize {
			0
		}

		fn name() -> &'static str {
			"Pallet"
		}

		fn module_name() -> &'static str {
			"Module"
		}

		fn crate_version() -> frame_support::traits::CrateVersion {
			Default::default()
		}

		fn name_hash() -> [u8; 16] {
			Default::default()
		}
	}

	thread_local! {
		pub static UPGRADES_COMPLETED: RefCell<u32> = const { RefCell::new(0) };
		pub static POST_UPGRADE_ERROR: RefCell<bool> = const { RefCell::new(false) };
	}

	impl GetStorageVersion for Pallet {
		type CurrentStorageVersion = StorageVersion;

		fn current_storage_version() -> StorageVersion {
			PALLET_VERSION
		}

		fn on_chain_storage_version() -> StorageVersion {
			StorageVersion::get::<Pallet>()
		}
	}

	struct DummyUpgrade;

	impl DummyUpgrade {
		fn upgrades_completed() -> u32 {
			UPGRADES_COMPLETED.with(|cell| *cell.borrow())
		}

		#[cfg(feature = "try-runtime")]
		fn set_error_on_post_upgrade(b: bool) {
			POST_UPGRADE_ERROR.with(|cell| *cell.borrow_mut() = b);
		}

		#[cfg(feature = "try-runtime")]
		fn is_error_on_post_upgrade() -> bool {
			POST_UPGRADE_ERROR.with(|cell| *cell.borrow())
		}
	}

	impl OnRuntimeUpgrade for DummyUpgrade {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			UPGRADES_COMPLETED.with(|cell| *cell.borrow_mut() += 1);
			Weight::from_parts(0, 0)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
			Ok(Default::default())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_data: Vec<u8>) -> Result<(), DispatchError> {
			if Self::is_error_on_post_upgrade() {
				Err("err".into())
			} else {
				Ok(())
			}
		}
	}

	type UpgradeFrom0To1 = VersionedMigration<Pallet, DummyUpgrade, 0, 1>;

	type UpgradeFrom1To2 = VersionedMigration<Pallet, DummyUpgrade, 1, 2>;

	type UpgradeFrom2To3 = VersionedMigration<Pallet, DummyUpgrade, 2, 3>;

	type UpgradeFrom0To2 = (UpgradeFrom0To1, UpgradeFrom1To2);

	type UpgradeFrom0To3 = (UpgradeFrom0To1, UpgradeFrom1To2, UpgradeFrom2To3);

	fn assert_upgrade(v: u16) {
		assert_eq!(Pallet::on_chain_storage_version(), v);
		assert_eq!(DummyUpgrade::upgrades_completed(), v as u32);
	}

	#[test]
	fn test_upgrade_from_0_to_1_and_0_to_1() {
		TestExternalities::new_empty().execute_with(|| {
			UpgradeFrom0To1::try_on_runtime_upgrade(true).unwrap();

			assert_upgrade(1);

			UpgradeFrom0To1::try_on_runtime_upgrade(true).unwrap();

			assert_upgrade(1);
		});
	}

	#[test]
	fn test_upgrade_from_0_to_1_and_1_to_2() {
		TestExternalities::new_empty().execute_with(|| {
			UpgradeFrom0To1::on_runtime_upgrade();

			assert_upgrade(1);

			UpgradeFrom1To2::on_runtime_upgrade();

			assert_upgrade(2);
		});
	}

	#[test]
	fn test_upgrade_from_0_to_1_and_0_to_2() {
		TestExternalities::new_empty().execute_with(|| {
			UpgradeFrom0To1::try_on_runtime_upgrade(true).unwrap();

			assert_upgrade(1);

			UpgradeFrom0To2::try_on_runtime_upgrade(true).unwrap();

			assert_upgrade(2);
		});
	}

	#[test]
	fn test_upgrade_from_0_to_2() {
		TestExternalities::new_empty().execute_with(|| {
			UpgradeFrom0To2::try_on_runtime_upgrade(true).unwrap();

			assert_upgrade(2);
		});
	}

	#[test]
	fn test_upgrade_from_0_to_2_checks_fail() {
		TestExternalities::new_empty().execute_with(|| {
			DummyUpgrade::set_error_on_post_upgrade(true);
			assert!(UpgradeFrom0To2::try_on_runtime_upgrade(true).is_err());

			// 2 upgrades are run as part of the try_on_runtime_upgrade. Even though the first
			// fails, the second is still run.
			assert_upgrade(2);
		});
	}

	#[test]
	fn test_upgrade_from_1_to_2() {
		TestExternalities::new_empty().execute_with(|| {
			UpgradeFrom1To2::try_on_runtime_upgrade(true).unwrap();

			assert_upgrade(0);
		});
	}

	#[test]
	fn test_upgrade_from_0_to_unsupported() {
		TestExternalities::new_empty().execute_with(|| {
			// This is what's called for the upgrades within the executive pallet.
			UpgradeFrom0To3::try_on_runtime_upgrade(true).unwrap();

			assert_upgrade(2);
		});
	}
}
