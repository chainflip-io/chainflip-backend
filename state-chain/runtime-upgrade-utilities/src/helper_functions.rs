use frame_support::storage::{storage_prefix, unhashed};

/// Not to be confused with [move_prefix]. This function move the storage identified by the given
/// pallet and storage names.
pub fn move_storage(
	old_pallet_name: &[u8],
	old_storage_name: &[u8],
	new_pallet_name: &[u8],
	new_storage_name: &[u8],
) {
	let new_prefix = storage_prefix(new_pallet_name, new_storage_name);
	let old_prefix = storage_prefix(old_pallet_name, old_storage_name);

	if let Some(value) = unhashed::get_raw(&old_prefix) {
		unhashed::put_raw(&new_prefix, &value);
		unhashed::kill(&old_prefix);
	}
}
