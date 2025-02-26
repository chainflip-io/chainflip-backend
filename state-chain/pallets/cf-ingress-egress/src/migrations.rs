use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;
mod add_ccm_aux_data_lookup_key;

pub type PalletMigration<T, I> = (
	VersionedMigration<
		20,
		21,
		add_ccm_aux_data_lookup_key::AddCcmAuxDataLookupKeyMigration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<21, Pallet<T, I>>,
);
