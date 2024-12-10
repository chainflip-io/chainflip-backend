use crate::Runtime;
use cf_chains::{
	arb::ArbitrumTrackedData, instances::ArbitrumInstance, Arbitrum, Chain, ChainState,
};
use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};
use serde::{Deserialize, Serialize};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::prelude::*;

// Define the old data structure in a temporary module
mod old {
	use super::*;
	use frame_support::{pallet_prelude::*, sp_runtime::FixedU64};

	#[derive(
		Copy,
		Clone,
		RuntimeDebug,
		PartialEq,
		Eq,
		Encode,
		Decode,
		MaxEncodedLen,
		TypeInfo,
		Serialize,
		Deserialize,
	)]
	#[codec(mel_bound())]
	pub struct ArbitrumTrackedData {
		pub base_fee: <Arbitrum as Chain>::ChainAmount,
		pub gas_limit_multiplier: FixedU64,
	}

	#[derive(
		PartialEqNoBound,
		EqNoBound,
		CloneNoBound,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		DebugNoBound,
		Serialize,
		Deserialize,
	)]
	pub struct ChainState {
		pub block_height: <Arbitrum as Chain>::ChainBlockNumber,
		pub tracked_data: ArbitrumTrackedData,
	}
}

pub struct ArbitrumChainTrackingMigration;

impl UncheckedOnRuntimeUpgrade for ArbitrumChainTrackingMigration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use frame_support::assert_ok;

		// Perform the state translation from the old to the new format
		assert_ok!(
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::translate::<
				old::ChainState,
				_,
			>(|maybe_old_state| {
				let (block_height, base_fee) = maybe_old_state
					.map(|state| (state.block_height, state.tracked_data.base_fee))
					.unwrap_or((Default::default(), Default::default()));
				Some(ChainState {
					block_height,
					tracked_data: ArbitrumTrackedData { base_fee, l1_base_fee_estimate: 1_u128 },
				})
			})
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		// Pre-checks to ensure the migration is needed
		if pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::get().is_none()
		{
			return Err(DispatchError::Other(
				"CurrentChainState for ArbitrumInstance does not exist",
			));
		}
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		// Post-checks to verify the migration was successful
		let new_state =
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::get().ok_or(
				{
					DispatchError::Other(
						"CurrentChainState for ArbitrumInstance is missing after migration",
					)
				},
			)?;
		assert_eq!(new_state.tracked_data.l1_base_fee_estimate, 1_u128);

		Ok(())
	}
}

pub struct NoOpMigration;
impl UncheckedOnRuntimeUpgrade for NoOpMigration {
	fn on_runtime_upgrade() -> Weight {
		Default::default()
	}
}
