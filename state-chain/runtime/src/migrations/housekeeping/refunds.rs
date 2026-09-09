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
//! Each entry is a scheduled egress straight out of the vault. Amounts are pre-fee: the egress
//! fee is deducted on the way out, so a recipient receives slightly less than the listed amount.
//!
//! Two neighbouring cases are handled elsewhere. Funds still sitting at a deposit channel rather
//! than in the vault cannot be egressed directly and are handled in [`super::stuck_channels`];
//! broadcast fees overcharged by the Tron gas estimation bug were converted to TRX and are repaid
//! by [`super::overcharged_gas`].

use crate::*;
use cf_chains::assets::{eth::Asset as EthAsset, tron::Asset as TronAsset};
use cf_traits::EgressApi;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use hex_literal::hex;
#[cfg(feature = "try-runtime")]
use pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer;
use sp_core::H160;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// (asset, amount in the asset's base units, destination address)
const ETH_REFUNDS: &[(EthAsset, u128, [u8; 20])] = &[
	// Case COM-413 — 1,000 USDC deposited to an expired channel, since swept into the vault.
	(EthAsset::Usdc, 1_000_000_000, hex!("dcd7b6b98b24bba086ccb969aac21d4fb85e944a")),
	// TODO Case COM-473 — pending.
	//
	// TODO COM-415 — ~$821 of ETH sent on Ethereum against an Arbitrum route. Blocked on the
	// fund location (vault vs deposit address 0xda038d5cae4973d71c07dece709a51c710016c3d) and on
	// a destination address.
	//
	// TODO COM-444 — 0.0574204408620121 ETH, wrong-asset send. Recoverability is doubtful: if the
	// deposit contract was derived from the Arbitrum vault the Ethereum vault cannot fetch it.
	// Do not add without on-chain confirmation.
];

/// Tron's `ChainAccount` is an EVM-style `H160`. Base58 Tron addresses (`T...`) decode to
/// `0x41 || <20-byte address>`; the entries below are that 20-byte tail.
///
/// (asset, amount in the asset's base units, destination address)
const TRON_REFUNDS: &[(TronAsset, u128, [u8; 20])] = &[
	// The overcharged-gas cases (COM-480, COM-442, COM-498) are deliberately not listed here:
	// those funds were converted to TRX and sit in `WithheldAssets`, so they are repaid via a
	// swap rather than a direct egress. See [`super::overcharged_gas`].
	//
	// TODO Case COM-366 — pending.
	//
	// TODO Case COM-408 — pending.
	//
	// TODO Case COM-476 — pending.
];

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
			match <TronIngressEgress as EgressApi<_>>::schedule_egress(
				*asset,
				*amount,
				H160(*address),
				None,
			) {
				Ok(d) => log::info!(
					"💸 Tron refund {} {:?} -> 0x{}: egress_id={:?} after_fees={} fee={}",
					amount,
					asset,
					hex::encode(address),
					d.egress_id,
					d.egress_amount,
					d.fee_withheld,
				),
				Err(e) => log::error!(
					"💸 Failed to schedule Tron refund {:?} to 0x{}: {:?}",
					asset,
					hex::encode(address),
					e,
				),
			}
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let eth = ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::decode_len()
			.unwrap_or(0) as u32;
		let tron = ScheduledEgressFetchOrTransfer::<Runtime, TronInstance>::decode_len()
			.unwrap_or(0) as u32;
		let mut buf = Vec::with_capacity(8);
		buf.extend_from_slice(&eth.to_be_bytes());
		buf.extend_from_slice(&tron.to_be_bytes());
		Ok(buf)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let [eth_before, tron_before] = state
			.chunks_exact(4)
			.map(|c| u32::from_be_bytes(c.try_into().expect("chunks_exact(4) yields 4 bytes")))
			.collect::<Vec<_>>()[..]
		else {
			return Err(DispatchError::Other("bad pre_upgrade state"));
		};

		let eth_after = ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::decode_len()
			.unwrap_or(0) as u32;
		let tron_after = ScheduledEgressFetchOrTransfer::<Runtime, TronInstance>::decode_len()
			.unwrap_or(0) as u32;

		assert_eq!(
			eth_after,
			eth_before + ETH_REFUNDS.len() as u32,
			"unexpected Ethereum egress queue delta",
		);
		assert_eq!(
			tron_after,
			tron_before + TRON_REFUNDS.len() as u32,
			"unexpected Tron egress queue delta",
		);
		Ok(())
	}
}
