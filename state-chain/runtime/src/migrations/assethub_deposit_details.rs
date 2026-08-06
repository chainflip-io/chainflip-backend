// Copyright 2026 Chainflip Labs GmbH
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

use crate::*;
use cf_chains::{
	address::EncodedAddress, assets::hub::Asset as HubAsset, Assethub, CcmDepositMetadataChecked,
	CcmDepositMetadataUnchecked, Chain, ChannelRefundParametersForChain, ForeignChainAddress,
};
use cf_primitives::{
	AffiliateShortId, Affiliates, Asset, BasisPoints, Beneficiary, ChannelId, DcaParameters,
	PolkadotBlockNumber, TxId,
};
use cf_runtime_utilities::{log_or_panic, NoopRuntimeUpgrade};
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::ValueQuery, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Twox64Concat,
};
use pallet_cf_ingress_egress::{
	BoostStatusLookup, ChannelActionForDeposit, DepositOrigin, DepositWitness,
	PendingPrewitnessedDeposit, PendingPrewitnessedDepositEntry, VaultDepositWitness,
};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

const OLD_STORAGE_VERSION: u16 = 30;
const NEW_STORAGE_VERSION: u16 = 31;

pub type Migration = (
	VersionedMigration<
		OLD_STORAGE_VERSION,
		NEW_STORAGE_VERSION,
		NoopRuntimeUpgrade,
		pallet_cf_ingress_egress::Pallet<Runtime, EthereumInstance>,
		DbWeight,
	>,
	VersionedMigration<
		OLD_STORAGE_VERSION,
		NEW_STORAGE_VERSION,
		NoopRuntimeUpgrade,
		pallet_cf_ingress_egress::Pallet<Runtime, PolkadotInstance>,
		DbWeight,
	>,
	VersionedMigration<
		OLD_STORAGE_VERSION,
		NEW_STORAGE_VERSION,
		NoopRuntimeUpgrade,
		pallet_cf_ingress_egress::Pallet<Runtime, BitcoinInstance>,
		DbWeight,
	>,
	VersionedMigration<
		OLD_STORAGE_VERSION,
		NEW_STORAGE_VERSION,
		NoopRuntimeUpgrade,
		pallet_cf_ingress_egress::Pallet<Runtime, ArbitrumInstance>,
		DbWeight,
	>,
	VersionedMigration<
		OLD_STORAGE_VERSION,
		NEW_STORAGE_VERSION,
		NoopRuntimeUpgrade,
		pallet_cf_ingress_egress::Pallet<Runtime, SolanaInstance>,
		DbWeight,
	>,
	VersionedMigration<
		OLD_STORAGE_VERSION,
		NEW_STORAGE_VERSION,
		MigrateAssethubToElections,
		pallet_cf_ingress_egress::Pallet<Runtime, AssethubInstance>,
		DbWeight,
	>,
	VersionedMigration<
		OLD_STORAGE_VERSION,
		NEW_STORAGE_VERSION,
		NoopRuntimeUpgrade,
		pallet_cf_ingress_egress::Pallet<Runtime, TronInstance>,
		DbWeight,
	>,
	VersionedMigration<
		OLD_STORAGE_VERSION,
		NEW_STORAGE_VERSION,
		NoopRuntimeUpgrade,
		pallet_cf_ingress_egress::Pallet<Runtime, BscInstance>,
		DbWeight,
	>,
);

pub struct MigrateAssethubToElections;

mod old {
	use cf_primitives::PolkadotBlockNumber;
	use codec::{Decode, Encode};
	use scale_info::TypeInfo;

	use super::*;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct PendingPrewitnessedDeposit {
		pub block_height: PolkadotBlockNumber,
		pub amount: u128,
		pub asset: HubAsset,
		pub deposit_details: u32,
		pub deposit_address: Option<cf_chains::dot::PolkadotAccountId>,
		pub action: ChannelActionForDeposit<AccountId, cf_chains::dot::PolkadotAccountId>,
		pub boost_fee: u16,
		pub channel_id: Option<u64>,
		pub origin: DepositOrigin<Runtime, AssethubInstance>,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct PendingPrewitnessedDepositEntry {
		pub boost_status_lookup: BoostStatusLookup<Runtime, AssethubInstance>,
		pub deposit: PendingPrewitnessedDeposit,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct DepositWitness {
		pub deposit_address: cf_chains::dot::PolkadotAccountId,
		pub asset: HubAsset,
		pub amount: u128,
		pub deposit_details: u32,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct VaultDepositWitness {
		pub input_asset: HubAsset,
		pub deposit_address: Option<cf_chains::dot::PolkadotAccountId>,
		pub channel_id: Option<ChannelId>,
		pub deposit_amount: u128,
		pub deposit_details: u32,
		pub output_asset: Asset,
		pub destination_address: EncodedAddress,
		pub deposit_metadata: Option<CcmDepositMetadataUnchecked<ForeignChainAddress>>,
		pub tx_id: TxId,
		pub broker_fee: Option<Beneficiary<AccountId>>,
		pub affiliate_fees: Affiliates<AffiliateShortId>,
		pub refund_params: ChannelRefundParametersForChain<Assethub>,
		pub dca_params: Option<DcaParameters>,
		pub boost_fee: BasisPoints,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct TransactionRejectionDetails {
		pub deposit_address: Option<cf_chains::dot::PolkadotAccountId>,
		pub refund_address: ForeignChainAddress,
		pub asset: HubAsset,
		pub amount: u128,
		pub deposit_details: u32,
		pub refund_ccm_metadata: Option<CcmDepositMetadataChecked<ForeignChainAddress>>,
	}

	#[storage_alias(pallet_name)]
	pub type PendingPrewitnessedDeposits = StorageMap<
		AssethubIngressEgress,
		Twox64Concat,
		BlockNumber,
		Vec<PendingPrewitnessedDepositEntry>,
		ValueQuery,
	>;

	#[storage_alias(pallet_name)]
	pub type PendingDepositChannelDeposits = StorageMap<
		AssethubIngressEgress,
		Twox64Concat,
		BlockNumber,
		Vec<(DepositWitness, PolkadotBlockNumber)>,
		ValueQuery,
	>;

	#[storage_alias(pallet_name)]
	pub type PendingVaultDeposits = StorageMap<
		AssethubIngressEgress,
		Twox64Concat,
		BlockNumber,
		Vec<(VaultDepositWitness, PolkadotBlockNumber)>,
		ValueQuery,
	>;

	#[storage_alias(pallet_name)]
	pub type ScheduledTransactionsForRejection =
		StorageValue<AssethubIngressEgress, Vec<TransactionRejectionDetails>, ValueQuery>;

	#[storage_alias(pallet_name)]
	pub type FailedRejections =
		StorageValue<AssethubIngressEgress, Vec<TransactionRejectionDetails>, ValueQuery>;
}

fn tx_id(block_number: PolkadotBlockNumber, extrinsic_index: u32) -> TxId {
	TxId { block_number, extrinsic_index }
}

fn migrate_prewitnessed_entry(
	old::PendingPrewitnessedDepositEntry { boost_status_lookup, deposit }: old::PendingPrewitnessedDepositEntry,
) -> PendingPrewitnessedDepositEntry<Runtime, AssethubInstance> {
	PendingPrewitnessedDepositEntry {
		boost_status_lookup,
		deposit: PendingPrewitnessedDeposit {
			block_height: deposit.block_height,
			amount: deposit.amount,
			asset: deposit.asset,
			deposit_details: tx_id(deposit.block_height, deposit.deposit_details),
			deposit_address: deposit.deposit_address,
			action: deposit.action,
			boost_fee: deposit.boost_fee,
			channel_id: deposit.channel_id,
			origin: deposit.origin,
		},
	}
}

fn migrate_deposit_witness(
	old::DepositWitness { deposit_address, asset, amount, deposit_details }: old::DepositWitness,
	block_height: PolkadotBlockNumber,
) -> DepositWitness<Assethub> {
	DepositWitness {
		deposit_address,
		asset,
		amount,
		deposit_details: tx_id(block_height, deposit_details),
	}
}

fn migrate_vault_deposit_witness(
	old: old::VaultDepositWitness,
	block_height: PolkadotBlockNumber,
) -> VaultDepositWitness<Runtime, AssethubInstance> {
	VaultDepositWitness {
		input_asset: old.input_asset,
		deposit_address: old.deposit_address,
		channel_id: old.channel_id,
		deposit_amount: old.deposit_amount,
		deposit_details: tx_id(block_height, old.deposit_details),
		output_asset: old.output_asset,
		destination_address: old.destination_address,
		deposit_metadata: old.deposit_metadata,
		tx_id: old.tx_id,
		broker_fee: old.broker_fee,
		affiliate_fees: old.affiliate_fees,
		refund_params: old.refund_params,
		dca_params: old.dca_params,
		boost_fee: old.boost_fee,
	}
}

impl UncheckedOnRuntimeUpgrade for MigrateAssethubToElections {
	fn on_runtime_upgrade() -> Weight {
		log::info!("Migrating Assethub ingress-egress deposit details");

		pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::mutate(
			|maybe_chain_state| {
				if let Some(chain_state) = maybe_chain_state {
					chain_state.block_height =
						Assethub::block_witness_root(chain_state.block_height);
				} else {
					log_or_panic!("Assethub current chain state must exist");
				}
			},
		);

		let pending_prewitnessed: Vec<_> = old::PendingPrewitnessedDeposits::drain().collect();
		let pending_deposit_channels: Vec<_> =
			old::PendingDepositChannelDeposits::drain().collect();
		let pending_vaults: Vec<_> = old::PendingVaultDeposits::drain().collect();

		let pending_count = pending_prewitnessed
			.len()
			.saturating_add(pending_deposit_channels.len())
			.saturating_add(pending_vaults.len()) as u64;

		for (state_chain_block, entries) in pending_prewitnessed {
			pallet_cf_ingress_egress::PendingPrewitnessedDeposits::<
				Runtime,
				AssethubInstance,
			>::insert(
				state_chain_block,
				entries.into_iter().map(migrate_prewitnessed_entry).collect::<Vec<_>>(),
			);
		}

		for (state_chain_block, entries) in pending_deposit_channels {
			pallet_cf_ingress_egress::PendingDepositChannelDeposits::<
				Runtime,
				AssethubInstance,
			>::insert(
				state_chain_block,
				entries
					.into_iter()
					.map(|(witness, block_height)| {
						(migrate_deposit_witness(witness, block_height), block_height)
					})
					.collect::<Vec<_>>(),
			);
		}

		for (state_chain_block, entries) in pending_vaults {
			pallet_cf_ingress_egress::PendingVaultDeposits::<Runtime, AssethubInstance>::insert(
				state_chain_block,
				entries
					.into_iter()
					.map(|(witness, block_height)| {
						(migrate_vault_deposit_witness(witness, block_height), block_height)
					})
					.collect::<Vec<_>>(),
			);
		}

		let scheduled_rejections = old::ScheduledTransactionsForRejection::take();
		if !scheduled_rejections.is_empty() {
			log_or_panic!("Assethub scheduled transaction rejections must be empty");
		}
		let failed_rejections = old::FailedRejections::take();
		if !failed_rejections.is_empty() {
			log_or_panic!("Assethub failed transaction rejections must be empty");
		}

		DbWeight::get().reads_writes(
			pending_count.saturating_add(3),
			pending_count.saturating_mul(2).saturating_add(3),
		)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		frame_support::ensure!(
			old::ScheduledTransactionsForRejection::get().is_empty(),
			"Assethub scheduled transaction rejections must be empty"
		);
		frame_support::ensure!(
			old::FailedRejections::get().is_empty(),
			"Assethub failed transaction rejections must be empty"
		);

		Ok((
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::get(),
			old::PendingPrewitnessedDeposits::iter().collect::<Vec<_>>(),
			old::PendingDepositChannelDeposits::iter().collect::<Vec<_>>(),
			old::PendingVaultDeposits::iter().collect::<Vec<_>>(),
		)
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		#[expect(clippy::type_complexity)]
		let (old_chain_state, pending_prewitnessed, pending_deposit_channels, pending_vaults): (
			Option<cf_chains::ChainState<Assethub>>,
			Vec<(BlockNumber, Vec<old::PendingPrewitnessedDepositEntry>)>,
			Vec<(BlockNumber, Vec<(old::DepositWitness, PolkadotBlockNumber)>)>,
			Vec<(BlockNumber, Vec<(old::VaultDepositWitness, PolkadotBlockNumber)>)>,
		) = Decode::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::Other("Failed to decode Assethub migration state"))?;

		let mut expected_chain_state = old_chain_state
			.ok_or(DispatchError::Other("Assethub current chain state must exist"))?;
		expected_chain_state.block_height =
			Assethub::block_witness_root(expected_chain_state.block_height);
		frame_support::ensure!(
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::get() ==
				Some(expected_chain_state),
			"Assethub chain state block height was not aligned correctly"
		);

		for (state_chain_block, entries) in pending_prewitnessed {
			let expected = entries.into_iter().map(migrate_prewitnessed_entry).collect::<Vec<_>>();
			frame_support::ensure!(
				pallet_cf_ingress_egress::PendingPrewitnessedDeposits::<
					Runtime,
					AssethubInstance,
				>::get(state_chain_block) == expected,
				"Assethub pending prewitnessed deposits were not migrated correctly"
			);
		}

		for (state_chain_block, entries) in pending_deposit_channels {
			let expected = entries
				.into_iter()
				.map(|(witness, block_height)| {
					(migrate_deposit_witness(witness, block_height), block_height)
				})
				.collect::<Vec<_>>();
			frame_support::ensure!(
				pallet_cf_ingress_egress::PendingDepositChannelDeposits::<
					Runtime,
					AssethubInstance,
				>::get(state_chain_block) == expected,
				"Assethub pending deposit channel deposits were not migrated correctly"
			);
		}

		for (state_chain_block, entries) in pending_vaults {
			let expected = entries
				.into_iter()
				.map(|(witness, block_height)| {
					(migrate_vault_deposit_witness(witness, block_height), block_height)
				})
				.collect::<Vec<_>>();
			frame_support::ensure!(
				pallet_cf_ingress_egress::PendingVaultDeposits::<Runtime, AssethubInstance>::get(
					state_chain_block
				) == expected,
				"Assethub pending vault deposits were not migrated correctly"
			);
		}

		frame_support::ensure!(
			pallet_cf_ingress_egress::ScheduledTransactionsForRejection::<
				Runtime,
				AssethubInstance,
			>::get()
			.is_empty(),
			"Assethub scheduled transaction rejections must remain empty"
		);
		frame_support::ensure!(
			pallet_cf_ingress_egress::FailedRejections::<Runtime, AssethubInstance>::get()
				.is_empty(),
			"Assethub failed transaction rejections must remain empty"
		);

		Ok(())
	}
}
