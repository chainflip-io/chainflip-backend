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

//! Moves brokers' permanently bound Ethereum withdrawal addresses from `cf-swapping` to
//! `cf-asset-balances`, alongside the other withdrawal destination restrictions.

use crate::Runtime;
use frame_support::{
	traits::{OnRuntimeUpgrade, StorageVersion},
	weights::Weight,
};

#[cfg(feature = "try-runtime")]
use frame_support::{ensure, traits::GetStorageVersion};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use crate::Runtime;
	use cf_chains::evm::Address as EthereumAddress;
	use frame_support::{storage_alias, Twox64Concat};

	#[storage_alias]
	pub type BoundBrokerWithdrawalAddress = StorageMap<
		Swapping,
		Twox64Concat,
		<Runtime as frame_system::Config>::AccountId,
		EthereumAddress,
	>;
}

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		let mut count: u64 = 0;
		for (broker, address) in old::BoundBrokerWithdrawalAddress::drain() {
			pallet_cf_asset_balances::BoundBrokerWithdrawalAddress::<Runtime>::insert(
				broker, address,
			);
			count = count.saturating_add(1);
		}
		log::info!("Relocated {count} broker withdrawal address(es) to cf-asset-balances.");

		StorageVersion::new(pallet_cf_swapping::STORAGE_VERSION_U16)
			.put::<pallet_cf_swapping::Pallet<Runtime>>();
		StorageVersion::new(pallet_cf_asset_balances::STORAGE_VERSION_U16)
			.put::<pallet_cf_asset_balances::Pallet<Runtime>>();

		<Runtime as frame_system::Config>::DbWeight::get()
			.reads_writes(count, count.saturating_mul(2).saturating_add(2))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		use codec::Encode;
		Ok((
			old::BoundBrokerWithdrawalAddress::iter().count() as u64,
			pallet_cf_asset_balances::BoundBrokerWithdrawalAddress::<Runtime>::iter().count()
				as u64,
		)
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		use codec::Decode;
		let (old_count, new_count_before) = <(u64, u64)>::decode(&mut &state[..])
			.map_err(|_| TryRuntimeError::Other("failed to decode pre-upgrade counts"))?;
		ensure!(
			pallet_cf_asset_balances::BoundBrokerWithdrawalAddress::<Runtime>::iter().count()
				as u64 == new_count_before.saturating_add(old_count),
			"broker withdrawal address count changed during migration"
		);
		ensure!(
			old::BoundBrokerWithdrawalAddress::iter().next().is_none(),
			"old broker withdrawal address storage not cleared"
		);
		ensure!(
			pallet_cf_swapping::Pallet::<Runtime>::on_chain_storage_version() ==
				pallet_cf_swapping::STORAGE_VERSION_U16,
			"cf-swapping storage version not bumped"
		);
		ensure!(
			pallet_cf_asset_balances::Pallet::<Runtime>::on_chain_storage_version() ==
				pallet_cf_asset_balances::STORAGE_VERSION_U16,
			"cf-asset-balances storage version not bumped"
		);
		Ok(())
	}
}
