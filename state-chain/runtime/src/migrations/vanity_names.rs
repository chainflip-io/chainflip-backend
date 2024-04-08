use crate::{AccountId, Runtime};
use frame_support::traits::{GetStorageVersion, StorageVersion};

pub struct Migration;

impl frame_support::traits::OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if <pallet_cf_validator::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version() == 0 &&
			<pallet_cf_account_roles::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version(
			) == 1
		{
			log::info!("⏫ Applying VanityNames migration.");
			// Moving the VanityNames storage item from the validators pallet to the account roles pallet.
			cf_runtime_upgrade_utilities::move_pallet_storage::<
				pallet_cf_validator::Pallet<Runtime>,
				pallet_cf_account_roles::Pallet<Runtime>,
			>(b"VanityNames");

			// Bump the version of both pallets
			StorageVersion::new(1).put::<pallet_cf_validator::Pallet<Runtime>>();
			StorageVersion::new(2).put::<pallet_cf_account_roles::Pallet<Runtime>>();
		} else {
			log::info!(
				"⏭ Skipping VanityNames migration. {:?}, {:?}",
				<pallet_cf_validator::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version(),
				<pallet_cf_account_roles::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version()
			);
		}
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
		use codec::Encode;
		use frame_support::migrations::VersionedPostUpgradeData;

		if <pallet_cf_validator::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version() ==
			0
		{
			// The new VanityNames item should be empty before the upgrade.
			frame_support::ensure!(
				pallet_cf_account_roles::VanityNames::<Runtime>::get().is_empty(),
				"Incorrect pre-upgrade state for pallet account roles VanityNames."
			);
			Ok(VersionedPostUpgradeData::MigrationExecuted(
				pallet_cf_validator::migrations::old::VanityNames::<Runtime>::get().encode(),
			)
			.encode())
		} else {
			Ok(VersionedPostUpgradeData::Noop.encode())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		use codec::Decode;
		use frame_support::migrations::VersionedPostUpgradeData;
		use sp_std::collections::btree_map::BTreeMap;

		if let VersionedPostUpgradeData::MigrationExecuted(pre_upgrade_data) =
			<VersionedPostUpgradeData>::decode(&mut &state[..])
				.map_err(|_| "Failed to decode pre-upgrade state.")?
		{
			let pre_upgrade_vanity_names =
				<BTreeMap<AccountId, frame_support::BoundedVec<u8, _>>>::decode(
					&mut &pre_upgrade_data[..],
				)
				.map_err(|_| "Failed to decode VanityNames from pre-upgrade state.")?;

			frame_support::ensure!(
				pre_upgrade_vanity_names == pallet_cf_account_roles::VanityNames::<Runtime>::get(),
				"Pre-upgrade state does not match post-upgrade state for VanityNames."
			);
		}
		Ok(())
	}
}
