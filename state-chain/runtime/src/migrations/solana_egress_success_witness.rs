use crate::{
	chainflip::solana_elections::SolanaElectoralSystem, Runtime, SolEnvironment, SolanaElections,
	Weight,
};
use cf_chains::{instances::SolanaInstance, sol::api::SolanaApi, TransferAssetParams};
use frame_support::traits::OnRuntimeUpgrade;

use pallet_cf_elections::{
	access_impls::ElectionAccess, electoral_system::ElectionWriteAccess,
	electoral_systems::composite::tuple_7_impls::CompositeElectionIdentifierExtra,
};

use cf_chains::sol::{SolAddress, SolPubkey, SolTransaction};

#[allow(unused_imports)]
mod try_runtime_import {
	pub use codec::{Decode, Encode};
	pub use sp_runtime::DispatchError;
	pub use sp_std::vec::Vec;
}
#[cfg(feature = "try-runtime")]
use try_runtime_import::*;

pub struct SolanaEgressSuccessWitnessMigration;

mod old {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[derive(Encode, Decode, TypeInfo)]

	pub struct SolanaApi<Environment: 'static> {
		pub call_type: SolanaTransactionType,
		pub transaction: SolTransaction,
		pub signer: Option<SolAddress>,
		pub _phantom: PhantomData<Environment>,
	}
	#[derive(Encode, Decode, TypeInfo)]
	pub enum SolanaTransactionType {
		BatchFetch,
		Transfer,
		RotateAggKey,
		CcmTransfer,
		SetGovKeyWithAggKey,
	}

	pub fn to_new_sol_transaction_type(
		old: SolanaTransactionType,
	) -> cf_chains::sol::api::SolanaTransactionType {
		// Use an invalid address and amount of 0 as fallback.
		// Only CCMs submitted after the runtime upgrade support Fallback.
		match old {
			SolanaTransactionType::BatchFetch =>
				cf_chains::sol::api::SolanaTransactionType::BatchFetch,
			SolanaTransactionType::Transfer => cf_chains::sol::api::SolanaTransactionType::Transfer,
			SolanaTransactionType::RotateAggKey =>
				cf_chains::sol::api::SolanaTransactionType::RotateAggKey,
			SolanaTransactionType::CcmTransfer =>
				cf_chains::sol::api::SolanaTransactionType::CcmTransfer {
					fallback: TransferAssetParams {
						asset: cf_chains::assets::sol::Asset::Sol,
						to: SolPubkey([0x00; 32]).into(),
						amount: Default::default(),
					},
				},
			SolanaTransactionType::SetGovKeyWithAggKey =>
				cf_chains::sol::api::SolanaTransactionType::SetGovKeyWithAggKey,
		}
	}
}

impl OnRuntimeUpgrade for SolanaEgressSuccessWitnessMigration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🥮 Running Solana Success witnessing migration.");

		// Clear Solana's egress-success votes.
		let _ =
			SolanaElections::with_electoral_access_and_identifiers(|_, election_identifiers| {
				SolanaElectoralSystem::with_identifiers(
					election_identifiers,
					|election_identifiers| {
						// Extract egress-success elections only.
						let (_, _, _, _, egress_success_election_identifiers, ..) =
							election_identifiers;
						egress_success_election_identifiers.into_iter().for_each(
							|election_identifier| {
								ElectionAccess::<Runtime, SolanaInstance>::new(
									election_identifier.with_extra(
										CompositeElectionIdentifierExtra::<
											(),
											(),
											u32,
											(),
											(),
											(),
											(),
										>::EE(()),
									),
								)
								.clear_votes()
							},
						);
						Ok(())
					},
				)
			});

		// Solana ApiCalls are stored in the broadcaster pallets. Add empty "fallback" info for
		// existing Ccms.
		pallet_cf_broadcast::PendingApiCalls::<Runtime, SolanaInstance>::translate_values::<
			old::SolanaApi<SolEnvironment>,
			_,
		>(
			|old::SolanaApi::<SolEnvironment> {
			     call_type: old_call_type,
			     transaction,
			     signer,
			     _phantom,
			 }| {
				Some(SolanaApi::<SolEnvironment> {
					call_type: old::to_new_sol_transaction_type(old_call_type),
					transaction,
					signer,
					_phantom,
				})
			},
		);
		log::info!("Solana elections cleared, storage in Broadcaster pallet migrated.");
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(Encode::encode(
			&(pallet_cf_broadcast::PendingApiCalls::<Runtime, SolanaInstance>::iter_keys().count()
				as u32),
		))
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pending_solana_calls = u32::decode(&mut &state[..]).unwrap_or_default();

		assert_eq!(
			pending_solana_calls,
			pallet_cf_broadcast::PendingApiCalls::<Runtime, SolanaInstance>::iter_keys().count()
				as u32
		);

		Ok(())
	}
}
