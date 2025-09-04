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

use crate::{
	chainflip::{
		self,
		bitcoin_block_processor::BtcEvent,
		bitcoin_elections::{
			BitcoinBlockHeightWitnesserES, BitcoinDepositChannelWitnessingES,
			BitcoinEgressWitnessingES, BitcoinFeeSettings, BitcoinLiveness,
			BitcoinVaultDepositWitnessing, BitcoinVaultDepositWitnessingES,
		},
		elections::TypesFor,
	},
	BitcoinInstance, Runtime,
};
use cf_chains::{btc::BtcAmount, refund_parameters::ChannelRefundParameters, Chain};
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::{
	migrations::VersionedMigration, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};
use pallet_cf_elections::{ElectoralSystemTypes, Pallet};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

pub type Migration = (
	VersionedMigration<
		6,
		7,
		BitcoinElectionMigration,
		pallet_cf_elections::Pallet<Runtime, BitcoinInstance>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<7, Pallet<Runtime, BitcoinInstance>>,
);

pub struct BitcoinElectionMigration;

mod old {

	use super::*;
	use cf_chains::btc::{self};
	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};
	use pallet_cf_elections::{
		electoral_systems::{
			block_witnesser::{primitives::CompactHeightTracker, state_machine::BWElectionType},
			state_machine::core::hook_test_utils::EmptyHook,
		},
		Config, UniqueMonotonicIdentifier,
	};

	use sp_std::collections::btree_map::BTreeMap;

	#[derive(codec::Encode, codec::Decode, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
	pub struct ChannelRefundParameters {
		pub retry_duration: cf_primitives::BlockNumber,
		pub refund_address: <cf_chains::Bitcoin as Chain>::ChainAccount,
		pub min_price: cf_primitives::Price,
		// no refund_ccm_metadata
	}

	#[derive(codec::Encode, codec::Decode, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
	pub struct VaultDepositWitness {
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
		pub refund_params: ChannelRefundParameters,
		pub dca_params: Option<crate::DcaParameters>,
		pub boost_fee: cf_primitives::BasisPoints,
	}

	#[derive(codec::Encode, codec::Decode, Clone, Debug)]
	pub struct BlockProcessingInfo {
		pub block_data: Vec<VaultDepositWitness>,
		pub next_age_to_process: u32,
		pub safety_margin: u32,
	}

	#[derive(codec::Encode, codec::Decode, Clone, Debug)]
	pub struct BlockProcessor {
		pub blocks_data: BTreeMap<btc::BlockNumber, BlockProcessingInfo>,
		pub processed_events: BTreeMap<
			crate::chainflip::bitcoin_block_processor::BtcEvent<VaultDepositWitness>,
			btc::BlockNumber,
		>,
		pub rules: TypesFor<BitcoinVaultDepositWitnessing>,
		pub execute: TypesFor<BitcoinVaultDepositWitnessing>,
		pub debug_events: EmptyHook,
	}

	#[derive(codec::Encode, codec::Decode, Clone, Debug)]
	pub struct OptimisticBlock {
		pub hash: btc::Hash,
		pub data: Vec<VaultDepositWitness>,
	}

	#[derive(codec::Encode, codec::Decode, Clone, Debug)]
	pub struct ElectionTracker {
		pub seen_heights_below: btc::BlockNumber,
		pub highest_ever_ongoing_election: btc::BlockNumber,
		pub queued_hash_elections: BTreeMap<btc::BlockNumber, btc::Hash>,
		pub queued_safe_elections: CompactHeightTracker<btc::BlockNumber>,
		pub ongoing:
			BTreeMap<btc::BlockNumber, BWElectionType<TypesFor<BitcoinVaultDepositWitnessing>>>,
		pub optimistic_block_cache: BTreeMap<btc::BlockNumber, OptimisticBlock>,
		pub debug_events: EmptyHook,
		pub safety_buffer: u32,
	}

	#[derive(codec::Encode, codec::Decode, Clone, Debug)]
	pub struct BlockWitnesserState {
		pub elections: ElectionTracker,
		pub generate_election_properties_hook: TypesFor<BitcoinVaultDepositWitnessing>,
		pub safemode_enabled: TypesFor<BitcoinVaultDepositWitnessing>,
		pub block_processor: BlockProcessor,
		pub processed_up_to: EmptyHook,
	}

	pub type CompositeElectoralUnsynchronisedState = (
		<BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		<BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		BlockWitnesserState,
		<BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		BtcAmount,
		<BitcoinLiveness as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
	);

	pub type CompositeElectoralUnsynchronisedSettings = (
		<BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		<BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		<BitcoinVaultDepositWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		<BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		BtcAmount,
		<BitcoinLiveness as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
	);

	pub type CompositeElectoralSettings = (
		<BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		<BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		<BitcoinVaultDepositWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		<BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		(),
		<BitcoinLiveness as ElectoralSystemTypes>::ElectoralSettings,
	);

	#[frame_support::storage_alias]
	pub type ElectoralUnsynchronisedState<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, CompositeElectoralUnsynchronisedState, OptionQuery>;

	#[frame_support::storage_alias]
	pub type ElectoralUnsynchronisedSettings<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, CompositeElectoralUnsynchronisedSettings, OptionQuery>;

	#[frame_support::storage_alias]
	pub type ElectoralSettings<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		CompositeElectoralSettings,
		OptionQuery,
	>;
}

impl UncheckedOnRuntimeUpgrade for BitcoinElectionMigration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let old_vault_state = old::ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::get()
			.expect("Should contain something")
			.2;

		let no_of_blocks_data_items_pre_upgrade: u64 = old_vault_state
			.block_processor
			.blocks_data
			.values()
			.map(|block_info| block_info.block_data.len() as u64)
			.sum();
		let no_of_reorg_events_items_pre_upgrade: u64 =
			old_vault_state.block_processor.processed_events.len() as u64;
		let no_of_optimistic_blocks_items_pre_upgrade: u64 = old_vault_state
			.elections
			.optimistic_block_cache
			.values()
			.map(|opti_block| opti_block.data.len() as u64)
			.sum::<u64>();
		Ok(codec::Encode::encode(&(
			no_of_blocks_data_items_pre_upgrade,
			no_of_reorg_events_items_pre_upgrade,
			no_of_optimistic_blocks_items_pre_upgrade,
		)))
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!("🍩 Migration for BTC Election started");
		let optional_storage = old::ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::get();
		let (a, b, old_vault_state, d, current_btc_fee, f) =
			optional_storage.expect("Should contain something");

		let new_block_processor = {
			let old_blocks_data = old_vault_state.block_processor.blocks_data;
			let old_processed_events = old_vault_state.block_processor.processed_events;
			let new_blocks_data = old_blocks_data
				.into_iter()
				.map(|(block_number, old_info)| {
					let new_block_data: Vec<pallet_cf_ingress_egress::VaultDepositWitness<Runtime, BitcoinInstance>> =
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
									max_oracle_price_slippage: None,
								};
								pallet_cf_ingress_egress::VaultDepositWitness {
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

					let new_info = pallet_cf_elections::electoral_systems::block_witnesser::block_processor::BlockProcessingInfo {
						block_data: new_block_data,
						next_age_to_process: old_info.next_age_to_process,
						safety_margin: old_info.safety_margin,
					};

					(block_number, new_info)
				})
				.collect();

			let new_processed_events: BTreeMap<
				BtcEvent<pallet_cf_ingress_egress::VaultDepositWitness<Runtime, BitcoinInstance>>,
				u64,
			> = {
				old_processed_events
					.into_iter()
					.map(|(old_event, block)| match old_event {
						chainflip::bitcoin_block_processor::BtcEvent::Witness(old_witness) => {
							let old_refund = old_witness.refund_params;
							let new_refund_params = ChannelRefundParameters {
								retry_duration: old_refund.retry_duration,
								refund_address: old_refund.refund_address,
								min_price: old_refund.min_price,
								refund_ccm_metadata: None,
								max_oracle_price_slippage: None,
							};
							(
								chainflip::bitcoin_block_processor::BtcEvent::Witness(
									pallet_cf_ingress_egress::VaultDepositWitness::<
										Runtime,
										BitcoinInstance,
									> {
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
									},
								),
								block,
							)
						},
						chainflip::bitcoin_block_processor::BtcEvent::PreWitness(old_witness) => {
							let old_refund = old_witness.refund_params;
							let new_refund_params = ChannelRefundParameters {
								retry_duration: old_refund.retry_duration,
								refund_address: old_refund.refund_address,
								min_price: old_refund.min_price,
								refund_ccm_metadata: None,
								max_oracle_price_slippage: None,
							};
							(
								chainflip::bitcoin_block_processor::BtcEvent::PreWitness(
									pallet_cf_ingress_egress::VaultDepositWitness::<
										Runtime,
										BitcoinInstance,
									> {
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
									},
								),
								block,
							)
						},
					})
					.collect()
			};
			pallet_cf_elections::electoral_systems::block_witnesser::block_processor::BlockProcessor {
				blocks_data: new_blocks_data,
				processed_events: new_processed_events,
				rules: old_vault_state.block_processor.rules,
				execute: old_vault_state.block_processor.execute,
				debug_events: old_vault_state.block_processor.debug_events,
			}
		};

		let new_election_tracker = {
			let old_election_tracker = old_vault_state.elections;
			let new_optimistic_block_cache = old_election_tracker
				.optimistic_block_cache
				.into_iter()
				.map(|(height, block_data)| {
					(
						height,
						pallet_cf_elections::electoral_systems::block_witnesser::primitives::OptimisticBlock {
							hash: block_data.hash,
							data: block_data
								.data
								.into_iter()
								.map(|old_witness| {
									let old_refund = old_witness.refund_params;
									let new_refund_params = ChannelRefundParameters {
										retry_duration: old_refund.retry_duration,
										refund_address: old_refund.refund_address,
										min_price: old_refund.min_price,
										refund_ccm_metadata: None,
										max_oracle_price_slippage: None,
									};
									pallet_cf_ingress_egress::VaultDepositWitness {
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
								.collect(),
						},
					)
				})
				.collect();
			pallet_cf_elections::electoral_systems::block_witnesser::primitives::ElectionTracker {
				seen_heights_below: old_election_tracker.seen_heights_below,
				highest_ever_ongoing_election: old_election_tracker.highest_ever_ongoing_election,
				queued_hash_elections: old_election_tracker.queued_hash_elections,
				queued_safe_elections: old_election_tracker.queued_safe_elections,
				ongoing: old_election_tracker.ongoing,
				optimistic_block_cache: new_optimistic_block_cache,
				debug_events: old_election_tracker.debug_events,
				safety_buffer: old_election_tracker.safety_buffer,
			}
		};

		let new_vault_state = pallet_cf_elections::electoral_systems::block_witnesser::state_machine::BlockWitnesserState {
			elections: new_election_tracker,
			generate_election_properties_hook: old_vault_state.generate_election_properties_hook,
			safemode_enabled: old_vault_state.safemode_enabled,
			block_processor: new_block_processor,
			processed_up_to: old_vault_state.processed_up_to,
		};

		pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::put((
			a,
			b,
			new_vault_state,
			d,
			(current_btc_fee, 0), // last election concluded at block 0
			f,
		));

		// migrating unsynchronised settings
		{
			let optional_storage =
				old::ElectoralUnsynchronisedSettings::<Runtime, BitcoinInstance>::get();
			let (a, b, c, d, _old_settings_amount, f) =
				optional_storage.expect("Should contain something");

			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime, BitcoinInstance>::put(
				(
					a, b, c, d, 20u32, // fee witnessing should happen every 20 SC blocks
					f,
				),
			);
		}

		// migrating settings
		{
			let settings_entries: Vec<_> =
				old::ElectoralSettings::<Runtime, BitcoinInstance>::drain().collect();

			for (id, (a, b, c, d, (), f)) in settings_entries {
				pallet_cf_elections::ElectoralSettings::<Runtime, BitcoinInstance>::insert(
					id,
					(a, b, c, d, BitcoinFeeSettings { tx_sample_count_per_mempool_block: 20 }, f),
				);
			}
		}

		log::info!("🍩 Migration for BTC Election completed");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		use core::assert;
		use sp_runtime::TryRuntimeError;

		let (
			no_of_blocks_data_items_pre_upgrade,
			no_of_reorg_events_items_pre_upgrade,
			no_of_optimistic_blocks_items_pre_upgrade,
		): (u64, u64, u64) = codec::Decode::decode(&mut state.as_slice())
			.map_err(|_| TryRuntimeError::from("Failed to decode state"))?;

		let current_state =
			pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::get()
				.unwrap()
				.2;
		assert!(
			no_of_blocks_data_items_pre_upgrade ==
				current_state
					.block_processor
					.blocks_data
					.values()
					.map(|block_info| block_info.block_data.len() as u64)
					.sum::<u64>()
		);

		assert!(
			no_of_reorg_events_items_pre_upgrade ==
				current_state.block_processor.processed_events.len() as u64
		);

		assert!(
			no_of_optimistic_blocks_items_pre_upgrade ==
				current_state
					.elections
					.optimistic_block_cache
					.values()
					.map(|opti_block| { opti_block.data.len() as u64 })
					.sum::<u64>()
		);

		// -----------------
		// checks for fee election migration
		let current_state =
			pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::get()
				.unwrap()
				.4;

		assert_eq!(current_state.1, 0);

		let current_unsynchronised_settings =
			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime, BitcoinInstance>::get()
				.unwrap()
				.4;

		assert_eq!(current_unsynchronised_settings, 10);

		pallet_cf_elections::ElectoralSettings::<Runtime, BitcoinInstance>::iter().for_each(
			|(_id, settings)| {
				assert_eq!(
					settings.4,
					BitcoinFeeSettings { tx_sample_count_per_mempool_block: 20 }
				);
			},
		);

		Ok(())
	}
}
