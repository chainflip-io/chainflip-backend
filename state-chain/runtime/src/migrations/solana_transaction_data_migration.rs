use frame_support::traits::UncheckedOnRuntimeUpgrade;
use pallet_cf_broadcast::BroadcastData;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use cf_chains::sol::SolanaTransactionData;
use codec::{Decode, Encode};

pub mod old {
	use cf_chains::sol::{SolLegacyMessage, SolSignature};
	use cf_primitives::BroadcastId;
	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};

	use super::*;

	#[derive(PartialEq, Eq, Encode, Decode)]
	pub struct SolanaTransactionData {
		pub serialized_transaction: Vec<u8>,
	}

	#[derive(PartialEq, Eq, Encode, Decode)]
	pub struct SolanaBroadcastData {
		pub broadcast_id: BroadcastId,
		pub transaction_payload: SolanaTransactionData,
		pub threshold_signature_payload: SolLegacyMessage,
		pub transaction_out_id: SolSignature,
		pub nominee: Option<<Runtime as frame_system::Config>::AccountId>,
	}

	#[frame_support::storage_alias]
	pub type AwaitingBroadcast =
		StorageMap<SolanaBroadcaster, Twox64Concat, BroadcastId, SolanaBroadcastData, OptionQuery>;
}

pub struct SolanaTransactionDataMigration;

impl UncheckedOnRuntimeUpgrade for SolanaTransactionDataMigration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok((old::AwaitingBroadcast::iter().count() as u64).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::translate_values::<
			old::SolanaBroadcastData,
			_,
		>(|old_sol_broadcast_data| {
			Some(BroadcastData::<Runtime, SolanaInstance> {
				broadcast_id: old_sol_broadcast_data.broadcast_id,
				transaction_payload: SolanaTransactionData {
					serialized_transaction: old_sol_broadcast_data
						.transaction_payload
						.serialized_transaction,
					skip_preflight: true,
				},
				threshold_signature_payload: old_sol_broadcast_data.threshold_signature_payload,
				transaction_out_id: old_sol_broadcast_data.transaction_out_id,
				nominee: old_sol_broadcast_data.nominee,
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_awaiting_broadcast_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_awaiting_broadcast_count =
			pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::iter().count()
				as u64;

		assert_eq!(pre_awaiting_broadcast_count, post_awaiting_broadcast_count);
		Ok(())
	}
}

pub struct NoopUpgrade;

impl UncheckedOnRuntimeUpgrade for NoopUpgrade {
	fn on_runtime_upgrade() -> Weight {
		Weight::zero()
	}
}
