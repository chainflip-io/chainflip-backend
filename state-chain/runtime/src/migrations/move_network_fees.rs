use crate::Runtime;
#[cfg(feature = "try-runtime")]
use cf_primitives::AssetAmount;
#[cfg(feature = "try-runtime")]
use frame_support::migrations::VersionedPostUpgradeData;
use frame_support::traits::{GetStorageVersion, StorageVersion};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct NetworkFeesMigration;

impl frame_support::traits::OnRuntimeUpgrade for NetworkFeesMigration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if <pallet_cf_swapping::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version() ==
			5 && <pallet_cf_pools::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version() ==
			4
		{
			log::info!("⏫ Applying network fees migration.");
			// Moving the FlipBuyInterval & CollectedNetworkFee storage items from the pools
			// pallet to the swapping pallet.
			cf_runtime_upgrade_utilities::move_pallet_storage::<
				pallet_cf_pools::Pallet<Runtime>,
				pallet_cf_swapping::Pallet<Runtime>,
			>(b"FlipBuyInterval");

			cf_runtime_upgrade_utilities::move_pallet_storage::<
				pallet_cf_pools::Pallet<Runtime>,
				pallet_cf_swapping::Pallet<Runtime>,
			>(b"CollectedNetworkFee");

			// Bump the version of both pallets
			StorageVersion::new(6).put::<pallet_cf_swapping::Pallet<Runtime>>();
			StorageVersion::new(5).put::<pallet_cf_pools::Pallet<Runtime>>();
		} else {
			log::info!(
					"⏭ Skipping network fees migration. {:?}, {:?}",
					<pallet_cf_swapping::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version(),
					<pallet_cf_pools::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version()
				);
		}
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
		if <pallet_cf_pools::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version() == 4
		{
			// The new CollectedNetworkFee item should be empty before the migration.
			frame_support::ensure!(
				pallet_cf_swapping::CollectedNetworkFee::<Runtime>::get() == Default::default(),
				"Incorrect pre-upgrade state for pallet swapping CollectedNetworkFee."
			);
			Ok(VersionedPostUpgradeData::MigrationExecuted(
				(
					pallet_cf_pools::migrations::old::FlipBuyInterval::<Runtime>::get(),
					pallet_cf_pools::migrations::old::CollectedNetworkFee::<Runtime>::get(),
				)
					.encode(),
			)
			.encode())
		} else {
			Ok(VersionedPostUpgradeData::Noop.encode())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		if let VersionedPostUpgradeData::MigrationExecuted(pre_upgrade_data) =
			<VersionedPostUpgradeData>::decode(&mut &state[..])
				.map_err(|_| "Failed to decode pre-upgrade state.")?
		{
			let (pre_upgrade_flip_buy_interval, pre_upgrade_collected_network_fee) =
				<(frame_system::pallet_prelude::BlockNumberFor<Runtime>, AssetAmount)>::decode(
					&mut &pre_upgrade_data[..],
				)
				.map_err(|_| "Failed to decode network fees data from pre-upgrade state.")?;

			frame_support::ensure!(
				pre_upgrade_flip_buy_interval ==
					pallet_cf_swapping::FlipBuyInterval::<Runtime>::get(),
				"Pre-upgrade state does not match post-upgrade state for FlipBuyInterval."
			);
			frame_support::ensure!(
				pre_upgrade_collected_network_fee ==
					pallet_cf_swapping::CollectedNetworkFee::<Runtime>::get(),
				"Pre-upgrade state does not match post-upgrade state for CollectedNetworkFee."
			);
		}
		Ok(())
	}
}
