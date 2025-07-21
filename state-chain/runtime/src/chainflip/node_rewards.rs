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

use crate::{Authorship, Emissions, Flip, Runtime, System};
use cf_primitives::FlipBalance;
use cf_traits::{Issuance, RewardsDistribution};
use frame_support::sp_runtime::traits::BlockNumberProvider;

pub struct BlockAuthorRewardDistribution;

impl RewardsDistribution for BlockAuthorRewardDistribution {
	type Balance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	fn distribute() {
		let reward_amount = Emissions::current_authority_emission_per_block();
		if reward_amount != 0 {
			if let Some(current_block_author) = Authorship::author() {
				pallet_cf_validator::distribute_among_delegators::<Runtime>(
					&current_block_author,
					reward_amount,
					|account, reward| {
						Flip::settle(account, Self::Issuance::mint(reward).into());
					},
				);
			} else {
				log::warn!("No block author for block {}.", System::current_block_number());
			}
		}
	}
}
