use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

use crate::*;
use frame_support::generate_storage_alias;

generate_storage_alias!(Auction, CurrentPhase => Value<()>);
generate_storage_alias!(Auction, LastAuctionResult => Value<()>);
generate_storage_alias!(Auction, CurrentAuctionIndex => Value<()>);

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Changes are the removal of three storage items: `CurrentPhase`, `CurrentAuctionIndex` and
		// `LastAuctionResult`
		CurrentAuctionIndex::kill();
		CurrentPhase::kill();
		LastAuctionResult::kill();

		log::info!(
			target: "runtime::cf_auction",
			"🔨 migration: Auction storage completed for version 1 successful! ✅"
		);

		T::DbWeight::get().writes(3)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
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

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use frame_support::assert_err;

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
