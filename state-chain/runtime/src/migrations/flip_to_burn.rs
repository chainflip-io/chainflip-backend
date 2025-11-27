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

use crate::{Runtime, SolanaInstance};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

pub struct FlipToBurn;

impl OnRuntimeUpgrade for FlipToBurn {
	fn on_runtime_upgrade() -> Weight {
		pallet_cf_swapping::FlipToBurn::<Runtime>::translate(|burn_amount: Option<Option<u128>>|{
			if let Some(burn_amount) = burn_amount {
				burn_amount.try_into().unwrap_or(i128::MAX)
			} 
		}).map_err(|_| {
			log::warn!("Migration for FlipToBurn was not able to interpret the existing storage in the old format!")
		});

		Weight::zero()
	}
}
