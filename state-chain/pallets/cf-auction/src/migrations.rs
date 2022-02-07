use super::*;

pub(crate) mod v1 {
	use super::*;
	use frame_support::generate_storage_alias;

	generate_storage_alias!(Auction, CurrentPhase => Value<()>);
	generate_storage_alias!(Auction, LastAuctionResult => Value<()>);

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");

		// For my sanity
		log::info!(target: "runtime:cf_auction", "AuctionPhase.exists()? {:?}", CurrentPhase::exists());
		log::info!(target: "runtime:cf_auction", "LastAuctionResult.exits()? {:?}", LastAuctionResult::exists());

		log::info!(
			target: "runtime::cf_auction",
			"migration: Auction storage version v1 PRE migration checks successful!",
		);

		Ok(())
	}

	pub fn migrate<T: Config>() -> frame_support::weights::Weight {
		// Current version is is genesis, upgrade to version 1
		// Changes are the removal of two storage items: `CurrentPhase` and `LastAuctionResult`
		CurrentPhase::kill();
		LastAuctionResult::kill();

		T::DbWeight::get().writes(2)
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);

		assert!(CurrentPhase::exists(), "CurrentPhase should be gone");
		assert!(LastAuctionResult::exists(), "LastAuctionResult should be gone");

		log::info!(
			target: "runtime::cf_auction",
			"migration: Auction storage version v1 POST migration checks successful!"
		);

		Ok(())
	}
}
