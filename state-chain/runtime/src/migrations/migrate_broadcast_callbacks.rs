// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! Migrates old broadcast callback storage items (`RequestSuccessCallbacks` /
//! `RequestFailureCallbacks`) from `pallet_cf_broadcast` into
//! `pallet_cf_ingress_egress::BroadcastActions`.
//!
//! This runs at the runtime level (before `PalletMigrations`) because it needs access to both
//! pallets, and the broadcast pallet does not depend on the ingress-egress pallet.

use crate::*;
use cf_chains::Chain;
use cf_primitives::BroadcastId;
use codec::{Decode, Encode};
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade,
	weights::Weight,
};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
use sp_std::marker::PhantomData;

type BroadcastMigration<I> = VersionedMigration<
	13,
	14,
	BroadcastCallbacksMigration<I>,
	pallet_cf_broadcast::Pallet<Runtime, I>,
	<Runtime as frame_system::Config>::DbWeight,
>;

pub type Migration = (
	BroadcastMigration<EthereumInstance>,
	BroadcastMigration<PolkadotInstance>,
	BroadcastMigration<BitcoinInstance>,
	BroadcastMigration<ArbitrumInstance>,
	BroadcastMigration<SolanaInstance>,
	BroadcastMigration<AssethubInstance>,
);

pub struct BroadcastCallbacksMigration<I>(PhantomData<I>);

type ChainAccountOf<T, I> =
	<<T as cf_traits::ChainflipWithTargetChain<I>>::TargetChain as Chain>::ChainAccount;

mod old {
	use super::*;

	#[derive(Encode, Decode)]
	#[expect(clippy::enum_variant_names)]
	pub enum OldRuntimeCall<ChainAccount: Encode + Decode> {
		#[codec(index = 32)]
		EthereumIngressEgress(OldIngressEgressCall<ChainAccount>),
		#[codec(index = 33)]
		PolkadotIngressEgress(OldIngressEgressCall<ChainAccount>),
		#[codec(index = 34)]
		BitcoinIngressEgress(OldIngressEgressCall<ChainAccount>),
		#[codec(index = 40)]
		ArbitrumIngressEgress(OldIngressEgressCall<ChainAccount>),
		#[codec(index = 44)]
		SolanaIngressEgress(OldIngressEgressCall<ChainAccount>),
		#[codec(index = 51)]
		AssethubIngressEgress(OldIngressEgressCall<ChainAccount>),
	}

	impl<ChainAccount: Encode + Decode> OldRuntimeCall<ChainAccount> {
		pub fn into_inner(self) -> OldIngressEgressCall<ChainAccount> {
			match self {
				Self::EthereumIngressEgress(call) |
				Self::PolkadotIngressEgress(call) |
				Self::BitcoinIngressEgress(call) |
				Self::ArbitrumIngressEgress(call) |
				Self::SolanaIngressEgress(call) |
				Self::AssethubIngressEgress(call) => call,
			}
		}
	}

	/// Minimal decoder for the inner ingress-egress pallet call.
	#[derive(Encode, Decode)]
	pub enum OldIngressEgressCall<ChainAccount: Encode + Decode> {
		#[codec(index = 0)]
		FinaliseIngress(Vec<ChainAccount>),
		#[codec(index = 5)]
		CcmBroadcastFailed(BroadcastId),
	}

	#[frame_support::storage_alias]
	pub type RequestSuccessCallbacks<T: pallet_cf_broadcast::Config<I>, I: 'static> = StorageMap<
		pallet_cf_broadcast::Pallet<T, I>,
		Twox64Concat,
		BroadcastId,
		OldRuntimeCall<ChainAccountOf<T, I>>,
	>;

	#[frame_support::storage_alias]
	pub type RequestFailureCallbacks<T: pallet_cf_broadcast::Config<I>, I: 'static> = StorageMap<
		pallet_cf_broadcast::Pallet<T, I>,
		Twox64Concat,
		BroadcastId,
		OldRuntimeCall<ChainAccountOf<T, I>>,
	>;
}

impl<I: 'static> UncheckedOnRuntimeUpgrade for BroadcastCallbacksMigration<I>
where
	Runtime: pallet_cf_broadcast::Config<I> + pallet_cf_ingress_egress::Config<I>,
{
	fn on_runtime_upgrade() -> Weight {
		migrate::<I>();
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let success_callbacks = old::RequestSuccessCallbacks::<Runtime, I>::iter().count() as u64;

		Ok(success_callbacks.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let success_callbacks = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode pre-migration state"))?;

		let post_finalise_fetch_actions = broadcast_action_counts::<I>();

		frame_support::ensure!(
			post_finalise_fetch_actions == success_callbacks,
			DispatchError::from("FinaliseFetch action count mismatch after migration"),
		);

		Ok(())
	}
}

#[cfg(feature = "try-runtime")]
fn broadcast_action_counts<I: 'static>() -> u64
where
	Runtime: pallet_cf_broadcast::Config<I> + pallet_cf_ingress_egress::Config<I>,
{
	let mut finalise_fetch_actions = 0u64;

	for (_, action) in pallet_cf_ingress_egress::BroadcastActions::<Runtime, I>::iter() {
		if let pallet_cf_ingress_egress::BroadcastAction::FinaliseFetch(_) = action {
			finalise_fetch_actions += 1;
		}
	}

	finalise_fetch_actions
}

fn migrate<I: 'static>()
where
	Runtime: pallet_cf_broadcast::Config<I> + pallet_cf_ingress_egress::Config<I>,
{
	let mut success_count = 0u32;
	let chain_name = core::any::type_name::<I>();

	for (broadcast_id, callback) in old::RequestSuccessCallbacks::<Runtime, I>::drain() {
		match callback.into_inner() {
			old::OldIngressEgressCall::FinaliseIngress(addresses) => {
				pallet_cf_ingress_egress::BroadcastActions::<Runtime, I>::insert(
					broadcast_id,
					pallet_cf_ingress_egress::BroadcastAction::FinaliseFetch(addresses),
				);
				success_count += 1;
			},
			_ => {
				log::warn!(
					"🧹 {chain_name}: Unexpected success callback variant for broadcast {broadcast_id}",
				);
			},
		}
	}

	let failure_count = old::RequestFailureCallbacks::<Runtime, I>::clear(u32::MAX, None).unique;

	log::info!(
		"🔄 {chain_name}: Migrated {success_count} success callbacks to BroadcastActions, and removed {failure_count} failure callbacks.",
	);
}

#[cfg(test)]
mod tests {
	use super::old::{OldIngressEgressCall, OldRuntimeCall};
	use codec::Decode;

	// Hex value coming from mainnet storage
	#[test]
	fn decodes_real_on_chain_callbacks() {
		use hex_literal::hex;
		use sp_core::H160;

		// {
		//   args: {
		//     broadcast_id: 96,903
		//   }
		//   method: ccmBroadcastFailed
		//   section: ethereumIngressEgress
		// }
		let failure_bytes = hex!("2005877a0100");
		let decoded = OldRuntimeCall::<H160>::decode(&mut &failure_bytes[..]).unwrap();
		match decoded {
			OldRuntimeCall::EthereumIngressEgress(call) => match call {
				OldIngressEgressCall::FinaliseIngress(_) => {
					panic!("Expected CcmBroadcastFailed");
				},
				OldIngressEgressCall::CcmBroadcastFailed(broadcast_id) => {
					assert_eq!(broadcast_id, 96903);
				},
			},
			_ => panic!("Expected EthereumIngressEgress"),
		}

		// {
		//   args: {
		//     addresses: [
		//       0x026ffcf2e279559a4f0c60433168e92ce7df2b7f
		//     ]
		//   }
		//   method: finaliseIngress
		//   section: ethereumIngressEgress
		// }
		let success_bytes = hex!("200004026ffcf2e279559a4f0c60433168e92ce7df2b7f");
		let decoded = OldRuntimeCall::<H160>::decode(&mut &success_bytes[..]).unwrap();
		match decoded {
			OldRuntimeCall::EthereumIngressEgress(call) => match call {
				OldIngressEgressCall::FinaliseIngress(addresses) => {
					assert_eq!(
						addresses,
						vec![H160(hex!("026ffcf2e279559a4f0c60433168e92ce7df2b7f"))]
					);
				},
				OldIngressEgressCall::CcmBroadcastFailed(_) => {
					panic!("Expected FinaliseIngress");
				},
			},
			_ => panic!("Expected EthereumIngressEgress"),
		}
	}
}
