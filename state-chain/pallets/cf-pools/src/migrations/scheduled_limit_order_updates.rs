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
use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;

	#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub(super) struct LimitOrderUpdate<T: Config> {
		pub lp: T::AccountId,
		pub id: OrderId,
		pub call: LimitOrderCall,
	}

	#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[repr(u8)]
	#[allow(clippy::unnecessary_cast)]
	// This enum is laid out in the same way as the old call, so it will decode exactly the same.
	// The index of the variants is set to match the call_index of the old call.
	pub(super) enum LimitOrderCall {
		UpdateLimitOrder {
			base_asset: any::Asset,
			quote_asset: any::Asset,
			side: Side,
			id: OrderId,
			option_tick: Option<Tick>,
			amount_change: IncreaseOrDecrease<AssetAmount>,
		} = 5_u8,
		SetLimitOrder {
			base_asset: Asset,
			quote_asset: Asset,
			side: Side,
			id: u64,
			option_tick: Option<Tick>,
			sell_amount: AssetAmount,
		} = 6_u8,
	}

	// Migrating this storage item. It was previously storing the call directly in
	// `LimitOrderUpdate`, but now it is a struct.
	#[frame_support::storage_alias]
	pub type ScheduledLimitOrderUpdates<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		BlockNumberFor<T>,
		Vec<LimitOrderUpdate<T>>,
		ValueQuery,
	>;
}

pub struct Migration<T: Config>(PhantomData<T>);

// Migrating from Call -> Struct
fn migrate_update<T: Config>(update: old::LimitOrderUpdate<T>) -> LimitOrderUpdate<T> {
	match update.call {
		old::LimitOrderCall::SetLimitOrder {
			base_asset,
			quote_asset,
			side,
			id,
			option_tick,
			sell_amount,
		} => LimitOrderUpdate {
			lp: update.lp,
			id,
			base_asset,
			quote_asset,
			side,
			details: LimitOrderUpdateDetails::Set {
				option_tick,
				sell_amount,
				// Setting the close_order_at to None by default because it did not exist before.
				close_order_at: None,
			},
		},
		old::LimitOrderCall::UpdateLimitOrder {
			base_asset,
			quote_asset,
			side,
			id,
			option_tick,
			amount_change,
		} => LimitOrderUpdate {
			lp: update.lp,
			id,
			base_asset,
			quote_asset,
			side,
			details: LimitOrderUpdateDetails::Update { option_tick, amount_change },
		},
	}
}

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let update_count = old::ScheduledLimitOrderUpdates::<T>::iter()
			.map(|(_, updates)| updates.len() as u64)
			.sum::<u64>();
		Ok(update_count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		crate::ScheduledLimitOrderUpdates::<T>::translate_values::<Vec<old::LimitOrderUpdate<T>>, _>(
			|old_updates| {
				Some(old_updates.into_iter().map(|update| migrate_update(update)).collect())
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_update_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_update_count = crate::ScheduledLimitOrderUpdates::<T>::iter()
			.map(|(_, updates)| updates.len() as u64)
			.sum::<u64>();

		assert_eq!(pre_update_count, post_update_count);
		Ok(())
	}
}

#[test]
fn test_migrate_update() {
	use crate::mock::*;

	// How the storage item used to look, storing the call directly
	#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	struct OldStorage<T: Config> {
		pub lp: T::AccountId,
		pub id: OrderId,
		pub call: crate::Call<T>,
	}

	new_test_ext().execute_with(|| {
		// Create a call
		// Note: The update_limit_order call has changed, but it should still decode into the old
		// struct because we have only added fields
		let call = crate::Call::<Test>::update_limit_order {
			base_asset: Asset::Flip,
			quote_asset: Asset::Eth,
			side: Side::Buy,
			id: 123,
			option_tick: None,
			amount_change: IncreaseOrDecrease::Increase(100),
			dispatch_at: None,
		};
		// Encode the call in the old storage format
		let encoded_storage = OldStorage { lp: 69, id: 123, call: call.clone() }.encode();

		// Decode it into the new storage struct
		let decoded_storage =
			old::LimitOrderUpdate::<Test>::decode(&mut &encoded_storage[..]).unwrap();

		// Migrate the data and check its the same
		let migrated_update = migrate_update(decoded_storage);
		assert_eq!(
			migrated_update,
			LimitOrderUpdate {
				lp: 69,
				id: 123,
				base_asset: Asset::Flip,
				quote_asset: Asset::Eth,
				side: Side::Buy,
				details: LimitOrderUpdateDetails::Update {
					option_tick: None,
					amount_change: IncreaseOrDecrease::Increase(100),
				},
			}
		);
	});
}
