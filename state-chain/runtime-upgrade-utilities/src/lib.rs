#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::{
	pallet_prelude::GetStorageVersion,
	traits::{OnRuntimeUpgrade, PalletInfoAccess, StorageVersion},
	weights::RuntimeDbWeight,
};
use sp_std::marker::PhantomData;

mod helper_functions;
pub use helper_functions::*;

#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// A Runtime upgrade for a pallet that migrates the pallet from version `FROM` to version `TO`.
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
	use frame_support::{storage_alias, traits::PalletInfoAccess, Twox64Concat};
	use sp_std::{
		cmp::{max, min},
		vec::Vec,
	};

	#[storage_alias]
	type MigrationBounds = StorageMap<RuntimeUpgradeUtils, Twox64Concat, Vec<u8>, (u16, u16)>;

	pub fn update_migration_bounds<T: PalletInfoAccess, const FROM: u16, const TO: u16>() {
		MigrationBounds::mutate(T::name().as_bytes(), |bounds| {
			*bounds = match bounds {
				None => Some((FROM, TO)),
				Some((lower, upper)) => Some((min(FROM, *(lower)), max(TO, *(upper)))),
			}
		});
	}

	pub fn get_migration_bounds<T: PalletInfoAccess>() -> Option<(u16, u16)> {
		MigrationBounds::get(T::name().as_bytes())
	}
}

fn should_upgrade<P: GetStorageVersion, const FROM: u16, const TO: u16>() -> bool {
	<P as GetStorageVersion>::on_chain_storage_version() == FROM &&
		<P as GetStorageVersion>::current_storage_version() >= TO
}

impl<P, U, const FROM: u16, const TO: u16> OnRuntimeUpgrade for VersionedMigration<P, U, FROM, TO>
where
	P: PalletInfoAccess + GetStorageVersion,
	U: OnRuntimeUpgrade,
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if should_upgrade::<P, FROM, TO>() {
			log::info!(
				"✅ {}: Applying storage migration from version {:?} to {:?}.",
				P::name(),
				FROM,
				TO
			);
			let w = U::on_runtime_upgrade();
			StorageVersion::new(TO).put::<P>();
			#[cfg(feature = "try-runtime")]
			try_runtime_helpers::update_migration_bounds::<P, FROM, TO>();
			w + RuntimeDbWeight::default().reads_writes(1, 1)
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

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		let state = U::pre_upgrade()?;
		log::info!(
			"✅ {}: Pre-upgrade checks for migration from version {:?} to {:?} ok.",
			P::name(),
			FROM,
			TO
		);
		Ok(state)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
		let (_, expected_version) =
			try_runtime_helpers::get_migration_bounds::<P>().ok_or_else(|| {
				log::error!("💥 {}: Expected a runtime storage upgrade.", P::name(),);
				"🛑 Pallet expected a runtime storage upgrade."
			})?;

		if <P as GetStorageVersion>::on_chain_storage_version() == expected_version {
			U::post_upgrade(state)?;
			log::info!("✅ {}: Post-upgrade checks ok.", P::name());
			Ok(())
		} else {
			log::error!(
				"💥 {}: Expected post-upgrade storage version {:?}, found {:?}.",
				P::name(),
				expected_version,
				<P as GetStorageVersion>::on_chain_storage_version(),
			);
			Err("🛑 Pallet storage migration version mismatch.")
		}
	}
}

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
	}

	thread_local! {
		pub static UPGRADES_COMPLETED: RefCell<u32> = RefCell::new(0);
		pub static POST_UPGRADE_ERROR: RefCell<bool> = RefCell::new(false);
	}

	impl GetStorageVersion for Pallet {
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
			Weight::from_ref_time(0)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
			Ok(Default::default())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_data: Vec<u8>) -> Result<(), &'static str> {
			if Self::is_error_on_post_upgrade() {
				Err("err")
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
			UpgradeFrom0To1::on_runtime_upgrade();

			assert_upgrade(1);

			UpgradeFrom0To1::on_runtime_upgrade();

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
			UpgradeFrom0To1::on_runtime_upgrade();

			assert_upgrade(1);

			UpgradeFrom0To2::on_runtime_upgrade();

			assert_upgrade(2);
		});
	}

	#[test]
	fn test_upgrade_from_0_to_2() {
		TestExternalities::new_empty().execute_with(|| {
			UpgradeFrom0To2::on_runtime_upgrade();

			assert_upgrade(2);
		});
	}

	#[test]
	fn test_upgrade_from_1_to_2() {
		TestExternalities::new_empty().execute_with(|| {
			UpgradeFrom1To2::on_runtime_upgrade();

			assert_upgrade(0);
		});
	}

	#[test]
	fn test_upgrade_from_0_to_unsupported() {
		TestExternalities::new_empty().execute_with(|| {
			UpgradeFrom0To3::on_runtime_upgrade();

			assert_upgrade(2);
		});
	}

	#[cfg(feature = "try-runtime")]
	#[test]
	fn test_pre_post_upgrade() {
		use frame_support::{assert_err, assert_ok};

		TestExternalities::new_empty().execute_with(|| {
			assert_ok!(UpgradeFrom0To1::pre_upgrade());
			UpgradeFrom0To1::on_runtime_upgrade();
			assert_ok!(UpgradeFrom0To1::post_upgrade(Default::default()));
			assert_eq!(try_runtime_helpers::get_migration_bounds::<Pallet>(), Some((0, 1)));

			// Post-migration runs even if upgrade is out of bounds.
			DummyUpgrade::set_error_on_post_upgrade(true);
			assert_ok!(UpgradeFrom2To3::pre_upgrade());
			UpgradeFrom2To3::on_runtime_upgrade();
			assert!(UpgradeFrom2To3::post_upgrade(Default::default()).is_err());
			assert_eq!(try_runtime_helpers::get_migration_bounds::<Pallet>(), Some((0, 1)));
		});

		// Error on post-upgrade is propagated.
		TestExternalities::new_empty().execute_with(|| {
			DummyUpgrade::set_error_on_post_upgrade(true);
			assert_ok!(UpgradeFrom0To1::pre_upgrade());
			UpgradeFrom0To1::on_runtime_upgrade();
			assert_err!(UpgradeFrom0To1::post_upgrade(Default::default()), "err");
			assert_eq!(try_runtime_helpers::get_migration_bounds::<Pallet>(), Some((0, 1)));
		});
	}
}
