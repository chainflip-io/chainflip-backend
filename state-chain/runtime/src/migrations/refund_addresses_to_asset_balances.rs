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

//! Relocates the liquidity refund address registry from `cf-lp` to `cf-asset-balances`, where it
//! is read by the withdrawal allowlist (a registered refund address is implicitly allowed). This
//! is a cross-pallet move, so it lives at the runtime level and bumps both pallets' on-chain
//! storage versions itself.

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

/// The refund-address storage as it existed in `cf-lp` before the move. `LiquidityProvider` is
/// cf-lp's `construct_runtime!` name, i.e. the pallet storage prefix.
mod old {
	use crate::Runtime;
	use cf_chains::{ForeignChain, ForeignChainAddress};
	use frame_support::{storage_alias, Identity, Twox64Concat};

	#[storage_alias]
	pub type LiquidityRefundAddress = StorageDoubleMap<
		LiquidityProvider,
		Identity,
		<Runtime as frame_system::Config>::AccountId,
		Twox64Concat,
		ForeignChain,
		ForeignChainAddress,
	>;
}

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		let mut count: u64 = 0;
		// `drain` moves each entry out of the old storage, clearing it as we go.
		for (account_id, chain, address) in old::LiquidityRefundAddress::drain() {
			pallet_cf_asset_balances::RefundAddresses::<Runtime>::insert(
				account_id, chain, address,
			);
			count = count.saturating_add(1);
		}
		log::info!("Relocated {count} liquidity refund address(es) to cf-asset-balances.");

		// Bump both pallets' on-chain versions to match the in-code versions.
		StorageVersion::new(pallet_cf_lp::STORAGE_VERSION_U16)
			.put::<pallet_cf_lp::Pallet<Runtime>>();
		StorageVersion::new(pallet_cf_asset_balances::STORAGE_VERSION_U16)
			.put::<pallet_cf_asset_balances::Pallet<Runtime>>();

		// Each moved entry is a read + a delete (from old) + an insert (into new); plus the two
		// version writes.
		<Runtime as frame_system::Config>::DbWeight::get()
			.reads_writes(count, count.saturating_mul(2).saturating_add(2))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		use codec::Encode;
		let count = old::LiquidityRefundAddress::iter().count() as u64;
		Ok(count.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		use codec::Decode;
		let old_count = u64::decode(&mut &state[..])
			.map_err(|_| TryRuntimeError::Other("failed to decode pre-upgrade count"))?;
		let new_count = pallet_cf_asset_balances::RefundAddresses::<Runtime>::iter().count() as u64;
		ensure!(old_count == new_count, "refund address count changed during migration");
		ensure!(
			old::LiquidityRefundAddress::iter().next().is_none(),
			"old refund address storage not cleared"
		);
		ensure!(
			pallet_cf_lp::Pallet::<Runtime>::on_chain_storage_version() ==
				pallet_cf_lp::STORAGE_VERSION_U16,
			"cf-lp storage version not bumped"
		);
		ensure!(
			pallet_cf_asset_balances::Pallet::<Runtime>::on_chain_storage_version() ==
				pallet_cf_asset_balances::STORAGE_VERSION_U16,
			"cf-asset-balances storage version not bumped"
		);
		Ok(())
	}
}
