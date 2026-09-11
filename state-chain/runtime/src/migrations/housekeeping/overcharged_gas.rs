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
use cf_traits::{BalanceApi, SwapOutputAction, SwapRequestHandler, SwapRequestType};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use hex_literal::hex;
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
	// Case COM-480 — 251.630603 trxUSDT overcharged across three swaps.
	(251_630_603, hex!("229218df4e574f9f30089f62ab4bd75e8dd97199")),
	// Case COM-498 — 90.13 trxUSDT overcharged.
	(90_130_000, hex!("b3f89c03b7db2886ca1b1ecfc243bcce54f8e805")),
	// Case COM-442 — 71.60 trxUSDT overcharged.
	(71_600_000, hex!("73bbb9d7396ae5b4838cb781806f3984a1fe995e")),
];

/// TRX drawn out of the withheld surplus and swapped back to trxUSDT.
///
/// Sized to comfortably exceed the total in `REFUNDS` at any plausible TRX/USDT price, so that the
/// optimistically scheduled transfers are covered without the migration having to price the swap.
/// Any excess simply stays in the protocol's trxUSDT balance.
///
/// Overcharges owed to accounts that hold an on-chain balance, credited in trxUSDT rather than
/// transferred. This is the exact inverse of the debit the withdrawal took, needs no destination
/// address, and leaves the holder free to withdraw to whichever chain they want.
///
/// (amount in trxUSDT base units, account id)
const CREDITS: &[(AssetAmount, [u8; 32])] = &[
	// TODO Case COM-507 — pending.
];

/// Everything owed across both tables, in trxUSDT base units.
const fn total_owed() -> AssetAmount {
	let mut total = 0;
	let mut i = 0;
	while i < REFUNDS.len() {
		total += REFUNDS[i].0;
		i += 1;
	}
	let mut i = 0;
	while i < CREDITS.len() {
		total += CREDITS[i].0;
		i += 1;
	}
	total
}

/// Fails the build if the top-up swap would not cover the tables even with TRX at $0.25, so that
/// adding a refund without resizing the swap cannot slip through. Both amounts are in 1e-6 units,
/// so the TRX needed is the trxUSDT owed divided by the price. Evaluated at compile time — the
/// assert is a build error, never a runtime panic.
const _: () = assert!(
	TRX_TO_SWAP >= total_owed() * 4,
	"TRX_TO_SWAP no longer covers the refunds; resize it against the current TRX price."
);

/// The three entries above total 413.360603 trxUSDT. At the TRX price of $0.3386 on 2026-09-11
/// that is ~1,221 TRX, so 2,000 leaves room for slippage and for the price to fall by a third
/// between this upgrade and the swap executing a few blocks later. Anything left over stays in the
/// protocol's trxUSDT balance.
///
/// Kept deliberately close to the requirement rather than maximally generous: this is drawn out of
/// the gas budget that reconciliation uses to repay validators, so over-swapping has a cost of its
/// own.
const TRX_TO_SWAP: AssetAmount = 2_000_000_000;

/// Internal account credited with the swapped trxUSDT, following the precedent set by the
/// `deploy_stuck_eth_channels` migration.
///
/// TODO: confirm which account this should be before shipping.
const SWAP_OUTPUT_ACCOUNT: &str = "cFLW4PhasdivcJKuA2BGw9Y9dz7EFwks82K8Z6U3MfCk8WcNW";

/// Transfers this module appends to the Tron queue. Checked by the housekeeping post-upgrade hook.
#[cfg(feature = "try-runtime")]
pub const TRON_EGRESSES: u32 = REFUNDS.len() as u32;

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

		for (amount, account_id) in CREDITS {
			pallet_cf_asset_balances::Pallet::<Runtime>::credit_account(
				&AccountId::new(*account_id),
				Asset::TrxUsdt,
				*amount,
			);

			log::info!(
				"⛽ Crediting {} trxUSDT of overcharged gas to {:?}.",
				amount,
				AccountId::new(*account_id),
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
