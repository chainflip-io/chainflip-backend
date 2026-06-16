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

//! Peer-identity types shared by all P2P transports.
//!
//! A [`PeerInfo`] is transport-agnostic: it carries the peer's Ed25519 public key as
//! registered on chain. Transports that need other key material (e.g. ZMQ's CURVE
//! x25519 key) derive it from `ed_pubkey` internally, so an invalid key fails to
//! connect rather than panicking at construction.

use std::net::Ipv6Addr;

use cf_utilities::Port;

use crate::{message::AccountId, EdPublicKey};

/// Information about a peer, shared across all transports.
#[derive(Debug, Clone)]
pub struct PeerInfo {
	pub account_id: AccountId,
	/// The peer's Ed25519 public key, as registered on chain.
	pub ed_pubkey: [u8; 32],
	pub ip: Ipv6Addr,
	pub port: Port,
}

impl PeerInfo {
	pub fn new(
		account_id: AccountId,
		ed_public_key: EdPublicKey,
		ip: Ipv6Addr,
		port: Port,
	) -> Self {
		// The key is stored verbatim; transports validate/convert it when needed, so an
		// invalid key from on-chain registration simply fails to connect rather than
		// panicking here.
		PeerInfo { account_id, ed_pubkey: ed_public_key.0, ip, port }
	}
}

impl std::fmt::Display for PeerInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(
			f,
			"PeerInfo {{ account_id: {}, ed_pubkey: {}, ip: {}, port: {} }}",
			self.account_id,
			hex::encode(self.ed_pubkey),
			self.ip,
			self.port,
		)
	}
}

/// Peer update events sourced from the state chain.
#[derive(Debug)]
pub enum PeerUpdate {
	Registered(PeerInfo),
	Deregistered(AccountId, EdPublicKey),
}
