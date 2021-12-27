use super::*;
use cf_traits::AuctionResult;
use frame_support::traits::{Get, GetStorageVersion, PalletInfoAccess, StorageVersion};

pub fn migrate_to_v1<T: Config, P: GetStorageVersion + PalletInfoAccess>(
) -> frame_support::weights::Weight {
	let on_chain_storage_version = <P as GetStorageVersion>::on_chain_storage_version();
	log::info!(
		target: "runtime::cf_validator",
		"Running migration storage v1 for cf_validator with storage version {:?}",
		on_chain_storage_version,
	);

	if on_chain_storage_version < 1 {
		// Current version is is genesis, upgrade to version 1
		// Changes are the addition of two storage items: `Validators` and `Bond`
		// We are using `Auctioneer::auction_result()` as the last successful auction to
		// determine the bond to set.  Although we can derive the winners and hence the
		// active validating set from the same storage item as the bond we want to maintain
		// continuity with the genesis version(0) by reading this from the session pallet.
		let AuctionResult { minimum_active_bid, .. } =
			// We expect to have a previous auction result and if not then this is not
			// recoverable so panic
			T::Auctioneer::auction_result().expect(
				"if we don't have a previous auction then we shouldn't be upgrading",
			);

		// Set the bond to that of the last auction result
		Bond::<T>::put(minimum_active_bid);
		let validators = <pallet_session::Pallet<T>>::validators();
		// Set the validating set from the session pallet
		Validators::<T>::put(validators.clone());
		// There is a bug in version v1.0.0 where the validator lookup is out of sync
		// with the current active validator set so we would want to align this with the
		// current validating set.
		// See https://github.com/chainflip-io/chainflip-backend/issues/1072.
		ValidatorLookup::<T>::remove_all(None);
		let number_of_validators = validators.len();
		for validator in validators {
			ValidatorLookup::<T>::insert(validator, ());
		}

		// Update the version number to 1
		StorageVersion::new(1).put::<P>();
		log::info!(
			target: "runtime::cf_validator",
			"Running migration storage v1 for cf_validator with storage version {:?} was complete",
			on_chain_storage_version,
		);
		T::DbWeight::get().reads_writes(3, 2 + number_of_validators as Weight)
	} else {
		log::warn!(
			target: "runtime::cf_validator",
			"Attempted to apply migration to v1 but failed because storage version is {:?}",
			on_chain_storage_version,
		);
		T::DbWeight::get().reads(1)
	}
}
