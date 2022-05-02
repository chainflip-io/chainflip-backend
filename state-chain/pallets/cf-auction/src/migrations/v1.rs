use frame_support::{migration::get_storage_value, traits::OnRuntimeUpgrade};
use sp_std::marker::PhantomData;

use crate::*;
use frame_support::generate_storage_alias;

generate_storage_alias!(Auction, CurrentPhase => Value<()>);
generate_storage_alias!(Auction, LastAuctionResult => Value<()>);
generate_storage_alias!(Auction, CurrentAuctionIndex => Value<()>);

const AUCTION_PALLET_NAME: &[u8] = b"Auction";
const ACTIVE_VALIDATOR_SIZE_RANGE: &[u8] = b"ActiveValidatorSizeRange";
const LOWEST_BACKUP_VALIDATOR_BID: &[u8] = b"LowestBackupValidatorBid";

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

		// rename ActiveValidatorSizeRange to AuthoritySetSizeRange

		let (min_size, max_size) =
			get_storage_value::<(u32, u32)>(AUCTION_PALLET_NAME, ACTIVE_VALIDATOR_SIZE_RANGE, b"")
				.unwrap();
		CurrentAuthoritySetSizeRange::<T>::put((min_size as u16, max_size as u16));

		// rename LowestBackupValidatorBid to LowestBackupNodeBid
		let lowest_backup_validator_bid =
			get_storage_value::<T::Amount>(AUCTION_PALLET_NAME, LOWEST_BACKUP_VALIDATOR_BID, b"")
				.unwrap();
		LowestBackupNodeBid::<T>::put(lowest_backup_validator_bid);

		T::DbWeight::get().writes(3)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		assert!(get_storage_value::<(u32, u32)>(
			AUCTION_PALLET_NAME,
			ACTIVE_VALIDATOR_SIZE_RANGE,
			b""
		)
		.is_some());

		assert!(get_storage_value::<()>(AUCTION_PALLET_NAME, LOWEST_BACKUP_VALIDATOR_BID, b"")
			.is_some());

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
		use frame_support::{assert_err, assert_ok};

		assert_ok!(CurrentAuthoritySetSizeRange::<T>::try_get());

		assert_ok!(LowestBackupNodeBid::<T>::try_get());

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
