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
//! Currently covers COM-477 — DOT sent to Assethub channel 14661791-Assethub-14 via extrinsic
//! 0xd9d24bc9de34c6d4a5eba986ecf9bcfc20027cb50086bfac428ef4225eab49df and never witnessed, because
//! of the Assethub witnessing bug fixed in 2.3. The funds never left the channel, so fetching them
//! and crediting the LP reproduces what witnessing would have done, and rides fine on 2.2.
//!
//! COM-371 (15.39031019 SOL at expired Solana channel 14120216-Solana-118758) belongs here too and
//! is still blocked on confirmation of who is being repaid. It needs a fetch followed by an egress
//! rather than an on-chain credit, since it is a swap channel with an external destination. Solana
//! deposit channels are not recycled and stay under our control across rotations, so the fetch
//! half works the same way.

use crate::*;
use cf_chains::{assets::hub::Asset as HubAsset, hub::calculate_derived_address};
use cf_primitives::Asset;
use cf_traits::BalanceApi;
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

		Weight::zero()
	}
}
