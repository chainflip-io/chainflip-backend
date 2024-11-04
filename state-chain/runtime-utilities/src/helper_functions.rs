use frame_support::{migration::move_storage_from_pallet, traits::PalletInfoAccess};

/// Move storage between pallets.
pub fn move_pallet_storage<From: PalletInfoAccess, To: PalletInfoAccess>(storage_name: &[u8]) {
	log::info!(
		"⏫ Moving storage {} from {} to {}.",
		sp_std::str::from_utf8(storage_name).expect("storage names are all valid utf8"),
		From::name(),
		To::name(),
	);
	move_storage_from_pallet(storage_name, From::name().as_bytes(), To::name().as_bytes());
}
