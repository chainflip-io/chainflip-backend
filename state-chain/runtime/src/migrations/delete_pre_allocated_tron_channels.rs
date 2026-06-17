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

use crate::*;
use cf_chains::{
	address::AddressDerivationApi, assets::tron::Asset as TronAsset, evm::DeploymentStatus,
	tron::TronAddress, DepositChannel, Tron,
};
use cf_runtime_utilities::log_or_panic;
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

pub struct DeletePreAllocatedTronChannels;

fn maybe_update_channel(mut channel: DepositChannel<Tron>) -> Option<DepositChannel<Tron>> {
	let expected_address =
		<crate::chainflip::address_derivation::AddressDerivation as AddressDerivationApi<
			Tron,
		>>::generate_address(channel.asset, channel.channel_id)
		.expect("Failed to derive expected address for pre-allocated Tron channel");
	if expected_address != channel.address {
		let other_asset = match channel.asset {
			TronAsset::Trx => TronAsset::TrxUsdt,
			TronAsset::TrxUsdt => TronAsset::Trx,
		};
		let other_expected_address =
			<crate::chainflip::address_derivation::AddressDerivation as AddressDerivationApi<
				Tron,
			>>::generate_address(other_asset, channel.channel_id)
			.expect("Failed to derive expected address for other pre-allocated Tron channel");
		if other_expected_address == channel.address {
			log::info!(
				"Channel {} with address {} has asset {:?}, which does not match. Expected address is {}. Updating.",
				channel.channel_id,
				TronAddress(channel.address),
				channel.asset,
				TronAddress(expected_address),
			);
			channel.address = expected_address;
			Some(channel)
		} else {
			log_or_panic!(
				"Channel {} with address {} does not match any expected address.",
				channel.channel_id,
				TronAddress(channel.address),
			);
			None
		}
	} else {
		// No change.
		None
	}
}

impl OnRuntimeUpgrade for DeletePreAllocatedTronChannels {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(Default::default())
	}

	fn on_runtime_upgrade() -> Weight {
		// Update channels in the pool. The should all be deployed, but some may have the wrong
		// address. If any channels are found in the pool with the wrong address, update them to
		// have the correct address.
		log::info!("🧹 Cleaning up pre-allocated Tron channels.");
		for channel in pallet_cf_ingress_egress::DepositChannelPool::<Runtime, TronInstance>::iter()
			.map(|(_, channel)| channel)
			.collect::<Vec<_>>()
		{
			if let Some(new_channel) = maybe_update_channel(channel.clone()) {
				log::info!(
					"🎱 Channel {} in pool with address {} is being updated to have address {}.",
					channel.channel_id,
					TronAddress(channel.address),
					TronAddress(new_channel.address)
				);
				pallet_cf_ingress_egress::DepositChannelPool::<Runtime, TronInstance>::insert(
					channel.channel_id,
					new_channel,
				);
			}
		}

		// Pre-allocated channels:
		// Drain from storage and keep any that are correct.
		// Update any that are deployed but have the wrong address.
		// Drop any that are undeployed.
		// Put updated channels back in the pool.
		for channel in
			pallet_cf_ingress_egress::PreallocatedChannels::<Runtime, TronInstance>::drain()
				.flat_map(|(_, channels)| channels)
		{
			match channel.state {
				DeploymentStatus::Pending => {
					log_or_panic!("Channel {} is Pending. This should not happen, as pre-allocated channels should have been deployed.", channel.channel_id);
				},
				DeploymentStatus::Undeployed => {
					log::info!("🎱 Channel {} is Undeployed. Deleting.", channel.channel_id);
					// Just skip it, as draining already removed it from storage.
				},
				DeploymentStatus::Deployed { at_block_height } => {
					log::info!("🎱 Channel {} is Deployed at block height {}. Checking if address matches expected.", channel.channel_id, at_block_height);
					if let Some(updated_channel) = maybe_update_channel(channel.clone()) {
						log::info!(
							"🎱 Preallocated channel {} has been updated with address {} and is being returned to the pool.",
							channel.channel_id,
							updated_channel.address
						);
						pallet_cf_ingress_egress::DepositChannelPool::<Runtime, TronInstance>::insert(
							updated_channel.channel_id,
							updated_channel,
						);
					} else {
						log::info!(
							"🎱 Preallocated Channel {} was preallocated with correct address, returning to pool unchanged.",
							channel.channel_id
						);
						pallet_cf_ingress_egress::DepositChannelPool::<Runtime, TronInstance>::insert(
							channel.channel_id,
							channel,
						);
					}
				},
			}
		}

		// Check the ChannelLookup but don't edit it. If any mismatches are found
		for (address, mut details) in
			pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, TronInstance>::iter()
				.collect::<Vec<_>>()
		{
			if let Some(mut updated_deposit_channel) =
				maybe_update_channel(details.deposit_channel.clone())
			{
				log::warn!(
					"❗️ ChannelLookup has address {} for channel {}, but expected address is {}. Ensuring it is marked as Undeployed to prevent recycling.",
					updated_deposit_channel.address,
					updated_deposit_channel.channel_id,
					address
				);
				if !matches!(updated_deposit_channel.state, DeploymentStatus::Undeployed) {
					updated_deposit_channel.state = DeploymentStatus::Undeployed;
					details.deposit_channel = updated_deposit_channel;
					pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, TronInstance>::insert(
						address, details,
					);
				}
			}
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
		frame_support::ensure!(
			pallet_cf_ingress_egress::PreallocatedChannels::<Runtime, TronInstance>::iter()
				.next()
				.is_none(),
			"Preallocated Tron channels were not deleted"
		);
		Ok(())
	}
}
