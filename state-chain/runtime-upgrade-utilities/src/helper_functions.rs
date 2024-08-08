use frame_support::{migration::move_storage_from_pallet, traits::PalletInfoAccess};

/// Move storage between pallets.
pub fn move_pallet_storage<From: PalletInfoAccess, To: PalletInfoAccess>(storage_name: &[u8]) {
	move_pallet_storage_to::<From>(storage_name, To::name())
}

/// Move storage between pallets.
pub fn move_pallet_storage_to<From: PalletInfoAccess>(
	storage_name: &[u8],
	destination_pallet_name: &str,
) {
	log::info!(
		"⏫ Moving storage {} from {} to {}.",
		sp_std::str::from_utf8(storage_name).expect("storage names are all valid utf8"),
		From::name(),
		destination_pallet_name,
	);
	move_storage_from_pallet(
		storage_name,
		From::name().as_bytes(),
		destination_pallet_name.as_bytes(),
	);
}
