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

//! Recovery for deposits that are still sitting at a deposit channel rather than in a vault.
//!
//! These cannot be paid out directly: the funds have to be fetched out of the channel first.
//!
//! Case COM-477 was never witnessed because of the Assethub witnessing bug fixed in 2.3. The
//! funds never left the channel, so fetching them and crediting the LP reproduces what witnessing
//! would have done, and rides fine on 2.2.
//!
//! Case COM-371 reached a Solana channel after it had expired. Solana deposit channels are not
//! recycled and stay under our control across rotations, so the same fetch works. Unlike COM-477
//! this is a swap channel with an external destination, so the fetch is followed by an egress
//! rather than an on-chain credit.

use crate::{chainflip::address_derivation::AddressDerivation, *};
use cf_chains::{
	address::AddressDerivationApi,
	assets::{hub::Asset as HubAsset, sol::Asset as SolAsset},
	hub::calculate_derived_address,
	sol::{SolAddress, SolanaDepositFetchId},
	Solana,
};
use cf_primitives::Asset;
use cf_traits::{BalanceApi, EgressApi};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use hex_literal::hex;
use pallet_cf_ingress_egress::{FetchOrTransfer, ScheduledEgressFetchOrTransfer};

/// The channel id. The deposit address is derived from it rather than hardcoded, so it cannot
/// drift from what the protocol would compute.
const COM_477_CHANNEL_ID: u64 = 14;

/// Balance of the deposit channel net of the fee the `balances.transfer_all` paid, in Planck.
const COM_477_AMOUNT: u128 = 19_999_691_329_918;

/// The channel's LP.
///
/// This was an LP deposit channel, so the recovery is what witnessing would have done had it
/// worked: credit the LP on-chain. They can then withdraw to their registered DOT refund address
/// or anywhere else, as normal.
const COM_477_LP_ACCOUNT: [u8; 32] =
	hex!("00000000000000000000000026d211574963fa0fc0dff0211caa20b614063de6");

/// The channel id.
const COM_371_CHANNEL_ID: u64 = 118_758;

/// 15.39031019 SOL, in lamports.
const COM_371_AMOUNT: u64 = 15_390_310_190;

/// The channel's deposit address.
///
/// The address and its PDA bump are re-derived from the channel id below rather than hardcoded;
/// this is kept only so the derivation can be checked against what is actually on chain, and the
/// migration refuses to act if the two disagree.
const COM_371_DEPOSIT_ADDRESS: [u8; 32] =
	hex!("7ee2065fd412020955e3e1d849579ea974bfc79493833d45d8ffc0a93e1a3a53");

/// The channel's registered refund address.
const COM_371_DESTINATION: [u8; 32] =
	hex!("03e69ee1b4f77ef79322365aa663dfdbbac8b6146578f227c67fe98e445c5148");

/// Queue entries this module appends. COM-477 only fetches, because the LP is credited on chain;
/// COM-371 fetches and then transfers. Checked by the housekeeping post-upgrade hook, so a failed
/// address derivation shows up as a delta mismatch rather than passing silently.
#[cfg(feature = "try-runtime")]
pub const ASSETHUB_EGRESSES: u32 = 1;
#[cfg(feature = "try-runtime")]
pub const SOLANA_EGRESSES: u32 = 2;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		let Some(master_account) = Environment::assethub_vault_account() else {
			log::error!("📦 No Assethub vault account set; cannot recover channel funds.");
			return Weight::zero();
		};

		let deposit_address = calculate_derived_address(master_account, COM_477_CHANNEL_ID);

		// Polkadot fetches take the whole channel balance, so the amount here is informational.
		ScheduledEgressFetchOrTransfer::<Runtime, AssethubInstance>::append(FetchOrTransfer::<
			cf_chains::Assethub,
		>::Fetch {
			asset: HubAsset::HubDot,
			deposit_address,
			deposit_fetch_id: Some(COM_477_CHANNEL_ID),
			amount: COM_477_AMOUNT,
		});

		pallet_cf_asset_balances::Pallet::<Runtime>::credit_account(
			&AccountId::new(COM_477_LP_ACCOUNT),
			Asset::HubDot,
			COM_477_AMOUNT,
		);

		log::info!(
			"📦 COM-477: fetching Assethub channel {} and crediting {} Planck to the LP.",
			COM_477_CHANNEL_ID,
			COM_477_AMOUNT,
		);

		Self::recover_com_371();

		Weight::zero()
	}
}

impl Migration {
	fn recover_com_371() {
		let (address, bump) =
			match <AddressDerivation as AddressDerivationApi<Solana>>::generate_address_and_state(
				SolAsset::Sol,
				COM_371_CHANNEL_ID,
			) {
				Ok(derived) => derived,
				Err(e) => {
					log::error!("📦 COM-371: could not derive the deposit address: {e:?}");
					return;
				},
			};

		if address != SolAddress(COM_371_DEPOSIT_ADDRESS) {
			log::error!(
				"📦 COM-371: derived {address:?} but the deposit is at {:?}; not fetching.",
				SolAddress(COM_371_DEPOSIT_ADDRESS),
			);
			return;
		}

		ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::append(FetchOrTransfer::<
			Solana,
		>::Fetch {
			asset: SolAsset::Sol,
			deposit_address: address,
			deposit_fetch_id: Some(SolanaDepositFetchId {
				channel_id: COM_371_CHANNEL_ID,
				address,
				bump,
			}),
			amount: COM_371_AMOUNT,
		});

		match <SolanaIngressEgress as EgressApi<_>>::schedule_egress(
			SolAsset::Sol,
			COM_371_AMOUNT,
			SolAddress(COM_371_DESTINATION),
			None,
		) {
			Ok(d) => log::info!(
				"📦 COM-371: fetching channel {} and refunding {} lamports: egress_id={:?} after_fees={} fee={}",
				COM_371_CHANNEL_ID,
				COM_371_AMOUNT,
				d.egress_id,
				d.egress_amount,
				d.fee_withheld,
			),
			Err(e) => log::error!("📦 COM-371: failed to schedule the SOL refund: {e:?}"),
		}
	}
}
