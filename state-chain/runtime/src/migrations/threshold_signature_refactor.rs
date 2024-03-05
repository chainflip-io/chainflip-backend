use crate::Runtime;
use cf_chains::{
	instances::{ChainInstanceFor, CryptoInstanceFor},
	Bitcoin, Chain, Ethereum, Polkadot,
};
use cf_runtime_upgrade_utilities::move_pallet_storage;
use frame_support::traits::GetStorageVersion;

pub struct Migration;

pub fn migrate_instance<C: Chain>()
where
	Runtime: pallet_cf_threshold_signature::Config<CryptoInstanceFor<C>>,
	Runtime: pallet_cf_vaults::Config<ChainInstanceFor<C>>,
{
	// The migration needs to be run *after* the vaults pallet migration (3 -> 5) and *before*
	// the threshold signer pallet migration (4 -> 5).
	if <pallet_cf_threshold_signature::Pallet::<Runtime, CryptoInstanceFor<C>> as GetStorageVersion>::on_chain_storage_version() == 4 &&
			<pallet_cf_vaults::Pallet::<Runtime, ChainInstanceFor<C>> as GetStorageVersion>::on_chain_storage_version() == 5 {

		log::info!("✅ Applying threshold signature refactor storage migration.");
		for storage_name in [
			"CeremonyIdCounter",
			"KeygenSlashAmount",
			"Vaults",
		] {
			move_pallet_storage::<
				pallet_cf_vaults::Pallet<Runtime, ChainInstanceFor<C>>,
				pallet_cf_threshold_signature::Pallet<Runtime, CryptoInstanceFor<C>>,
			>(storage_name.as_bytes());
		}
	} else {
		log::info!("⏭ Skipping threshold signature refactor migration.");
	}
}

impl frame_support::traits::OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("⏫ Applying threshold signature refactor storage migration.");
		migrate_instance::<Ethereum>();
		migrate_instance::<Bitcoin>();
		migrate_instance::<Polkadot>();

		Default::default()
	}
}
