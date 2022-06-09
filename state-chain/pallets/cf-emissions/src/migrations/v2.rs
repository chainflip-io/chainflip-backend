use crate::*;
use cf_runtime_upgrade_utilities::move_storage;
use frame_support::weights::RuntimeDbWeight;
use sp_std::marker::PhantomData;

const EMISSIONS_PALLET_NAME: &[u8] = b"Emissions";
const MINT_INTERVAL: &[u8] = b"MintInterval";
const SUPPLY_UPDATE_INTERVAL: &[u8] = b"SupplyUpdateInterval";
const LAST_MINT_BLOCK: &[u8] = b"LastMintBlock";
const LAST_SUPPLY_UPDATE_BLOCK: &[u8] = b"LastSupplyUpdateBlock";

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		move_storage(
			EMISSIONS_PALLET_NAME,
			MINT_INTERVAL,
			EMISSIONS_PALLET_NAME,
			SUPPLY_UPDATE_INTERVAL,
		);
		move_storage(
			EMISSIONS_PALLET_NAME,
			LAST_MINT_BLOCK,
			EMISSIONS_PALLET_NAME,
			LAST_SUPPLY_UPDATE_BLOCK,
		);
		RuntimeDbWeight::default().reads_writes(2, 2)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		use frame_support::migration::get_storage_value;

		assert!(get_storage_value::<T::BlockNumber>(EMISSIONS_PALLET_NAME, LAST_MINT_BLOCK, b"",)
			.is_some());
		assert!(get_storage_value::<T::BlockNumber>(EMISSIONS_PALLET_NAME, MINT_INTERVAL, b"",)
			.is_some());
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use frame_support::assert_ok;
		assert_ok!(SupplyUpdateInterval::<T>::try_get());
		assert_ok!(LastSupplyUpdateBlock::<T>::try_get());
		log::info!(
			target: "runtime::cf_emissions",
			"migration: Emissions storage version v1 POST migration checks successful!"
		);
		Ok(())
	}
}
