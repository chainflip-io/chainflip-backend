use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

mod mandatory_refund_params;

pub type PalletMigration<T, I> = (
	VersionedMigration<22, 23, mandatory_refund_params::Migration<T, I>, Pallet<T, I>, ()>,
	PlaceholderMigration<23, Pallet<T, I>>,
);
