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

//! Generic message types for the P2P layer.
//!
//! These types are protocol-agnostic and can be used by any overlay protocol.

/// Peer identifier type used throughout the P2P layer.
///
/// This is an alias for `sp_core::crypto::AccountId32`, allowing the p2p crate
/// to be independent of the `cf-primitives` crate.
pub type AccountId = sp_core::crypto::AccountId32;

/// A topic/channel identifier for message routing.
/// Protocols register handlers for specific topics.
pub type TopicId = u16;

/// Protocol version for wire format evolution
pub type ProtocolVersion = u16;

/// Current protocol version
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = 1;

/// Generic outgoing message that p2p can route
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutgoingMessage {
	/// Send identical payload to all specified recipients
	Broadcast { recipients: Vec<AccountId>, payload: Vec<u8> },
	/// Send different payloads to different recipients
	Private { messages: Vec<(AccountId, Vec<u8>)> },
}

/// Incoming message from a peer
#[derive(Debug, Clone)]
pub struct IncomingMessage {
	pub sender: AccountId,
	pub topic: TopicId,
	pub version: ProtocolVersion,
	pub payload: Vec<u8>,
}
