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

//! Repayment of Tron broadcast fees that were overcharged by the gas estimation bug.
//!
//! These are not stuck deposits and cannot be refunded with a plain egress. The path the money
//! took was:
//!
//! 1. `withhold_ingress_or_egress_fee` estimated the egress fee in the chain's gas asset (TRX). The
//!    bug inflated that estimate.
//! 2. Because the egress asset (trxUSDT) is not `Tron::GAS_ASSET`, the estimate was converted with
//!    `calculate_input_for_gas_output` into an amount of trxUSDT — the ~$82-88 actually taken off
//!    each user's output.
//! 3. That trxUSDT was swapped to TRX via an `IngressEgressFee` swap request, and the proceeds
//!    accrued into `WithheldAssets[Trx]`.
//! 4. The broadcast then consumed far less TRX than was bought, leaving a surplus (~20k TRX).
//!
//! So the users' money is sitting in `WithheldAssets[Trx]` denominated in TRX, while what they are
//! owed is denominated in trxUSDT.
//!
//! Repayment therefore runs step 3 in reverse, but the two halves are deliberately independent:
//!
//! * The transfers are scheduled directly onto `ScheduledEgressFetchOrTransfer`, bypassing
//!   `schedule_egress`. That skips fee estimation entirely — which matters both because it is the
//!   component that misbehaved in the first place, and because a withheld fee would leave the user
//!   short of the exact amount we are trying to give back.
//! * A single TRX draw-down and swap tops the protocol back up in trxUSDT. It is sized generously
//!   rather than exactly, and nothing waits on it: the transfers above are scheduled optimistically
//!   on the assumption that the Tron vault already holds enough trxUSDT to cover them.
//!
//! Note that an `IngressEgressFee` swap request cannot be used for the swap. Its completion
//! handler `log_or_panic!`s unless the output asset is the chain's gas asset, so it only runs
//! towards TRX, never away from it.

use crate::*;
use cf_chains::{ForeignChain, SwapOrigin};
use cf_primitives::{Asset, AssetAmount};
use cf_traits::{SwapOutputAction, SwapRequestHandler, SwapRequestType};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_ingress_egress::{EgressIdCounter, FetchOrTransfer, ScheduledEgressFetchOrTransfer};
use sp_core::{crypto::Ss58Codec, H160};
use sp_runtime::AccountId32;

/// Exact amounts owed, in trxUSDT base units (1e-6), and the destination address.
///
/// Tron's `ChainAccount` is an EVM-style `H160`. Base58 Tron addresses (`T...`) decode to
/// `0x41 || <20-byte address>`, and it is that 20-byte tail which goes here.
///
/// These are paid in full: no egress fee is deducted.
const REFUNDS: &[(AssetAmount, [u8; 20])] = &[
	// TODO Case COM-480 — pending.
	//
	// TODO Case COM-442 — pending.
	//
	// TODO Case COM-498 — pending.
];

/// TRX drawn out of the withheld surplus and swapped back to trxUSDT.
///
/// Sized to comfortably exceed the total in `REFUNDS` at any plausible TRX/USDT price, so that the
/// optimistically scheduled transfers are covered without the migration having to price the swap.
/// Any excess simply stays in the protocol's trxUSDT balance.
///
/// TODO: size this once the refund total is known. Zero disables the swap.
const TRX_TO_SWAP: AssetAmount = 0;

/// Internal account credited with the swapped trxUSDT, following the precedent set by the
/// `deploy_stuck_eth_channels` migration.
///
/// TODO: confirm which account this should be before shipping.
const SWAP_OUTPUT_ACCOUNT: &str = "cFLW4PhasdivcJKuA2BGw9Y9dz7EFwks82K8Z6U3MfCk8WcNW";

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		// Scheduled first and unconditionally: the users are repaid whether or not the top-up
		// swap below can be funded.
		for (amount, address) in REFUNDS {
			let egress_id = EgressIdCounter::<Runtime, TronInstance>::mutate(|id_counter| {
				*id_counter = id_counter.saturating_add(1);
				(ForeignChain::Tron, *id_counter)
			});

			ScheduledEgressFetchOrTransfer::<Runtime, TronInstance>::append(FetchOrTransfer::<
				cf_chains::Tron,
			>::Transfer {
				egress_id,
				asset: cf_chains::assets::tron::Asset::TrxUsdt,
				destination_address: H160(*address),
				amount: *amount,
			});

			log::info!(
				"⛽ Repaying {} trxUSDT of overcharged gas to 0x{} (egress {:?}), fee-free.",
				amount,
				hex::encode(address),
				egress_id,
			);
		}

		if TRX_TO_SWAP == 0 {
			log::warn!("⛽ No TRX top-up swap configured; the trxUSDT paid out is unbacked.");
			return Weight::zero();
		}

		let drawn =
			pallet_cf_asset_balances::WithheldAssets::<Runtime>::mutate(Asset::Trx, |withheld| {
				let drawn = (*withheld).min(TRX_TO_SWAP);
				*withheld = withheld.saturating_sub(drawn);
				drawn
			});

		if drawn < TRX_TO_SWAP {
			log::error!(
				"⛽ Only {} TRX withheld, wanted {} for the top-up swap; swapping what there is.",
				drawn,
				TRX_TO_SWAP,
			);
		}

		let account_id = match AccountId32::from_ss58check(SWAP_OUTPUT_ACCOUNT) {
			Ok(account_id) => account_id,
			Err(e) => {
				log::error!("⛽ Could not decode the swap output account: {e:?}");
				return Weight::zero();
			},
		};

		if drawn > 0 {
			let request_id = <Swapping as SwapRequestHandler>::init_swap_request(
				Asset::Trx,
				drawn,
				Asset::TrxUsdt,
				SwapRequestType::RegularNoNetworkFee {
					output_action: SwapOutputAction::CreditOnChain { account_id },
				},
				Default::default(), // no broker fees
				None,               // no price limits: a delayed top-up beats a failed one
				None,               // no DCA
				SwapOrigin::Internal,
			);

			log::info!(
				"⛽ Swapping {} TRX of surplus back to trxUSDT (swap request {:?}).",
				drawn,
				request_id,
			);
		}

		Weight::zero()
	}
}
