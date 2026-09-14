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
//! The path the money took was:
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
//! The users are owed trxUSDT, and are repaid at the exact amounts as transfers straight out of
//! the Tron vault with no egress fee deducted. The TRX surplus stays in `WithheldAssets`.

use crate::*;
use cf_chains::{assets::tron::Asset as TronAsset, ForeignChain};
use cf_primitives::AssetAmount;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use hex_literal::hex;
use sp_core::H160;

use super::refunds::transfer_from_tron_vault;

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

/// Overcharges owed to accounts that were debited on withdrawal, transferred to the account's
/// registered Tron refund address.
///
/// (amount in trxUSDT base units, account id)
const ACCOUNT_REFUNDS: &[(AssetAmount, [u8; 32])] = &[
	// Case COM-507 — 91.368448 trxUSDT overcharged on a 51,311.693462 USDT_TRON withdrawal.
	(91_368_448, hex!("a01c278b9262bdea45f3c33efe71b06ad3a747263273c796efd9192b70851626")),
];

/// Transfers this module appends to the Tron queue. Checked by the housekeeping post-upgrade hook.
#[cfg(feature = "try-runtime")]
pub const TRON_EGRESSES: u32 = (REFUNDS.len() + ACCOUNT_REFUNDS.len()) as u32;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		for (amount, address) in REFUNDS {
			log::info!(
				"⛽ Repaying {} trxUSDT of overcharged gas to 0x{} (egress {:?}), fee-free.",
				amount,
				hex::encode(address),
				transfer_from_tron_vault(TronAsset::TrxUsdt, *amount, *address),
			);
		}

		for (amount, account_id) in ACCOUNT_REFUNDS {
			let account_id = AccountId::new(*account_id);
			match pallet_cf_lp::LiquidityRefundAddress::<Runtime>::get(&account_id, ForeignChain::Tron)
				.map(H160::try_from)
			{
				Some(Ok(address)) => log::info!(
					"⛽ Repaying {} trxUSDT of overcharged gas to {:?}'s refund address 0x{} (egress {:?}), fee-free.",
					amount,
					account_id,
					hex::encode(address),
					transfer_from_tron_vault(TronAsset::TrxUsdt, *amount, address.0),
				),
				other => log::error!(
					"⛽ No usable Tron refund address for {:?} ({other:?}); {} trxUSDT not repaid.",
					account_id,
					amount,
				),
			}
		}

		Weight::zero()
	}
}
