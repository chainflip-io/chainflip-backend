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

use core::marker::PhantomData;

use cf_primitives::{Asset, AssetAmount, SwapId, SwapRequestId, SWAP_DELAY_BLOCKS};
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_runtime::traits::BlockNumberProvider;
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

use crate::{
	Config, Event, ScheduledSwaps, Swap, SwapFailureReason, SwapRequestState, SwapRequests,
};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

pub struct RescheduleStuckSwaps<T>(PhantomData<T>);

/// Fee swap chunks that were dropped from `ScheduledSwaps` on mainnet. Amounts and swap ids are
/// taken from historical events. Request 1079456 is given swap id 1209168 (originally request
/// 895792's chunk) because a previous migration executed 1079456's swap id for 895792.
fn stuck_swaps<T: Config>() -> Vec<Swap<T>> {
	let execute_at = frame_system::Pallet::<T>::current_block_number() + SWAP_DELAY_BLOCKS.into();
	let swap = |swap_request_id: SwapRequestId,
	            swap_id: SwapId,
	            from: Asset,
	            to: Asset,
	            input_amount: AssetAmount| Swap {
		swap_id,
		swap_request_id,
		from,
		to,
		input_amount,
		refund_params: None,
		execute_at,
	};

	vec![
		swap(SwapRequestId(896597), SwapId(1210204), Asset::Usdt, Asset::Eth, 1_334_836), // ~$1.33
		swap(SwapRequestId(1079456), SwapId(1209168), Asset::ArbUsdc, Asset::ArbEth, 189_370), // ~$0.19
	]
}

/// A swap is only rescheduled if its request is an open fee request with matching assets and
/// nothing is scheduled for it yet, so the migration is a no-op on other networks and can never
/// overwrite or duplicate a live swap.
fn can_reschedule<T: Config>(swap: &Swap<T>, scheduled_swaps: &BTreeMap<SwapId, Swap<T>>) -> bool {
	let Some(swap_request) = SwapRequests::<T>::get(swap.swap_request_id) else {
		log::warn!("Swap request does not exist: {}", swap.swap_request_id);
		return false;
	};

	if swap_request.input_asset != swap.from ||
		swap_request.output_asset != swap.to ||
		!matches!(
			swap_request.state,
			SwapRequestState::IngressEgressFee | SwapRequestState::NetworkFee
		) {
		log::error!("Parameters don't match for swap request: {}", swap.swap_request_id);
		return false;
	}

	if scheduled_swaps.contains_key(&swap.swap_id) ||
		scheduled_swaps.values().any(|s| s.swap_request_id == swap.swap_request_id)
	{
		log::error!(
			"Swap request {} or swap id {} is already scheduled",
			swap.swap_request_id,
			swap.swap_id
		);
		return false;
	}

	true
}

impl<T: Config> UncheckedOnRuntimeUpgrade for RescheduleStuckSwaps<T> {
	fn on_runtime_upgrade() -> Weight {
		let swaps_to_reschedule = stuck_swaps::<T>();
		let mut rescheduled_swaps_count = 0u64;

		ScheduledSwaps::<T>::mutate(|scheduled_swaps| {
			for swap in swaps_to_reschedule.iter() {
				if can_reschedule(swap, scheduled_swaps) {
					crate::Pallet::<T>::deposit_event(Event::<T>::SwapRescheduled {
						swap_id: swap.swap_id,
						execute_at: swap.execute_at,
						reason: SwapFailureReason::PriceImpactLimit,
					});
					log::info!("Rescheduled swap: {}", swap.swap_request_id);
					scheduled_swaps.insert(swap.swap_id, swap.clone());
					rescheduled_swaps_count += 1;
				}
			}
		});

		T::DbWeight::get()
			.reads_writes(1 + swaps_to_reschedule.len() as u64, 1 + rescheduled_swaps_count)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let scheduled_swaps = ScheduledSwaps::<T>::get();
		let expected: Vec<(SwapId, SwapRequestId)> = stuck_swaps::<T>()
			.into_iter()
			.filter(|swap| can_reschedule(swap, &scheduled_swaps))
			.map(|swap| (swap.swap_id, swap.swap_request_id))
			.collect();
		log::info!("Expecting {} stuck swaps to be rescheduled", expected.len());
		Ok(expected.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let expected = <Vec<(SwapId, SwapRequestId)>>::decode(&mut &state[..])
			.map_err(|_| DispatchError::from("Failed to decode pre-upgrade state"))?;
		let scheduled_swaps = ScheduledSwaps::<T>::get();
		for (swap_id, swap_request_id) in expected {
			frame_support::ensure!(
				scheduled_swaps
					.get(&swap_id)
					.is_some_and(|swap| swap.swap_request_id == swap_request_id),
				"Stuck swap was not rescheduled"
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mock::{new_test_ext, Test},
		SwapRequest,
	};

	fn insert_fee_request(id: u64, input_asset: Asset, output_asset: Asset) {
		SwapRequests::<Test>::insert(
			SwapRequestId(id),
			SwapRequest {
				id: SwapRequestId(id),
				input_asset,
				output_asset,
				state: SwapRequestState::IngressEgressFee,
			},
		);
	}

	#[test]
	fn reschedules_matching_fee_requests() {
		new_test_ext().execute_with(|| {
			insert_fee_request(896597, Asset::Usdt, Asset::Eth);
			insert_fee_request(1079456, Asset::ArbUsdc, Asset::ArbEth);

			RescheduleStuckSwaps::<Test>::on_runtime_upgrade();

			let scheduled = ScheduledSwaps::<Test>::get();
			assert_eq!(scheduled.len(), 2);
			assert_eq!(scheduled[&SwapId(1210204)].swap_request_id, SwapRequestId(896597));
			assert_eq!(scheduled[&SwapId(1210204)].input_amount, 1_334_836);
			assert_eq!(scheduled[&SwapId(1209168)].swap_request_id, SwapRequestId(1079456));
			assert_eq!(scheduled[&SwapId(1209168)].input_amount, 189_370);
		});
	}

	#[test]
	fn skips_missing_mismatched_or_already_scheduled_requests() {
		new_test_ext().execute_with(|| {
			// Wrong output asset:
			insert_fee_request(896597, Asset::Usdt, Asset::Flip);
			// Matching, but a chunk is already scheduled for the request:
			insert_fee_request(1079456, Asset::ArbUsdc, Asset::ArbEth);
			let existing = Swap::<Test> {
				swap_id: SwapId(5),
				swap_request_id: SwapRequestId(1079456),
				from: Asset::ArbUsdc,
				to: Asset::ArbEth,
				input_amount: 1,
				refund_params: None,
				execute_at: 1,
			};
			ScheduledSwaps::<Test>::put(BTreeMap::from([(SwapId(5), existing.clone())]));

			RescheduleStuckSwaps::<Test>::on_runtime_upgrade();

			assert_eq!(ScheduledSwaps::<Test>::get(), BTreeMap::from([(SwapId(5), existing)]));
		});
	}
}
