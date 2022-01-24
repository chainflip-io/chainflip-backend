use frame_support::traits::Get;

use super::*;

pub(crate) mod v1 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");

		log::info!(
			target: "runtime::cf_auction",
			"migration: Auction storage version v1 PRE migration checks successful!"
		);

		Ok(())
	}

	pub fn migrate<T: Config>() -> frame_support::weights::Weight {
		// Current version is is genesis, upgrade to version 1
		// With version 2 we have changed the type for CurrentPhase. If we were in an auction, where
		// `CurrentPhase != AuctionPhase::WaitingForBids`, then we state back to
		// `AuctionPhase::WaitingForBids`
		if CurrentPhase::<T>::get() != AuctionPhase::WaitingForBids {
			CurrentPhase::<T>::set(AuctionPhase::WaitingForBids);
			log::info!(
				target: "runtime::cf_auction",
				"migration: Auction migration setting phase to `WaitingForBids`"
			);
			return T::DbWeight::get().reads_writes(1, 1)
		}

		T::DbWeight::get().reads(1)
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);

		// We should see the auction state set to `WaitingForBids` in migration
		assert!(
			Pallet::<T>::phase() == AuctionPhase::WaitingForBids,
			"The migration should have reset the auction phase"
		);

		log::info!(
			target: "runtime::cf_auction",
			"migration: Auction storage version v1 POST migration checks successful!"
		);

		Ok(())
	}
}
