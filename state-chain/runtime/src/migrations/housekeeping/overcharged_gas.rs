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
//! These are not stuck deposits, and cannot be refunded with a plain egress. The path the money
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
//! owed is denominated in trxUSDT. Repaying them means running step 3 in reverse: draw the TRX down
//! out of `WithheldAssets` and swap it back, egressing the output to the user.
//!
//! `WithheldAssets` is netted against `Liabilities` during reconciliation — it is what pays
//! validators back for the gas they front — so the draw-down below is not optional. Scheduling the
//! egress without it would leave the surplus stranded in TRX and open a fresh vault deficit in the
//! output asset.
//!
//! Note that the output asset does not have to be a Tron asset: COM-480 may be paid in ethUSDT, in
//! which case the swap is TRX -> ethUSDT and the egress lands on Ethereum. The table carries the
//! output asset explicitly for that reason.

use crate::*;
use cf_chains::{address::ForeignChainAddress, ForeignChain, SwapOrigin};
use cf_primitives::{Asset, AssetAmount};
use cf_traits::{AssetConverter, SwapOutputAction, SwapRequestHandler, SwapRequestType};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_core::H160;

/// (output asset, amount owed in that asset's base units, destination address)
///
/// The destination is the 20-byte EVM/Tron address; Tron's base58 addresses (`T...`) decode to
/// `0x41 || <20-byte address>`, and it is that 20-byte tail which goes here.
const OVERCHARGED_GAS_REFUNDS: &[(Asset, AssetAmount, [u8; 20])] = &[
	// TODO Case COM-480 — pending.
	//
	// TODO Case COM-442 — pending.
	//
	// TODO Case COM-498 — pending.
];

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		for (output_asset, output_amount, address) in OVERCHARGED_GAS_REFUNDS {
			// Work backwards from what the user is owed to the TRX it costs us, mirroring the
			// conversion that overcharged them in the first place.
			let trx_input =
				<Swapping as AssetConverter>::calculate_input_for_desired_output_or_default_to_zero(
					Asset::Trx,
					*output_asset,
					*output_amount,
					false, // no network fee on a goodwill refund
					true,  // internal swap
				);

			if trx_input == 0 {
				log::error!(
					"⛽ Could not price {} of {:?} in TRX; skipping refund.",
					output_amount,
					output_asset,
				);
				continue;
			}

			// Draw the surplus down rather than opening a fresh vault deficit.
			let drawn = pallet_cf_asset_balances::WithheldAssets::<Runtime>::mutate(
				Asset::Trx,
				|withheld| {
					let drawn = (*withheld).min(trx_input);
					*withheld = withheld.saturating_sub(drawn);
					drawn
				},
			);

			if drawn < trx_input {
				log::error!(
					"⛽ Only {} TRX withheld, needed {} to refund {} of {:?}; skipping.",
					drawn,
					trx_input,
					output_amount,
					output_asset,
				);
				continue;
			}

			let destination = match ForeignChain::from(*output_asset) {
				ForeignChain::Tron => ForeignChainAddress::Tron(H160(*address)),
				ForeignChain::Ethereum => ForeignChainAddress::Eth(H160(*address)),
				ForeignChain::Arbitrum => ForeignChainAddress::Arb(H160(*address)),
				other => {
					log::error!("⛽ Unsupported refund chain {:?}; skipping.", other);
					continue;
				},
			};

			let request_id = <Swapping as SwapRequestHandler>::init_swap_request(
				Asset::Trx,
				drawn,
				*output_asset,
				SwapRequestType::RegularNoNetworkFee {
					output_action: SwapOutputAction::Egress {
						ccm_deposit_metadata: None,
						output_address: destination,
					},
				},
				Default::default(), // no broker fees
				None,               // no price limits: a delayed refund beats a failed one
				None,               // no DCA
				SwapOrigin::Internal,
			);

			log::info!(
				"⛽ Refunding overcharged gas: swapping {} TRX for ~{} of {:?} -> 0x{} (swap request {:?}).",
				drawn,
				output_amount,
				output_asset,
				hex::encode(address),
				request_id,
			);
		}

		Weight::zero()
	}
}
