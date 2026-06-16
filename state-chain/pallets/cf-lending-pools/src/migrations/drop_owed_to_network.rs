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

//! Drops the `owed_to_network` field from `LendingPool`.
//!
//! For any pool that still has `owed_to_network > 0` at upgrade time, we credit as much of
//! it as `available_amount` can cover to `PendingNetworkFees` (and deduct it from
//! `available_amount`). Any residue that exceeds `available_amount` can no longer be drained
//! to the network, so it is redistributed to lenders by adding it to
//! `total_amount`: the borrower's `owed_principal` already includes the corresponding amount, so
//! when they repay, that cash flows into `available_amount` and — with `total_amount` already
//! grown to match — becomes claimable by lenders pro-rata to their shares. This keeps the
//! pool's `available + owed_principal = total_amount` invariant intact after the field is
//! removed.

use crate::{Config, GeneralLendingPools, LendingPool, PendingNetworkFees};
use cf_primitives::AssetAmount;
use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::Weight, sp_runtime::Saturating, traits::UncheckedOnRuntimeUpgrade,
};
use scale_info::TypeInfo;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use super::*;
	use cf_primitives::Asset;
	use frame_support::{
		pallet_prelude::OptionQuery, sp_runtime::Perquintill, storage_alias, Twox64Concat,
	};

	#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(AccountId))]
	pub struct LendingPool<AccountId>
	where
		AccountId: Decode + Encode + Ord + Clone,
	{
		pub total_amount: AssetAmount,
		pub available_amount: AssetAmount,
		pub lender_shares: BTreeMap<AccountId, Perquintill>,
		pub owed_to_network: AssetAmount,
	}

	#[storage_alias]
	pub type GeneralLendingPools<T: crate::Config> = StorageMap<
		crate::Pallet<T>,
		Twox64Concat,
		Asset,
		LendingPool<<T as frame_system::Config>::AccountId>,
		OptionQuery,
	>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		GeneralLendingPools::<T>::translate::<old::LendingPool<T::AccountId>, _>(|asset, old| {
			let collected = core::cmp::min(old.available_amount, old.owed_to_network);
			// The residue the network can't be paid is redistributed to lenders pro-rata by
			// growing `total_amount` (it is already part of the borrower's `owed_principal`).
			let residue = old.owed_to_network.saturating_sub(old.available_amount);
			if collected > 0 {
				PendingNetworkFees::<T>::mutate(asset, |fees| {
					fees.saturating_accrue(collected);
				});
			}
			Some(LendingPool {
				total_amount: old.total_amount.saturating_add(residue),
				available_amount: old.available_amount.saturating_sub(collected),
				lender_shares: old.lender_shares,
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		use cf_primitives::Asset;

		let snapshot: BTreeMap<Asset, (AssetAmount, AssetAmount, AssetAmount, AssetAmount)> =
			old::GeneralLendingPools::<T>::iter()
				.map(|(asset, pool)| {
					let pending = PendingNetworkFees::<T>::get(asset);
					(
						asset,
						(pool.total_amount, pool.available_amount, pool.owed_to_network, pending),
					)
				})
				.collect();
		Ok(snapshot.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_primitives::Asset;

		let snapshot: BTreeMap<Asset, (AssetAmount, AssetAmount, AssetAmount, AssetAmount)> =
			Decode::decode(&mut &state[..]).map_err(|_| "pre_upgrade snapshot decode failed")?;

		for (asset, (total, old_available, owed, pending_before)) in snapshot {
			let pool =
				GeneralLendingPools::<T>::get(asset).ok_or("pool disappeared during migration")?;
			let collected = core::cmp::min(old_available, owed);
			let residue = owed.saturating_sub(old_available);

			frame_support::ensure!(
				pool.total_amount == total.saturating_add(residue),
				"total_amount should grow by the IOU residue"
			);
			frame_support::ensure!(
				pool.available_amount == old_available.saturating_sub(collected),
				"available_amount should drop by the collected IOU"
			);
			frame_support::ensure!(
				PendingNetworkFees::<T>::get(asset) == pending_before.saturating_add(collected),
				"PendingNetworkFees should grow by the collected IOU"
			);
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mocks::{new_test_ext, Test, LP, OTHER_LP};
	use cf_primitives::Asset;
	use frame_support::sp_runtime::Perquintill;

	#[test]
	fn residue_is_credited_to_lenders() {
		new_test_ext().execute_with(|| {
			const ASSET: Asset = Asset::Eth;

			let lender_shares = BTreeMap::from([
				(LP, Perquintill::from_percent(60)),
				(OTHER_LP, Perquintill::from_percent(40)),
			]);

			// A high-utilisation pool: the network is owed more (50) than the pool has
			// available (10), so only part of the IOU can be collected and the rest is residue.
			old::GeneralLendingPools::<Test>::insert(
				ASSET,
				old::LendingPool {
					total_amount: 1000,
					available_amount: 10,
					lender_shares: lender_shares.clone(),
					owed_to_network: 50,
				},
			);

			// Some network fees may already be pending for this asset.
			PendingNetworkFees::<Test>::insert(ASSET, 7);

			Migration::<Test>::on_runtime_upgrade();

			// Expected outcomes for this scenario, kept as independent literals so that a bug in
			// the migration's arithmetic can't be mirrored into the expectations.
			const COLLECTED: AssetAmount = 10; // min(available_amount, owed_to_network)
			const RESIDUE: AssetAmount = 40; // owed_to_network - available_amount

			// The borrower's outstanding debt (990 borrowed + 50 network fee, which includes the
			// residue). Unaffected by the migration.
			const OWED_PRINCIPAL: AssetAmount = 1040;

			let mut pool = GeneralLendingPools::<Test>::get(ASSET).expect("pool should exist");

			// `total_amount` grows by the residue (redistributed to lenders)...
			assert_eq!(pool.total_amount, 1000 + RESIDUE);
			// ...`available_amount` drops by the collected portion...
			assert_eq!(pool.available_amount, 0);
			// ...and lender shares are carried over unchanged.
			assert_eq!(pool.lender_shares, lender_shares);

			// The collected portion is credited to the network on top of what was pending.
			assert_eq!(PendingNetworkFees::<Test>::get(ASSET), 7 + COLLECTED);

			// Once the borrower repays in full, available rises to exactly total: the residue is
			// claimable by lenders and no orphan dust (available > total) is left behind.
			pool.receive_repayment(OWED_PRINCIPAL);
			assert_eq!(pool.available_amount, pool.total_amount);
		});
	}
}
