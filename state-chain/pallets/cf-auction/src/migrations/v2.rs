use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

use crate::*;
use frame_support::generate_storage_alias;

generate_storage_alias!(Auction, ActiveValidatorSizeRange => Value<(u32, u32)>);

/// Migrates to dynamic set size resolution.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if let Some((min_size, max_size)) = ActiveValidatorSizeRange::take() {
			AuctionParameters::<T>::put(DynamicSetSizeParameters {
				min_size,
				max_size,
				max_contraction: max_size,
				max_expansion: max_size,
			});
		}

		T::DbWeight::get().reads_writes(1, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		ensure!(
			ActiveValidatorSizeRange::get().is_some(),
			"Expected ActiveValidatorSizeRange to be set"
		);

		Ok(())
	}
}
