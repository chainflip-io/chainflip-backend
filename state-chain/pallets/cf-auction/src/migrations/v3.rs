use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

use crate::*;

/// Migrates to dynamic set size resolution.
pub struct Migration<T: Config>(PhantomData<T>);

#[derive(
	Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen,
)]
struct DynamicSetSizeParameters {
	pub min_size: u32,
	pub max_size: u32,
	pub max_contraction: u32,
	pub max_expansion: u32,
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		AuctionParameters::<T>::translate::<DynamicSetSizeParameters, _>(|old| {
			old.map(|params| SetSizeParameters {
				min_size: params.min_size,
				max_size: params.max_size,
				max_expansion: params.max_expansion,
			})
		})
		.unwrap_or_else(|e| {
			log::error!(
				"Failed to migrate DynamicSetSizeParameters to simplified AuctionParameters: {:?}",
				e
			);
			None
		});

		T::DbWeight::get().reads_writes(1, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		ensure!(
			SetSizeMaximisingAuctionResolver::try_new(
				T::EpochInfo::current_authority_count(),
				AuctionParameters::<T>::get()
			)
			.is_ok(),
			"AuctionParameters are inconsistent with the current authority count",
		);

		Ok(())
	}
}
