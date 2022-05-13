use crate::*;
use frame_support::{generate_storage_alias, storage::migration::*};
mod v1_types {
	use super::*;
	use codec::{Decode, Encode};
	use frame_support::RuntimeDebug;
	#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, Default)]
	pub struct AuctionResult<ValidatorId, Amount> {
		pub winners: Vec<ValidatorId>,
		pub minimum_active_bid: Amount,
	}
}

generate_storage_alias!(Validator, Force => Value<bool>);

fn take_v1_auction_result<T: Config>(
) -> Option<v1_types::AuctionResult<<T as cf_traits::Chainflip>::ValidatorId, T::Amount>> {
	take_storage_value(b"Auction", b"LastAuctionResult", b"")
}

#[cfg(feature = "try-runtime")]
fn get_v1_auction_result<T: Config>(
) -> Option<v1_types::AuctionResult<<T as cf_traits::Chainflip>::ValidatorId, T::Amount>> {
	get_storage_value(b"Auction", b"LastAuctionResult", b"")
}

const PERCENTAGE_CLAIM_PERIOD: u8 = 50;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if let Some(v1_types::AuctionResult { .. }) = take_v1_auction_result::<T>() {
			// Kill the Force
			Force::kill();
			// Set last expired epoch to the previous one
			let current_epoch_index = CurrentEpoch::<T>::get();
			LastExpiredEpoch::<T>::put(current_epoch_index.saturating_sub(1));
			// Set the claim percentage
			ClaimPeriodAsPercentage::<T>::put(PERCENTAGE_CLAIM_PERIOD);
			T::DbWeight::get().reads_writes(2, 4)
		} else {
			log::error!("Validator migration failed, there is no auction result.");
			T::DbWeight::get().reads(1)
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		assert!(
			get_v1_auction_result::<T>().is_some(),
			"if we don't have a previous auction then we shouldn't be upgrading"
		);

		log::info!(
			target: "runtime::cf_validator",
			"migration: Validator storage version v2 PRE migration checks successful!",
		);

		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		assert!(!Force::exists(), "Force should not be set.");

		let current_epoch_index = CurrentEpoch::<T>::get();

		assert_eq!(LastExpiredEpoch::<T>::get(), current_epoch_index.saturating_sub(1));
		assert_eq!(ClaimPeriodAsPercentage::<T>::get(), PERCENTAGE_CLAIM_PERIOD);

		log::info!(
			target: "runtime::cf_validator",
			"migration: Validator storage version v2 POST migration checks successful!"
		);

		Ok(())
	}
}
