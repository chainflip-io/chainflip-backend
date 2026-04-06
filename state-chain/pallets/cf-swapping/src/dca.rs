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

use cf_primitives::{AssetAmount, DcaParameters, SwapId, SWAP_DELAY_BLOCKS};
use cf_runtime_utilities::log_or_panic;
use frame_support::pallet_prelude::*;
use sp_runtime::Saturating;
use sp_std::collections::btree_set::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct DcaState {
	pub(crate) scheduled_chunks: BTreeSet<SwapId>,
	pub(crate) remaining_input_amount: AssetAmount,
	pub(crate) remaining_chunks: u32,
	pub(crate) chunk_interval: u32,
	pub(crate) accumulated_output_amount: AssetAmount,
}

impl DcaState {
	pub(crate) fn new(input_amount: AssetAmount, params: Option<DcaParameters>) -> DcaState {
		DcaState {
			remaining_input_amount: input_amount,
			remaining_chunks: params.as_ref().map(|p| p.number_of_chunks).unwrap_or(1),
			// Chunk interval won't be used for non-DCA swaps but seems nicer to
			// set a reasonable default than unwrap Option when it is needed:
			chunk_interval: params.as_ref().map(|p| p.chunk_interval).unwrap_or(SWAP_DELAY_BLOCKS),
			accumulated_output_amount: 0,
			scheduled_chunks: BTreeSet::new(),
		}
	}

	/// Calculate the amount of the next chunk to be scheduled.
	pub(crate) fn calculate_next_chunk(&self) -> Option<AssetAmount> {
		if self.remaining_chunks > 0 {
			let chunk_input_amount = self
				.remaining_input_amount
				.checked_div(self.remaining_chunks as u128)
				.unwrap_or(0);

			Some(chunk_input_amount)
		} else {
			None
		}
	}

	/// Called directly after a chunk has been scheduled. Records the new swap in the DCA state.
	pub(crate) fn record_scheduled_chunk(
		&mut self,
		scheduled_chunk_swap_id: SwapId,
		scheduled_chunk_amount: AssetAmount,
	) {
		// Add the new chunk to the scheduled swaps.
		self.scheduled_chunks.insert(scheduled_chunk_swap_id);

		// Update the remaining values
		self.remaining_chunks.saturating_reduce(1);
		self.remaining_input_amount.saturating_reduce(scheduled_chunk_amount);
	}

	/// Remove the completed chunk from the DCA state and accumulate the output amount.
	pub(crate) fn record_chunk_completion(
		&mut self,
		completed_chunk_swap_id: SwapId,
		completed_chunk_output_amount: AssetAmount,
	) {
		if self.scheduled_chunks.remove(&completed_chunk_swap_id) {
			self.accumulated_output_amount += completed_chunk_output_amount;
		} else {
			log_or_panic!(
				"Invariant violation: the completed swap id {completed_chunk_swap_id} does not match a scheduled chunk."
			);
		}
	}
}
