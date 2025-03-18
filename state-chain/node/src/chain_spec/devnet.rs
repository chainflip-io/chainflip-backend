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
use state_chain_runtime::SetSizeParameters;

pub use super::common::*;

// These represent approximately 10 minutes in localnet block times
// Bitcoin blocks are 5 seconds on localnets.
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 10 * 60 / 5;
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 10 * 60 / 14;
pub const ARBITRUM_EXPIRY_BLOCKS: u32 = 10 * 60 * 4;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 10 * 60 / 6;
pub const SOLANA_EXPIRY_BLOCKS: u32 = 10 * 60 * 10 / 4;
pub const ASSETHUB_EXPIRY_BLOCKS: u32 = 10 * 60 / 12;

pub const MIN_AUTHORITIES: AuthorityCount = 1;
pub const AUCTION_PARAMETERS: SetSizeParameters = SetSizeParameters {
	min_size: MIN_AUTHORITIES,
	max_size: MAX_AUTHORITIES,
	max_expansion: MAX_AUTHORITIES,
};

pub const BITCOIN_SAFETY_MARGIN: u64 = 2;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 2;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; // Unused - we use "finalized" instead
