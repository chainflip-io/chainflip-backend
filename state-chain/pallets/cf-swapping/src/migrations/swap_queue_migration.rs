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

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct Swap<T: Config> {
		pub swap_id: SwapId,
		pub swap_request_id: SwapRequestId,
		pub from: Asset,
		pub to: Asset,
		pub input_amount: AssetAmount,
		pub fees: Vec<FeeType<T>>,
		pub refund_params: Option<SwapRefundParameters>,
		// Migration is adding an execute_at field here
	}

	#[frame_support::storage_alias]
	// Migration is also renaming this storage item to ScheduledSwaps
	pub type SwapQueue<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, BlockNumberFor<T>, Vec<Swap<T>>, ValueQuery>;
}
use sp_std::collections::btree_map::BTreeMap;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let swaps_count =
			old::SwapQueue::<T>::iter().fold(0u32, |acc, (_, swaps)| acc + swaps.len() as u32);
		Ok(swaps_count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let swaps_map: BTreeMap<SwapId, Swap<T>> = <old::SwapQueue<T>>::iter()
			.flat_map(|(block, swaps)| {
				swaps.into_iter().map(move |swap| {
					log::info!("🧜‍♂️ migrating swap with id {:?}, block {:?}", swap.swap_id, block);
					let new_swap = Swap::new(
						swap.swap_id,
						swap.swap_request_id,
						swap.from,
						swap.to,
						swap.input_amount,
						swap.refund_params,
						swap.fees,
						block,
					);
					(swap.swap_id, new_swap)
				})
			})
			.collect();

		let _result = <old::SwapQueue<T>>::clear(u32::MAX, None);

		crate::ScheduledSwaps::<T>::put(swaps_map);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_swaps_count = <u32>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_swaps_count = crate::ScheduledSwaps::<T>::get().len() as u32;

		assert_eq!(pre_swaps_count, post_swaps_count);
		Ok(())
	}
}
