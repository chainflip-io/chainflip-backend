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

use crate::{AssetBalances, Environment, Flip, Swapping, Witnesser};
use cf_primitives::EpochIndex;
use cf_traits::{EpochTransitionHandler, FeePayment};

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	fn on_expired_epoch(expired: EpochIndex) {
		<Witnesser as EpochTransitionHandler>::on_expired_epoch(expired);
	}
	fn on_epoch_ending(ending: EpochIndex) {
		let flip_bridged_in = Flip::trigger_flip_reward_distribution(ending);
		let egress_fee_consumed = Swapping::maybe_trigger_flip_to_gateway_egress(
			Environment::state_chain_gateway_address(),
			flip_bridged_in,
		);
		Flip::add_to_offchain_flip_to_be_distributed(
			i128::try_from(egress_fee_consumed).unwrap_or(i128::MAX).saturating_neg(),
		);
	}

	fn on_new_epoch(_new: EpochIndex) {
		AssetBalances::trigger_reconciliation();
	}
}

#[cfg(test)]
mod tests {
	use crate::{configs::BlockWeights, Runtime};
	use frame_support::dispatch::DispatchClass;
	use pallet_cf_flip::WeightInfo;

	/// Payees of one operator's delegation pool that a reward distribution step must be able to
	/// make progress on. Anyone can delegate for the 6 FLIP minimum funding.
	const MIN_GROUP_PAYEES: u32 = 1_000_000;

	// A step settles a pool's payees only after computing all of their cuts, so a pool too large
	// for the idle weight would stop the distribution.
	#[test]
	fn a_large_delegation_pool_fits_in_a_busy_block() {
		let weight = <Runtime as pallet_cf_flip::Config>::WeightInfo::continue_reward_distribution(
			1,
			MIN_GROUP_PAYEES,
		)
		.saturating_add(<Runtime as frame_system::Config>::DbWeight::get().reads(2));
		let block_weights = BlockWeights::get();
		let idle_weight = block_weights.max_block.saturating_sub(
			block_weights
				.get(DispatchClass::Normal)
				.max_total
				.unwrap_or(block_weights.max_block),
		);
		assert!(
			weight.all_lte(idle_weight),
			"Settling one payee of a pool of {MIN_GROUP_PAYEES} ({weight:?}) no longer fits in the \
			 weight left by a full block ({idle_weight:?}). Reduce the per-payee cost of computing \
			 a pool's cuts."
		);
	}
}
