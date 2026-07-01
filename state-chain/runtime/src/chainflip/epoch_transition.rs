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

use crate::{AssetBalances, Emissions, Environment, Flip, Swapping, Validator, Witnesser};
use cf_primitives::{EpochIndex, ACCUMULATE_REWARDS_EPOCH_START};
use cf_traits::EpochTransitionHandler;

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	fn on_expired_epoch(expired: EpochIndex) {
		<Witnesser as EpochTransitionHandler>::on_expired_epoch(expired);
	}
	fn on_new_epoch(new: EpochIndex) {
		AssetBalances::trigger_reconciliation();

		let activation_epoch = pallet_cf_flip::FeeRewardsActivationEpoch::<crate::Runtime>::get();
		if new == activation_epoch {
			Emissions::burn_and_broadcast_supply_update(
				frame_system::Pallet::<crate::Runtime>::block_number(),
			);
		} else if new > activation_epoch {
			let flip_distributed =
				Flip::trigger_flip_reward_distribution(Validator::historical_authorities(new - 1));
			Swapping::maybe_trigger_flip_to_gateway_egress(
				Environment::state_chain_gateway_address(),
				flip_distributed,
			);
		}
	}
}
