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

//! P2P interface types for multisig ceremonies.
//!
//! This module defines message types that can be used to bridge between
//! the multisig ceremony logic and the P2P network layer.

use cf_primitives::AccountId;

/// Message received from the P2P network.
///
/// This is the format that the engine orchestrator converts incoming P2P
/// messages into before passing them to the multisig ceremony manager.
#[derive(Debug, Clone)]
pub struct ReceivedCeremonyMessage {
	pub sender: AccountId,
	pub version: u16,
	pub payload: Vec<u8>,
}
