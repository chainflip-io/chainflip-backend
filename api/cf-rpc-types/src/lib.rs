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

use cf_amm::common::{PoolPairsMap, Side};
/// cf-rpc-types module defines all RPC related types
/// Common types are defined in here
use cf_chains::{
	address::AddressString, address::ToHumanreadableAddress, Chain, ChannelRefundParameters,
};
use cf_primitives::{AccountId, Asset, BlockNumber, FlipBalance, Tick, TxIndex};
use frame_support::{Deserialize, Serialize};
use std::ops::Range;

pub use cf_chains::eth::Address as EthereumAddress;
pub use cf_utilities::rpc::NumberOrHex;
pub use sp_core::{bounded::BoundedVec, crypto::AccountId32, ConstU32, H256, U256};
pub use state_chain_runtime::{chainflip::BlockUpdate, Hash};

/// Defines all broker related RPC types
pub mod broker;
/// Defines all LP related RPC types
pub mod lp;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtrinsicResponse<Response> {
	pub block_number: BlockNumber,
	pub block_hash: Hash,
	pub tx_index: TxIndex,
	pub response: Response,
}

pub type RedemptionAmount = pallet_cf_funding::RedemptionAmount<FlipBalance>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapChannelInfo<C: Chain> {
	pub deposit_address: <C::ChainAccount as ToHumanreadableAddress>::Humanreadable,
	pub source_asset: Asset,
	pub destination_asset: Asset,
}

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, PartialEq, Eq)]
pub struct OrderFills {
	pub fills: Vec<OrderFilled>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum OrderFilled {
	LimitOrder {
		lp: AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: U256,
		tick: Tick,
		sold: U256,
		bought: U256,
		fees: U256,
		remaining: U256,
	},
	RangeOrder {
		lp: AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		id: U256,
		range: Range<Tick>,
		bought_amounts: PoolPairsMap<U256>,
		fees: PoolPairsMap<U256>,
		liquidity: U256,
	},
}

pub type RefundParametersRpc = ChannelRefundParameters<AddressString>;
