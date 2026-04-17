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

use super::*;

/// Provides a static price that can be used as a fallback when estimating fees/gas.
/// These prices are rough approximations of real market prices and should be updated
/// occasionally if the prices move significantly.
pub fn hard_coded_price_for_asset(asset: Asset) -> Price {
	let price_usd_cents = match asset {
		Asset::Usdc |
		Asset::Usdt |
		Asset::ArbUsdc |
		Asset::ArbUsdt |
		Asset::SolUsdc |
		Asset::SolUsdt |
		Asset::HubUsdc |
		Asset::HubUsdt => 100, // $1
		Asset::Flip => 40,                     // ~$0.40
		Asset::Eth | Asset::ArbEth => 280_000, // ~$2,800
		Asset::Dot | Asset::HubDot => 200,     // ~$2
		Asset::Btc | Asset::Wbtc => 8_650_000, // ~$86,500
		Asset::Sol => 12_700,                  // ~$127
	};

	Price::from_usd_cents(asset, price_usd_cents)
}

pub(crate) fn split_off_highest_impact_swap<T: Config>(
	swaps: &mut BTreeMap<SwapId, Swap<T>>,
	failed_swap_group: Vec<FailedSwapState<T>>,
) -> Option<Swap<T>> {
	// Check invariants:
	if failed_swap_group.is_empty() {
		log_or_panic!("Invariant violation: there should be at least one swap in a failed group")
	}
	for failed_swap in &failed_swap_group {
		if !swaps.iter().any(|(swap_id, _)| *swap_id == failed_swap.swap_id()) {
			log_or_panic!(
				"Invariant violation: failed group must be a subset of all executed swaps"
			)
		}
	}
	// Find a swap id that we want to remove (in theory there should always be
	// one from the failing asset/direction, but if we don't for some reason, the fallback is to
	// remove nothing, which would abort the entire batch):
	let maybe_swap_id_to_remove = failed_swap_group
		.iter()
		// If the direction is TO_STABLE, swap amount is in the input amount of
		// *the same* asset (swaps from different assets are executed separately).
		// If the direction is FROM_STABLE, swap amount is the amount in USDC.
		// Either way, the amounts are in the same asset, so we can compare them directly:
		.max_by_key(|swap| swap.stage.swap_amount)
		.map(|swap| swap.swap_id());

	maybe_swap_id_to_remove.and_then(|swap_id_to_remove| swaps.get(&swap_id_to_remove).cloned())
}
