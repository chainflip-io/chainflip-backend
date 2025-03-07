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

use cf_primitives::AuthorityCount;
use sp_runtime::{Percent, Permill};
pub use state_chain_runtime::constants::common::*;
use state_chain_runtime::{chainflip::Offence, BlockNumber, FlipBalance, SetSizeParameters};

pub const GENESIS_FUNDING_AMOUNT: FlipBalance = 5_000 * FLIPPERINOS_PER_FLIP;
pub const MIN_FUNDING: FlipBalance = 10 * FLIPPERINOS_PER_FLIP;
pub const REDEMPTION_TAX: FlipBalance = 5 * FLIPPERINOS_PER_FLIP;
pub const MIN_AUTHORITIES: AuthorityCount = 2;
pub const AUCTION_PARAMETERS: SetSizeParameters = SetSizeParameters {
	min_size: MIN_AUTHORITIES,
	max_size: MAX_AUTHORITIES,
	max_expansion: MAX_AUTHORITIES,
};

/// Percent of the epoch we are allowed to redeem
pub const REDEMPTION_PERIOD_AS_PERCENTAGE: u8 = 50;

// Consider the equation (1 + x/1_000_000_000)^n = 1 + inf/100
// where inf is the target yearly inflation (percent), n is the number of compundings that
// we do in a year and x is the inflation rate (Perbill) for each compunding time period.
//
// The following values are calculated by solving the above equation for x using n =
// (365*14400)/150 (since compunding is done every heartbeat which is every 150 blocks) and
// inf is taken as 0.1 percent for authority emissions and 0.02 percent for backup node emissions.
//
// Can be generated using the following python code:
//
// ```
// import math
// def per_bill_inflation(pct):
//      return (math.pow(1 + pct/100, 1/35040) - 1) * 1000000000
//
// # Testnet:
// round(per_bill_inflation(0.1)) -> 28
// round(per_bill_inflation(0.02)) -> 6
// # Mainnet:
// round(per_bill_inflation(7)) -> 1931
// round(per_bill_inflation(1)) -> 284
// ```
//
pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 28;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 6;

pub const SUPPLY_UPDATE_INTERVAL: u32 = 24 * HOURS;

// This is equivalent to one reputation point for every minute of online time.
pub const REPUTATION_PER_HEARTBEAT: i32 = 15;
pub const ACCRUAL_RATIO: (i32, u32) = (REPUTATION_PER_HEARTBEAT, HEARTBEAT_BLOCK_INTERVAL);

const REPUTATION_PENALTY_SMALL: i32 = REPUTATION_PER_HEARTBEAT; // 15 minutes to recover reputation
const REPUTATION_PENALTY_MEDIUM: i32 = REPUTATION_PER_HEARTBEAT * 4; // One hour to recover reputation
const REPUTATION_PENALTY_LARGE: i32 = REPUTATION_PER_HEARTBEAT * 8; // Two hours to recover reputation

// A penalty of 1 is missing an Ethereum witnessing every 15 seconds. We do liveness every minute.
// which is equivalent to ~4 missed Ethereum witnessings.
const LIVENESS_REPUTATION_PENALTY: i32 = REPUTATION_PER_HEARTBEAT / 5;

/// The offences committable within the protocol and their respective reputation penalty and
/// suspension durations.
pub const PENALTIES: &[(Offence, (i32, BlockNumber))] = &[
	(Offence::MissedHeartbeat, (REPUTATION_PENALTY_SMALL, 0)),
	(Offence::ParticipateKeygenFailed, (REPUTATION_PENALTY_MEDIUM, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::ParticipateSigningFailed, (REPUTATION_PENALTY_MEDIUM, MINUTES / 2)),
	(Offence::MissedAuthorshipSlot, (REPUTATION_PENALTY_LARGE, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::FailedToBroadcastTransaction, (REPUTATION_PENALTY_MEDIUM, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::GrandpaEquivocation, (REPUTATION_PENALTY_LARGE, HEARTBEAT_BLOCK_INTERVAL * 5)),
	(
		Offence::FailedLivenessCheck(cf_chains::ForeignChain::Solana),
		(LIVENESS_REPUTATION_PENALTY, 0),
	),
];

/// Daily slashing rate 0.1% (of the bond) for offline authority
pub const DAILY_SLASHING_RATE: Permill = Permill::from_perthousand(1);

/// Redemption delay on testnets is 2 MINUTES.
/// We use a ttl of 1 hour to give enough of a buffer.
pub const REDEMPTION_TTL_SECS: u64 = 2 * 3600;

/// Determines the expiry duration for governance proposals.
pub const EXPIRY_SPAN_IN_SECONDS: u64 = 24 * 3600;

pub const AUCTION_BID_CUTOFF_PERCENTAGE: Percent = Percent::from_percent(10);
