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

//! A scheduled `set_limit_order` now records the order's backing. Updates scheduled before it
//! existed get `None`, which keeps an existing order's backing and opens a new one backed by the
//! LP's balance: exactly what they would have done when they were scheduled.

use crate::{Config, LimitOrderUpdate, LimitOrderUpdateDetails, ScheduledLimitOrderUpdates};
use frame_support::{
	pallet_prelude::Weight,
	sp_runtime::Saturating,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use sp_std::{marker::PhantomData, vec::Vec};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::{ensure, pallet_prelude::DispatchError};
#[cfg(feature = "try-runtime")]
use frame_system::pallet_prelude::BlockNumberFor;

mod old {
	use crate::{Config, IncreaseOrDecrease, Pallet};
	use cf_amm::{common::Side, math::Tick};
	use cf_primitives::{chains::assets::any, AssetAmount, OrderId};
	use codec::{Decode, Encode};
	use frame_support::{pallet_prelude::ValueQuery, Twox64Concat};
	use frame_system::pallet_prelude::BlockNumberFor;
	use sp_std::vec::Vec;

	#[derive(Clone, Encode, Decode)]
	pub(super) struct LimitOrderUpdate<T: Config> {
		pub lp: T::AccountId,
		pub base_asset: any::Asset,
		pub quote_asset: any::Asset,
		pub side: Side,
		pub id: OrderId,
		pub details: LimitOrderUpdateDetails<BlockNumberFor<T>>,
	}

	#[derive(Clone, Encode, Decode)]
	pub(super) enum LimitOrderUpdateDetails<BlockNumber> {
		Update {
			option_tick: Option<Tick>,
			amount_change: IncreaseOrDecrease<AssetAmount>,
		},
		Set {
			option_tick: Option<Tick>,
			sell_amount: AssetAmount,
			close_order_at: Option<BlockNumber>,
		},
		Close,
	}

	#[frame_support::storage_alias]
	pub(super) type ScheduledLimitOrderUpdates<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		BlockNumberFor<T>,
		Vec<LimitOrderUpdate<T>>,
		ValueQuery,
	>;
}

impl<T: Config> From<old::LimitOrderUpdate<T>> for LimitOrderUpdate<T> {
	fn from(old: old::LimitOrderUpdate<T>) -> Self {
		LimitOrderUpdate {
			lp: old.lp,
			base_asset: old.base_asset,
			quote_asset: old.quote_asset,
			side: old.side,
			id: old.id,
			details: match old.details {
				old::LimitOrderUpdateDetails::Update { option_tick, amount_change } =>
					LimitOrderUpdateDetails::Update { option_tick, amount_change },
				old::LimitOrderUpdateDetails::Set { option_tick, sell_amount, close_order_at } =>
					LimitOrderUpdateDetails::Set {
						option_tick,
						sell_amount,
						close_order_at,
						backing: None,
					},
				old::LimitOrderUpdateDetails::Close => LimitOrderUpdateDetails::Close,
			},
		}
	}
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut entries: u64 = 0;

		ScheduledLimitOrderUpdates::<T>::translate::<Vec<old::LimitOrderUpdate<T>>, _>(
			|_, updates| {
				entries.saturating_accrue(1);
				Some(updates.into_iter().map(Into::into).collect())
			},
		);

		T::DbWeight::get().reads_writes(entries, entries)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(old::ScheduledLimitOrderUpdates::<T>::iter()
			.map(|(block, updates)| {
				(block, updates.into_iter().map(LimitOrderUpdate::<T>::from).collect::<Vec<_>>())
			})
			.collect::<Vec<_>>()
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let mut expected =
			<Vec<(BlockNumberFor<T>, Vec<LimitOrderUpdate<T>>)>>::decode(&mut &state[..])
				.map_err(|_| "failed to decode pre_upgrade state")?;
		let mut migrated = ScheduledLimitOrderUpdates::<T>::iter().collect::<Vec<_>>();

		expected.sort_by_key(|(block, _)| *block);
		migrated.sort_by_key(|(block, _)| *block);

		ensure!(
			migrated == expected,
			"scheduled limit order updates changed other than gaining a backing"
		);

		Ok(())
	}
}
