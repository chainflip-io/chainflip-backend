use super::*;

pub(crate) mod v1 {
	use super::*;
	use frame_support::generate_storage_alias;

	generate_storage_alias!(Auction, CurrentPhase => Value<()>);
	generate_storage_alias!(Auction, LastAuctionResult => Value<()>);
	generate_storage_alias!(Auction, CurrentAuctionIndex => Value<()>);

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");

		// For my sanity
		log::info!(target: "runtime:cf_auction", "AuctionPhase.exists()? {:?}", CurrentPhase::exists());
		log::info!(target: "runtime:cf_auction", "LastAuctionResult.exits()? {:?}", LastAuctionResult::exists());
		log::info!(target: "runtime:cf_auction", "CurrentAuctionIndex.exits()? {:?}", CurrentAuctionIndex::exists());

		log::info!(
			target: "runtime::cf_auction",
			"🔨 migration: Auction storage version v1 PRE migration checks successful! ✅",
		);

		Ok(())
	}

	pub fn migrate<T: Config>() -> frame_support::weights::Weight {
		// Current version is is genesis, upgrade to version 1
		// Changes are the removal of two storage items: `CurrentPhase`, `CurrentAuctionIndex` and
		// `LastAuctionResult`
		CurrentAuctionIndex::kill();
		CurrentPhase::kill();
		LastAuctionResult::kill();

		log::info!(
			target: "runtime::cf_auction",
			"🔨 migration: Auction storage completed for version 1 successful! ✅"
		);

		T::DbWeight::get().writes(2)
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		use frame_support::assert_err;

		assert_eq!(P::on_chain_storage_version(), releases::V1);

		// We should expect no values for these items
		assert_err!(CurrentPhase::try_get(), ());
		assert_err!(LastAuctionResult::try_get(), ());
		assert_err!(CurrentAuctionIndex::try_get(), ());

		log::info!(
			target: "runtime::cf_auction",
			"🔨 migration: Auction storage version v1 POST migration checks successful! ✅"
		);

		Ok(())
	}
}
