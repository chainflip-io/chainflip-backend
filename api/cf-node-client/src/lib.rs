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
use frame_support::dispatch::DispatchInfo;
use sp_core::H256;

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
}

// TODO: deprecate/remove this in favour of ExtrinsicData<state_chain_runtime::RuntimeEvent>
pub type ExtrinsicDetails =
	(H256, Vec<state_chain_runtime::RuntimeEvent>, state_chain_runtime::Header, DispatchInfo);

#[derive(Debug)]
pub enum WaitForResult {
	// The hash of the SC transaction that was submitted.
	TransactionHash(H256),
	Details(ExtrinsicDetails),
}

#[derive(Debug)]
pub enum WaitForDynamicResult {
	// The hash of the SC transaction that was submitted.
	TransactionHash(H256),
	Data(ExtrinsicData<DynamicEvents>),
}
