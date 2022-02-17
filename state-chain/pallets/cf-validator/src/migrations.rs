use cf_traits::{AuctionResult, Auctioneer};
use frame_support::traits::Get;

use super::*;

pub(crate) mod v1 {
	use super::*;
	const PERCENTAGE_CLAIM_PERIOD: u8 = 50;

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");
		assert!(
			T::Auctioneer::auction_result().is_some(),
			"if we don't have a previous auction then we shouldn't be upgrading"
		);

		log::info!(
			target: "runtime::cf_validator",
			"migration: Validator storage version v1 PRE migration checks successful!"
		);

		Ok(())
	}

	pub fn migrate<T: Config>() -> frame_support::weights::Weight {
		// Current version is is genesis, upgrade to version 1
		// Changes are the addition of two storage items: `Validators`, `Bond` and
		// `LastExpiredEpoch` We are using `Auctioneer::auction_result()` as the last successful
		// auction to determine the bond to set.  Although we can derive the winners and hence the
		// active validating set from the same storage item as the bond we want to maintain
		// continuity with the genesis version(0) by reading this from the session pallet.
		if let Some(AuctionResult { minimum_active_bid, .. }) = T::Auctioneer::auction_result() {
			// Set the bond to that of the last auction result
			Bond::<T>::put(minimum_active_bid);
			let validators = <pallet_session::Pallet<T>>::validators();
			// Set the validating set from the session pallet
			Validators::<T>::put(validators);
			// Set last expired epoch to the previous one
			let current_epoch_index = CurrentEpoch::<T>::get();
			LastExpiredEpoch::<T>::put(current_epoch_index.saturating_sub(1));
			// Set the claim percentage
			ClaimPeriodAsPercentage::<T>::put(PERCENTAGE_CLAIM_PERIOD);
			T::DbWeight::get().reads_writes(3, 4)
		} else {
			log::error!(
				target: "runtime::cf_validator",
				"migration: Migration failed, there is no auction result."
			);
			T::DbWeight::get().reads(1)
		}
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);

		let AuctionResult { minimum_active_bid, .. } = T::Auctioneer::auction_result()
			.expect("if we don't have a previous auction then we shouldn't be upgrading");

		assert_eq!(minimum_active_bid, Bond::<T>::get());

		assert_eq!(<pallet_session::Pallet<T>>::validators(), Validators::<T>::get());

		let current_epoch_index = CurrentEpoch::<T>::get();

		assert_eq!(LastExpiredEpoch::<T>::get(), current_epoch_index.saturating_sub(1));
		assert_eq!(ClaimPeriodAsPercentage::<T>::get(), PERCENTAGE_CLAIM_PERIOD);

		log::info!(
			target: "runtime::cf_validator",
			"migration: Validator storage version v1 POST migration checks successful!"
		);

		Ok(())
	}
}
