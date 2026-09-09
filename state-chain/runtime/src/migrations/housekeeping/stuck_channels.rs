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
//! These cannot be egressed directly: the channel has to be put back into
//! `DepositChannelLookup`, a `Fetch` scheduled for it, and the amount credited before a refund
//! egress can be scheduled. The reference implementation of that sequence is
//! `deploy_stuck_eth_channels.rs`, added in #6529 (`978e225901`) and since removed; recover it
//! with `git show
//! 978e225901:state-chain/runtime/src/migrations/housekeeping/deploy_stuck_eth_channels.rs`.
//!
//! Two cases are queued for this release, both blocked on data:
//!
//! * COM-371 — 15.39031019 SOL sent to Solana channel 14120216-Solana-118758 (deposit address
//!   9YJGWpYPdpGX5Yn3ycC5KuJRPTcsWehcw4Vk1baY1sYv) a day after it expired. Solana deposit channels
//!   are not recycled and stay under our control across rotations, so a fetch in a runtime upgrade
//!   is viable. Blocked on the destination: the user's sending address
//!   4VemNMuieDGa3EGCxAakNdq4izUUQXiHBYxPE8fXWXEq or the channel's registered refund address
//!   GEBjqmmVo2a8uGcXHuMi3PkCEA8aKUYmZ1F9uVgSXYs — to be confirmed with the counterparty.
//!
//! * COM-477 — DOT sent to Assethub channel 14661791-Assethub-14 and never witnessed, via extrinsic
//!   0xd9d24bc9de34c6d4a5eba986ecf9bcfc20027cb50086bfac428ef4225eab49df. The underlying witnessing
//!   bug is fixed in 2.3; the recovery itself is just credit + fetch + transfer and rides on 2.2.
//!   Blocked on the exact amount (the deposit was a `balances.transfer_all`, so it is not recorded
//!   anywhere) and on a destination address.
//!
//! Once both are filled in, this migration replaces the no-op body below with the per-chain
//! channel reinstatement and fetch, and the refund egresses move into [`super::refunds`].

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("📦 No stuck-channel recoveries configured; nothing to do.");
		Weight::zero()
	}
}
