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
	P: PalletInfoAccess + GetStorageVersion<CurrentStorageVersion = StorageVersion>,
	U: OnRuntimeUpgrade,
	const FROM: u16,
	const TO: u16,
>(PhantomData<(P, U)>);

#[cfg(feature = "try-runtime")]
mod try_runtime_helpers {
	use frame_support::traits::PalletInfoAccess;
	use sp_std::vec::Vec;

	#[cfg(feature = "std")]
	pub use with_std::*;

	#[cfg(not(feature = "std"))]
	pub use without_std::*;

	#[cfg(feature = "std")]
	mod with_std {
		use super::*;
		use core::cell::RefCell;
		use sp_std::{
			cmp::{max, min},
			collections::btree_map::BTreeMap,
		};

		thread_local! {
			pub static MIGRATION_BOUNDS: RefCell<BTreeMap<&'static str, (u16, u16)>> = Default::default();
			#[allow(clippy::type_complexity)]
			pub static MIGRATION_STATE: RefCell<BTreeMap<&'static str, BTreeMap<(u16, u16), Vec<u8>>>> = Default::default();
		}

		pub fn update_migration_bounds<T: PalletInfoAccess, const FROM: u16, const TO: u16>() {
			MIGRATION_BOUNDS.with(|cell| {
				cell.borrow_mut()
					.entry(T::name())
					.and_modify(|(from, to)| {
						*from = min(*from, FROM);
						*to = max(*to, TO);
					})
					.or_insert((FROM, TO));
			});
		}

		pub fn get_migration_bounds<T: PalletInfoAccess>() -> Option<(u16, u16)> {
			MIGRATION_BOUNDS.with(|cell| cell.borrow().get(T::name()).copied())
		}

		pub fn save_state<T: PalletInfoAccess, const FROM: u16, const TO: u16>(s: Vec<u8>) {
			MIGRATION_STATE
				.with(|cell| cell.borrow_mut().entry(T::name()).or_default().insert((FROM, TO), s));
		}

		pub fn restore_state<T: PalletInfoAccess, const FROM: u16, const TO: u16>() -> Vec<u8> {
			MIGRATION_STATE.with(|cell| {
				cell.borrow()
					.get(T::name())
					.cloned()
					.unwrap_or_default()
					.get(&(FROM, TO))
					.cloned()
					.unwrap_or_default()
			})
		}
	}

	#[cfg(not(feature = "std"))]
	mod without_std {
		use super::*;

		pub fn update_migration_bounds<T: PalletInfoAccess, const FROM: u16, const TO: u16>() {
			log::warn!("❗️ Runtime upgrade utilities are not supported in no-std.");
		}

		pub fn get_migration_bounds<T: PalletInfoAccess>() -> Option<(u16, u16)> {
			Default::default()
		}

		pub fn save_state<T: PalletInfoAccess, const FROM: u16, const TO: u16>(s: Vec<u8>) {
			log::warn!("❗️ Runtime upgrade utilities are not supported in no-std.");
		}

		pub fn restore_state<T: PalletInfoAccess, const FROM: u16, const TO: u16>() -> Vec<u8> {
			Default::default()
		}
	}
}

fn should_upgrade<P: GetStorageVersion<CurrentStorageVersion = StorageVersion>, const FROM: u16, const TO: u16>() -> bool {
	<P as GetStorageVersion>::on_chain_storage_version() == FROM &&
		<P as GetStorageVersion>::current_storage_version() >= TO
}

impl<P, U, const FROM: u16, const TO: u16> OnRuntimeUpgrade for VersionedMigration<P, U, FROM, TO>
where
	P: PalletInfoAccess + GetStorageVersion<CurrentStorageVersion = StorageVersion>,
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
		if should_upgrade::<P, FROM, TO>() {
			let state = U::pre_upgrade().map_err(|e| {
				log::error!(
					"💥 {}: Pre-upgrade checks for migration failed at stage {FROM}->{TO}: {:?}",
					P::name(),
					e
				);
				"🛑 Pallet pre-upgrade checks failed."
			})?;
			log::info!(
				"✅ {}: Pre-upgrade checks for migration from version {:?} to {:?} ok.",
				P::name(),
				FROM,
				TO
			);
			try_runtime_helpers::save_state::<P, FROM, TO>(state.clone());
			Ok(state)
		} else {
			Ok(Vec::new())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), &'static str> {
		if let Some((lowest, highest)) = try_runtime_helpers::get_migration_bounds::<P>() {
			assert_eq!(
				<P as GetStorageVersion>::on_chain_storage_version(),
				highest,
				"Runtime upgrade expected to process all pre-checks, then upgrade, then all post-checks.",
			);
			U::post_upgrade(try_runtime_helpers::restore_state::<P, FROM, TO>()).map_err(|e| {
					log::error!(
					"💥 {}: Post-upgrade checks for migration from version {lowest} to {highest} failed at stage {FROM}->{TO}: {:?}",
					P::name(),
					e
				);
					"🛑 Pallet post-upgrade checks failed."
				})?;
			log::info!("✅ {}: Post-upgrade checks ok.", P::name());
			Ok(())
		} else {
			Ok(())
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
