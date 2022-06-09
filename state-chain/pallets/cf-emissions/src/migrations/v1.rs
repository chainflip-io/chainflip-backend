use crate::*;
use frame_support::generate_storage_alias;

use frame_support::{migration::get_storage_value, weights::RuntimeDbWeight};
use sp_std::marker::PhantomData;

// The value for the MintInterval
// runtime constant in pallet version V0
const MINT_INTERVAL_V0: u32 = 100;
const EMISSIONS_PALLET_NAME: &[u8] = b"Emissions";
const VALIDATOR_EMISSION_INFLATION: &[u8] = b"ValidatorEmissionInflation";
const VALIDATOR_EMISSION_PER_BLOCK: &[u8] = b"ValidatorEmissionPerBlock";
const BACKUP_VALIDATOR_EMISSION_INFLATION: &[u8] = b"BackupValidatorEmissionInflation";
const BACKUP_VALIDATOR_EMISSION_PER_BLOCK: &[u8] = b"BackupValidatorEmissionPerBlock";

generate_storage_alias!(Emissions, MintInterval<T: Config> => Value<T::BlockNumber>);

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		MintInterval::<T>::put(T::BlockNumber::from(MINT_INTERVAL_V0));

		let current_authority_emission_inflation = get_storage_value::<BasisPoints>(
			EMISSIONS_PALLET_NAME,
			VALIDATOR_EMISSION_INFLATION,
			b"",
		)
		.unwrap();
		log::info!(target: "runtime::cf_emissions", "current_authority_emission_inflation: {:?}", current_authority_emission_inflation);
		CurrentAuthorityEmissionInflation::<T>::put(current_authority_emission_inflation);

		let backup_node_emission_inflation = get_storage_value::<BasisPoints>(
			EMISSIONS_PALLET_NAME,
			BACKUP_VALIDATOR_EMISSION_INFLATION,
			b"",
		)
		.unwrap();
		log::info!(target: "runtime::cf_emissions", "backup_node_emission_inflation: {:?}", backup_node_emission_inflation);
		BackupNodeEmissionInflation::<T>::put(backup_node_emission_inflation);

		let current_authority_emission_per_block = get_storage_value::<T::FlipBalance>(
			EMISSIONS_PALLET_NAME,
			VALIDATOR_EMISSION_PER_BLOCK,
			b"",
		)
		.unwrap();
		log::info!(target: "runtime::cf_emissions", "current_authority_emission_per_block: {:?}", current_authority_emission_per_block);
		CurrentAuthorityEmissionPerBlock::<T>::put(current_authority_emission_per_block);

		let backup_node_emission_per_block = get_storage_value::<T::FlipBalance>(
			EMISSIONS_PALLET_NAME,
			BACKUP_VALIDATOR_EMISSION_PER_BLOCK,
			b"",
		)
		.unwrap();
		log::info!(target: "runtime::cf_emissions", "backup_node_emission_per_block: {:?}", backup_node_emission_per_block);
		BackupNodeEmissionPerBlock::<T>::put(backup_node_emission_per_block);

		RuntimeDbWeight::default().reads_writes(4, 4)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		assert!(get_storage_value::<BasisPoints>(
			EMISSIONS_PALLET_NAME,
			VALIDATOR_EMISSION_INFLATION,
			b"",
		)
		.is_some());
		assert!(get_storage_value::<BasisPoints>(
			EMISSIONS_PALLET_NAME,
			BACKUP_VALIDATOR_EMISSION_INFLATION,
			b""
		)
		.is_some());
		assert!(get_storage_value::<BasisPoints>(
			EMISSIONS_PALLET_NAME,
			VALIDATOR_EMISSION_PER_BLOCK,
			b"",
		)
		.is_some());
		assert!(get_storage_value::<BasisPoints>(
			EMISSIONS_PALLET_NAME,
			BACKUP_VALIDATOR_EMISSION_PER_BLOCK,
			b""
		)
		.is_some());
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use frame_support::assert_ok;

		//assert_eq!(T::BlockNumber::from(100 as u32), MintInterval::<T>::get());
		assert_ok!(BackupNodeEmissionInflation::<T>::try_get());
		assert_ok!(CurrentAuthorityEmissionInflation::<T>::try_get());
		assert_ok!(BackupNodeEmissionPerBlock::<T>::try_get());
		assert_ok!(CurrentAuthorityEmissionPerBlock::<T>::try_get());
		log::info!(
			target: "runtime::cf_emissions",
			"migration: Emissions storage version v1 POST migration checks successful!"
		);
		Ok(())
	}
}
