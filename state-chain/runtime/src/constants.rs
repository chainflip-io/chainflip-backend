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

pub mod common {
	use cf_primitives::{
		AuthorityCount, BlockNumber, FlipBalance, FLIPPERINOS_PER_FLIP, MILLISECONDS_PER_BLOCK,
		SECONDS_PER_BLOCK,
	};

	pub const CHAINFLIP_SS58_PREFIX: u16 = 2112;

	pub const TOTAL_ISSUANCE: FlipBalance = {
		const TOTAL_ISSUANCE_IN_FLIP: FlipBalance = 90_000_000;
		TOTAL_ISSUANCE_IN_FLIP * FLIPPERINOS_PER_FLIP
	};

	pub const MAX_AUTHORITIES: AuthorityCount = 150;

	// ======= Keygen and signing =======

	/// Maximum duration a ceremony stage can last
	pub const MAX_STAGE_DURATION_SECONDS: u32 = 30;

	const EXPECTED_FINALITY_DELAY_BLOCKS: u32 = 4;

	/// The transaction with the ceremony outcome needs some
	/// time to propagate to other nodes.
	const NETWORK_DELAY_SECONDS: u32 = 6;
	/// Buffer for final key computation.
	const KEY_DERIVATION_DELAY_SECONDS: u32 = 120;

	const TIMEOUT_BUFFER_SECONDS: u32 =
		EXPECTED_FINALITY_DELAY_BLOCKS * (SECONDS_PER_BLOCK as u32) + NETWORK_DELAY_SECONDS;

	const KEYGEN_TIMEOUT_BUFFER_SECONDS: u32 =
		TIMEOUT_BUFFER_SECONDS + KEY_DERIVATION_DELAY_SECONDS;

	const NUM_THRESHOLD_SIGNING_STAGES: u32 = 4;

	const NUM_KEYGEN_STAGES: u32 = 9;

	/// The number of blocks to wait for a threshold signature ceremony to complete.
	pub const THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS: u32 =
		((MAX_STAGE_DURATION_SECONDS * NUM_THRESHOLD_SIGNING_STAGES) + TIMEOUT_BUFFER_SECONDS) /
			SECONDS_PER_BLOCK as u32;

	/// The maximum number of blocks to wait for a keygen to complete.
	pub const KEYGEN_CEREMONY_TIMEOUT_BLOCKS: u32 = ((MAX_STAGE_DURATION_SECONDS *
		(NUM_KEYGEN_STAGES + NUM_THRESHOLD_SIGNING_STAGES)) +
		KEYGEN_TIMEOUT_BUFFER_SECONDS) /
		SECONDS_PER_BLOCK as u32;

	// NOTE: Currently it is not possible to change the slot duration after the chain has started.
	//       Attempting to do so will brick block production.
	pub const SLOT_DURATION: u64 = MILLISECONDS_PER_BLOCK;

	// Time is measured by number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECONDS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;
	pub const YEAR: BlockNumber = DAYS * 365;

	// Measurements for other chains
	pub const MILLISECONDS_PER_BLOCK_ETHEREUM: u64 = 14 * 1000;
	pub const MILLISECONDS_PER_BLOCK_POLKADOT: u32 = 6 * 1000;
	pub const MILLISECONDS_PER_BLOCK_BITCOIN: u64 = 10 * 60 * 1000;
	pub const MILLISECONDS_PER_BLOCK_ARBITRUM: u64 = 250;
	pub const MILLISECONDS_PER_BLOCK_SOLANA: u64 = 400;
	pub const MILLISECONDS_PER_BLOCK_ASSETHUB: u32 = 12 * 1000;

	pub const BLOCKS_PER_MINUTE_ETHEREUM: u64 = 60000 / MILLISECONDS_PER_BLOCK_ETHEREUM;
	pub const BLOCKS_PER_MINUTE_POLKADOT: u32 = 60000 / MILLISECONDS_PER_BLOCK_POLKADOT;
	// no constant for bitcoin since the block time is too large
	pub const BLOCKS_PER_MINUTE_ARBITRUM: u64 = 60000 / MILLISECONDS_PER_BLOCK_ARBITRUM;
	pub const BLOCKS_PER_MINUTE_SOLANA: u64 = 60000 / MILLISECONDS_PER_BLOCK_SOLANA;
	pub const BLOCKS_PER_MINUTE_ASSETHUB: u32 = 60000 / MILLISECONDS_PER_BLOCK_ASSETHUB;

	/// Percent of the epoch we are allowed to redeem
	pub const REDEMPTION_PERIOD_AS_PERCENTAGE: u8 = 50;

	/// The duration of the heartbeat interval in blocks. 150 blocks at a 6 second block time is
	/// equivalent to 15 minutes.
	pub const HEARTBEAT_BLOCK_INTERVAL: BlockNumber = 150;

	/// The interval at which we update the per-block emission rate.
	///
	/// **Important**: If this constant is changed, we must also change
	/// [CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL].
	pub const COMPOUNDING_INTERVAL: u32 = HEARTBEAT_BLOCK_INTERVAL;

	/// The multiplier used to convert transaction weight into fees paid by the validators.
	///
	/// This can be used to estimate the value we put on our block execution times. We have 6
	/// seconds, and 1_000_000_000_000 weight units per block. We can extrapolate this to an epoch,
	/// and compare this to the rewards earned by validators over this period.
	///
	/// See https://github.com/chainflip-io/chainflip-backend/issues/1629
	pub const TX_FEE_MULTIPLIER: FlipBalance = 10_000;

	/// The amount of time (in block number) allowed to pass from when a Witnessed call is
	/// dispatched to the witnessing deadline. After the deadline is passed, any authorities failed
	/// to witness the dispatched call are penalized.
	pub const LATE_WITNESS_GRACE_PERIOD: BlockNumber = 10u32;

	/// How many blocks does a liveness election last. After `LIVENESS_CHECK_DURATION` blocks,
	/// the liveness check will be performed, if validators have not voted OR voted for something
	/// that wasn't the consensus value, they will be penalized.
	pub const LIVENESS_CHECK_DURATION: BlockNumber = 10;
}
