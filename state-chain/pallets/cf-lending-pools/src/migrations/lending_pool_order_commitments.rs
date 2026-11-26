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

//! Lending pools now record how much of each lender's position is promised to supply-backed limit
//! orders. No such orders exist before this, so every pool starts with no commitments.

use crate::{Config, GeneralLendingPools, LendingPool};
use frame_support::{
	pallet_prelude::Weight,
	sp_runtime::Saturating,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::{ensure, pallet_prelude::DispatchError};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use crate::{Config, Pallet};
	use cf_primitives::{Asset, AssetAmount};
	use codec::{Decode, Encode};
	use frame_support::{pallet_prelude::OptionQuery, sp_runtime::Perquintill, Twox64Concat};
	use sp_std::collections::btree_map::BTreeMap;

	#[derive(Clone, Encode, Decode)]
	pub(super) struct LendingPool<AccountId: Ord> {
		pub total_amount: AssetAmount,
		pub available_amount: AssetAmount,
		pub lender_shares: BTreeMap<AccountId, Perquintill>,
	}

	#[frame_support::storage_alias]
	pub(super) type GeneralLendingPools<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		Asset,
		LendingPool<<T as frame_system::Config>::AccountId>,
		OptionQuery,
	>;
}

impl<AccountId: Ord + Clone + codec::Encode + codec::Decode> From<old::LendingPool<AccountId>>
	for LendingPool<AccountId>
{
	fn from(old: old::LendingPool<AccountId>) -> Self {
		LendingPool {
			total_amount: old.total_amount,
			available_amount: old.available_amount,
			lender_shares: old.lender_shares,
			order_commitments: BTreeMap::new(),
		}
	}
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut entries: u64 = 0;

		GeneralLendingPools::<T>::translate_values::<old::LendingPool<T::AccountId>, _>(|pool| {
			entries.saturating_accrue(1);
			Some(pool.into())
		});

		T::DbWeight::get().reads_writes(entries, entries)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(old::GeneralLendingPools::<T>::iter()
			.map(|(asset, pool)| (asset, LendingPool::<T::AccountId>::from(pool)))
			.collect::<Vec<_>>()
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let mut expected =
			<Vec<(cf_primitives::Asset, LendingPool<T::AccountId>)>>::decode(&mut &state[..])
				.map_err(|_| "failed to decode pre_upgrade state")?;
		let mut migrated = GeneralLendingPools::<T>::iter().collect::<Vec<_>>();

		expected.sort_by_key(|(asset, _)| *asset);
		migrated.sort_by_key(|(asset, _)| *asset);

		ensure!(migrated == expected, "lending pools changed other than gaining order commitments");

		Ok(())
	}
}
