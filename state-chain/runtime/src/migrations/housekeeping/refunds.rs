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

//! Batched refunds for funds that reached a Chainflip vault but were never credited to a swap.
//!
//! Each entry is a scheduled egress straight out of the vault. Ethereum amounts are pre-fee: the
//! egress fee is deducted on the way out, so a recipient receives slightly less than the listed
//! amount. Tron amounts are paid in full, see [`transfer_from_tron_vault`].
//!
//! Two neighbouring cases are handled elsewhere. Funds still sitting at a deposit channel rather
//! than in the vault cannot be egressed directly and are handled in [`super::stuck_channels`];
//! broadcast fees overcharged by the Tron gas estimation bug were converted to TRX and are repaid
//! by [`super::overcharged_gas`].

use crate::*;
use cf_chains::{
	assets::{eth::Asset as EthAsset, tron::Asset as TronAsset},
	ForeignChain,
};
use cf_primitives::EgressId;
use cf_traits::EgressApi;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use hex_literal::hex;
use pallet_cf_ingress_egress::{EgressIdCounter, FetchOrTransfer, ScheduledEgressFetchOrTransfer};
use sp_core::H160;

/// (asset, amount in the asset's base units, destination address)
const ETH_REFUNDS: &[(EthAsset, u128, [u8; 20])] = &[
	// Case COM-413 — 1,000 USDC deposited to an expired channel, since swept into the vault.
	(EthAsset::Usdc, 1_000_000_000, hex!("dcd7b6b98b24bba086ccb969aac21d4fb85e944a")),
	// Case COM-473 — 0.325 ETH sent to a stale deposit address, owed to the integrator that
	// already made the end user whole. ETH is swept to the vault automatically, so a plain
	// egress suffices.
	(EthAsset::Eth, 325_000_000_000_000_000, hex!("6696c3ba4dd13457e0fd3bc8546cecb8df98e2b7")),
	// COM-415 and COM-444 are deliberately absent. Both sent ETH on Ethereum to what was an
	// Arbitrum deposit address. EVM deposit contracts are per-chain, so those funds are not
	// under our control on Ethereum and there is nothing to refund from.
];

/// Tron's `ChainAccount` is an EVM-style `H160`. Base58 Tron addresses (`T...`) decode to
/// `0x41 || <20-byte address>`; the entries below are that 20-byte tail.
///
/// (asset, amount in the asset's base units, destination address)
const TRON_REFUNDS: &[(TronAsset, u128, [u8; 20])] = &[
	// The overcharged-gas cases (COM-480, COM-442, COM-498) are listed in
	// [`super::overcharged_gas`] instead.
	//
	// Case COM-366 — 100 USDT that reached the vault without a note.
	(TronAsset::TrxUsdt, 100_000_000, hex!("b9ef9e40f4b9f8799e9e23b071286f12bda1daa4")),
	// Case COM-408 — two stuck Tron swaps, kept as separate entries so each transfer maps to
	// the transaction it repays.
	(TronAsset::TrxUsdt, 1_000_000_000, hex!("b9ef9e40f4b9f8799e9e23b071286f12bda1daa4")),
	(TronAsset::TrxUsdt, 500_000_000, hex!("b9ef9e40f4b9f8799e9e23b071286f12bda1daa4")),
	// Case COM-476 — 5,777 USDT sent to the vault without a memo, returned to the sender.
	(TronAsset::TrxUsdt, 5_777_000_000, hex!("a6d5e8e7e2833835043933db69e49c1a47efe1a6")),
];

/// Egresses this module appends, per chain. Checked by the housekeeping post-upgrade hook.
#[cfg(feature = "try-runtime")]
pub const ETHEREUM_EGRESSES: u32 = ETH_REFUNDS.len() as u32;
#[cfg(feature = "try-runtime")]
pub const TRON_EGRESSES: u32 = TRON_REFUNDS.len() as u32;

/// Schedules a transfer of `amount` out of the Tron vault with no egress fee deducted, so the
/// recipient receives exactly `amount`.
///
/// Goes directly onto `ScheduledEgressFetchOrTransfer` rather than through `schedule_egress`.
/// Withholding a fee in trxUSDT would start an `IngressEgressFee` swap into TRX, which cannot
/// execute once the trxUSDT pool has been emptied, and would be retried indefinitely. The
/// broadcast is paid for out of the TRX already withheld on the Tron vault.
pub(super) fn transfer_from_tron_vault(
	asset: TronAsset,
	amount: u128,
	address: [u8; 20],
) -> EgressId {
	let egress_id = EgressIdCounter::<Runtime, TronInstance>::mutate(|id_counter| {
		*id_counter = id_counter.saturating_add(1);
		(ForeignChain::Tron, *id_counter)
	});

	ScheduledEgressFetchOrTransfer::<Runtime, TronInstance>::append(FetchOrTransfer::<
		cf_chains::Tron,
	>::Transfer {
		egress_id,
		asset,
		destination_address: H160(address),
		amount,
	});

	egress_id
}

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		for (asset, amount, address) in ETH_REFUNDS {
			match <EthereumIngressEgress as EgressApi<_>>::schedule_egress(
				*asset,
				*amount,
				H160(*address),
				None,
			) {
				Ok(d) => log::info!(
					"💸 ETH refund {} {:?} -> 0x{}: egress_id={:?} after_fees={} fee={}",
					amount,
					asset,
					hex::encode(address),
					d.egress_id,
					d.egress_amount,
					d.fee_withheld,
				),
				Err(e) => log::error!(
					"💸 Failed to schedule ETH refund {:?} to 0x{}: {:?}",
					asset,
					hex::encode(address),
					e,
				),
			}
		}

		for (asset, amount, address) in TRON_REFUNDS {
			log::info!(
				"💸 Tron refund {} {:?} -> 0x{}: egress_id={:?}, fee-free.",
				amount,
				asset,
				hex::encode(address),
				transfer_from_tron_vault(*asset, *amount, *address),
			);
		}

		Weight::zero()
	}
}
