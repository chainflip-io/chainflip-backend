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

#![cfg_attr(test, feature(closure_track_caller))]
#![feature(async_fn_track_caller)]

//! Multisig signing and keygen
pub use crypto::{
	bitcoin, ed25519, eth, polkadot, CanonicalEncoding, ChainSigning, ChainTag, CryptoScheme,
	KeyId, Rng, SignatureToThresholdSignature, CHAIN_TAG_SIZE,
};

pub use client::{MultisigClient, MultisigMessage};

/// Multisig client
pub mod client;
/// Provides cryptographic primitives used by the multisig client
mod crypto;

/// Maximum number of payloads in a single bitcoin signing ceremony
// We choose 20,000 because this is approaching the theoretical maximum number of UTXOs in a single
// Bitcoin block.
pub const MAX_BTC_SIGNING_PAYLOADS: usize = 20_000;

pub mod p2p {
	use cf_primitives::AccountId;

	pub type ProtocolVersion = u16;

	/// Currently active wire protocol version
	pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = 1;

	// TODO: Consider if this should be removed, particularly once we no longer use Substrate for
	// peering
	#[derive(Debug, PartialEq, Eq)]
	pub enum OutgoingMultisigStageMessages {
		Broadcast(Vec<AccountId>, Vec<u8>),
		Private(Vec<(AccountId, Vec<u8>)>),
	}

	#[derive(Debug)]
	pub struct VersionedCeremonyMessage {
		pub version: ProtocolVersion,
		pub payload: Vec<u8>,
	}
}
