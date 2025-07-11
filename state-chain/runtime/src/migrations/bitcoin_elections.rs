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

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_elections::{
	electoral_systems::{
		block_witnesser::block_processor::BlockProcessingInfo,
		state_machine::core::hook_test_utils::EmptyHook,
	},
	ElectoralUnsynchronisedState,
};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
use sp_std::vec::Vec;

use crate::{
	chainflip,
	chainflip::{bitcoin_elections::BitcoinVaultDepositWitnessing, elections::TypesFor},
	BitcoinInstance, Runtime,
};
use cf_chains::{refund_parameters::ChannelRefundParameters, Chain};
use pallet_cf_elections::electoral_systems::block_witnesser::{
	block_processor::BlockProcessor, state_machine::BlockWitnesserState,
};
use pallet_cf_ingress_egress::VaultDepositWitness;

pub type Migration = (BitcoinElectionMigration,);

pub struct BitcoinElectionMigration;

mod old {
	use crate::chainflip::bitcoin_elections::{
		BitcoinBlockHeightWitnesserES, BitcoinDepositChannelWitnessingES,
		BitcoinEgressWitnessingES, BitcoinFeeTracking, BitcoinLiveness,
	};

	use super::*;
	use cf_chains::btc;
	use pallet_cf_elections::ElectoralSystemTypes;
	use sp_std::collections::btree_map::BTreeMap;

	#[derive(codec::Decode)]
	pub struct OldChannelRefundParameters {
		pub retry_duration: cf_primitives::BlockNumber,
		pub refund_address: <cf_chains::Bitcoin as Chain>::ChainAccount,
		pub min_price: cf_primitives::Price,
		// no refund_ccm_metadata
	}

	#[derive(codec::Decode)]
	pub struct OldVaultDepositWitness {
		pub input_asset: pallet_cf_ingress_egress::TargetChainAsset<Runtime, BitcoinInstance>,
		pub deposit_address:
			Option<pallet_cf_ingress_egress::TargetChainAccount<Runtime, BitcoinInstance>>,
		pub channel_id: Option<cf_primitives::ChannelId>,
		pub deposit_amount: <cf_chains::Bitcoin as Chain>::ChainAmount,
		pub deposit_details: <cf_chains::Bitcoin as Chain>::DepositDetails,
		pub output_asset: crate::Asset,
		pub destination_address: crate::EncodedAddress,
		pub deposit_metadata:
			Option<cf_chains::CcmDepositMetadataUnchecked<cf_chains::ForeignChainAddress>>,
		pub tx_id: pallet_cf_ingress_egress::TransactionInIdFor<Runtime, BitcoinInstance>,
		pub broker_fee:
			Option<cf_primitives::Beneficiary<<Runtime as frame_system::Config>::AccountId>>,
		pub affiliate_fees: crate::Affiliates<cf_primitives::AffiliateShortId>,
		pub refund_params: OldChannelRefundParameters,
		pub dca_params: Option<crate::DcaParameters>,
		pub boost_fee: cf_primitives::BasisPoints,
	}

	#[derive(codec::Decode)]
	pub struct OldBlockProcessingInfo {
		pub block_data: Vec<OldVaultDepositWitness>,
		pub next_age_to_process: u32,
		pub safety_margin: u32,
	}

	#[derive(codec::Decode)]
	pub struct OldBlockProcessor {
		pub blocks_data: BTreeMap<btc::BlockNumber, OldBlockProcessingInfo>,
		pub processed_events: BTreeMap<
			crate::chainflip::bitcoin_block_processor::BtcEvent<
				pallet_cf_ingress_egress::VaultDepositWitness<Runtime, BitcoinInstance>,
			>,
			btc::BlockNumber,
		>,
		pub rules: TypesFor<chainflip::bitcoin_elections::BitcoinVaultDepositWitnessing>,
		pub execute: TypesFor<chainflip::bitcoin_elections::BitcoinVaultDepositWitnessing>,
		pub debug_events: EmptyHook,
	}

	#[derive(codec::Decode)]
	pub struct OldBlockWitnesserState {
		pub elections:
			pallet_cf_elections::electoral_systems::block_witnesser::primitives::ElectionTracker<
				crate::chainflip::elections::TypesFor<BitcoinVaultDepositWitnessing>,
			>,
		pub generate_election_properties_hook:
			TypesFor<chainflip::bitcoin_elections::BitcoinVaultDepositWitnessing>,
		pub safemode_enabled: TypesFor<chainflip::bitcoin_elections::BitcoinVaultDepositWitnessing>,
		pub block_processor: OldBlockProcessor,
		pub processed_up_to: EmptyHook,
	}

	pub type OldCompositeElectoralUnsynchronisedState = (
		<BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		<BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		OldBlockWitnesserState,
		<BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		<BitcoinFeeTracking as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		<BitcoinLiveness as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
	);
}

impl OnRuntimeUpgrade for BitcoinElectionMigration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		use pallet_cf_elections::ElectoralUnsynchronisedState;

		let key = ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::hashed_key();
		let maybe_bytes: Vec<u8> = frame_support::storage::unhashed::get(&key).unwrap();
		let old_vault_state = {
			let (_, _, old_vault_state, _, _, _): old::OldCompositeElectoralUnsynchronisedState =
				codec::Decode::decode(&mut &maybe_bytes[..])
					.expect("Failed to decode old composite state");
			old_vault_state
		};

		let no_of_items_pre_upgrade: u64 = old_vault_state
			.block_processor
			.blocks_data
			.values()
			.map(|block_info| block_info.block_data.len() as u64)
			.sum();
		Ok(codec::Encode::encode(&no_of_items_pre_upgrade))
	}

	fn on_runtime_upgrade() -> Weight {
		let key = ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::hashed_key();
		let maybe_bytes: Vec<u8> = frame_support::storage::unhashed::get(&key).unwrap();
		let old_vault_state = {
			let (_, _, old_vault_state, _, _, _): old::OldCompositeElectoralUnsynchronisedState =
				codec::Decode::decode(&mut &maybe_bytes[..])
					.expect("Failed to decode old composite state");
			old_vault_state
		};

		// Migrate the block_processor
		let new_block_processor = {
			let old_blocks_data = old_vault_state.block_processor.blocks_data;
			let new_blocks_data = old_blocks_data
				.into_iter()
				.map(|(block_number, old_info)| {
					// Map each OldVaultDepositWitness in the Vec to a new VaultDepositWitness
					let new_block_data: Vec<VaultDepositWitness<Runtime, BitcoinInstance>> =
						old_info
							.block_data
							.into_iter()
							.map(|old_witness| {
								let old_refund = old_witness.refund_params;
								let new_refund_params = ChannelRefundParameters {
									retry_duration: old_refund.retry_duration,
									refund_address: old_refund.refund_address,
									min_price: old_refund.min_price,
									refund_ccm_metadata: None,
								};
								VaultDepositWitness {
									input_asset: old_witness.input_asset,
									deposit_address: old_witness.deposit_address,
									channel_id: old_witness.channel_id,
									deposit_amount: old_witness.deposit_amount,
									deposit_details: old_witness.deposit_details,
									output_asset: old_witness.output_asset,
									destination_address: old_witness.destination_address,
									deposit_metadata: old_witness.deposit_metadata,
									tx_id: old_witness.tx_id,
									broker_fee: old_witness.broker_fee,
									affiliate_fees: old_witness.affiliate_fees,
									refund_params: new_refund_params,
									dca_params: old_witness.dca_params,
									boost_fee: old_witness.boost_fee,
								}
							})
							.collect();

					let new_info = BlockProcessingInfo {
						block_data: new_block_data,
						next_age_to_process: old_info.next_age_to_process,
						safety_margin: old_info.safety_margin,
					};

					(block_number, new_info)
				})
				.collect();

			BlockProcessor {
				blocks_data: new_blocks_data,
				processed_events: old_vault_state.block_processor.processed_events,
				rules: old_vault_state.block_processor.rules,
				execute: old_vault_state.block_processor.execute,
				debug_events: old_vault_state.block_processor.debug_events,
			}
		};

		let new_vault_state = BlockWitnesserState {
			elections: old_vault_state.elections,
			generate_election_properties_hook: old_vault_state.generate_election_properties_hook,
			safemode_enabled: old_vault_state.safemode_enabled,
			block_processor: new_block_processor,
			processed_up_to: old_vault_state.processed_up_to,
		};

		let mut composite =
			ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::get().unwrap();
		composite.2 = new_vault_state;
		ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::put(composite);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		use core::assert;
		use sp_runtime::TryRuntimeError;

		let no_of_items_pre_upgrade: u64 = codec::Decode::decode(&mut state.as_slice())
			.map_err(|_| TryRuntimeError::from("Failed to decode state"))?;

		assert!(
			no_of_items_pre_upgrade ==
				ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::get()
					.unwrap()
					.2
					.block_processor
					.blocks_data
					.values()
					.map(|block_info| block_info.block_data.len() as u64)
					.sum::<u64>()
		);

		Ok(())
	}
}
