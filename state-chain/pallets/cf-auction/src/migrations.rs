use cf_traits::Auctioneer;
use frame_support::traits::Get;

use super::*;

pub(crate) mod v1 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");

		// We don't want to run the migration during an auction
		assert!(
			Pallet::<T>::phase() == AuctionPhase::WaitingForBids,
			"Migration should be run out of auction."
		);

		log::info!(
			target: "runtime::cf_auction",
			"migration: Auction storage version v1 PRE migration checks successful!"
		);

		Ok(())
	}

	pub fn migrate<T: Config>() -> frame_support::weights::Weight {
		// Current version is is genesis, upgrade to version 1
		// With version 2 we have changed the type for CurrentPhase, there is no need to do anything
		// here as our try-runtime checks will assert if we are in an auction.
		// As we don't want to panic here we will log the case we have run the migration and that
		// we are in an auction, we can't really do much more than that.
		if Pallet::<T>::phase() != AuctionPhase::WaitingForBids {
			log::error!(
				target: "runtime::cf_auction",
				"migration: Migration failed, this was not to be run during an auction."
			);
		}
		T::DbWeight::get().reads(1)
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);

		// We should see no changes to the auction phase post migration
		assert!(
			Pallet::<T>::phase() == AuctionPhase::WaitingForBids,
			"The migration should not have updated auction state."
		);

		log::info!(
			target: "runtime::cf_auction",
			"migration: Auction storage version v1 POST migration checks successful!"
		);

		Ok(())
	}
}
