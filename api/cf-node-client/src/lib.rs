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

use crate::events_decoder::DynamicEvents;
use cf_primitives::TxIndex;
use frame_support::dispatch::DispatchInfo;
use sp_core::H256;
use state_chain_runtime::RuntimeEvent;

pub mod error_decoder;
pub mod events_decoder;
pub mod runtime_decoder;
pub mod signer;
pub mod subxt_state_chain_config;

#[derive(Debug, Clone)]
pub struct ExtrinsicData<Events> {
	pub tx_hash: H256,
	pub events: Events,
	pub header: state_chain_runtime::Header,
	pub dispatch_info: DispatchInfo,
	pub block_hash: state_chain_runtime::Hash,
	pub tx_index: TxIndex,
}

#[derive(Debug)]
pub enum WaitForResult {
	// The hash of the SC transaction that was submitted.
	TransactionHash(H256),
	Details(Box<ExtrinsicData<Vec<RuntimeEvent>>>),
}

#[derive(Debug)]
pub enum WaitForDynamicResult {
	// The hash of the SC transaction that was submitted.
	TransactionHash(H256),
	Data(Box<ExtrinsicData<DynamicEvents>>),
}
