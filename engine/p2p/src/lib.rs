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

//! Generic P2P networking layer.
//!
//! This crate provides protocol-agnostic peer-to-peer messaging infrastructure.
//! It handles:
//! - Peer discovery and connection management
//! - Encrypted communication using X25519 (ZMQ) or TLS 1.3 with Ed25519 (QUIC)
//! - Topic-based message routing via [`muxer::TopicMuxer`]
//!
//! Protocols (like multisig ceremonies) can register for topics and receive
//! messages through [`muxer::ProtocolHandle`].
//!
//! ## Transport Selection
//!
//! Two transports are available, both compiled in and selected at runtime via the
//! [`Transport`] enum passed to [`start_transport`]:
//! - [`Transport::Zmq`]: ZeroMQ with CURVE encryption
//! - [`Transport::Quic`]: QUIC with TLS 1.3 and Ed25519 certificates
//!
//! The two are mutually unintelligible on the wire, so the choice must be coordinated
//! network-wide.

use sp_core::ed25519;
use tokio::sync::{
	mpsc::{UnboundedReceiver, UnboundedSender},
	oneshot,
};

use cf_utilities::Port;

// Transport modules
pub mod quic;
pub mod zmq;

// Shared modules
pub mod fair_channel;
pub mod message;
pub mod muxer;
pub mod peer;
pub mod supervisor;

// Re-export commonly used types
pub use fair_channel::{FairReceiver, FairSender};
pub use message::{AccountId, IncomingMessage, OutgoingMessage, ProtocolVersion, TopicId};
pub use muxer::{ProtocolHandle, Topic, TopicMuxer};
pub use peer::{PeerInfo, PeerUpdate};

/// Per-peer in-flight message limit for the supervisor→muxer incoming channel. Shared between
/// `zmq.rs` (listener→control) and `supervisor.rs` (supervisor→muxer).
pub const INCOMING_MESSAGE_PER_PEER_LIMIT: usize = 100;

/// Which P2P transport implementation to use. The transports are not interoperable, so
/// this must be the same across the whole validator set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
	Zmq,
	Quic,
}

/// Start the selected P2P transport. The two transports share the same interface; this
/// dispatches to the chosen one.
///
/// The transport runs until `shutdown` fires, at which point it tears down (stopping its
/// background threads/tasks and releasing its socket) and returns. This lets the
/// [`supervisor`] restart it on a different transport without restarting the engine.
#[allow(clippy::too_many_arguments)]
pub async fn start_transport(
	transport: Transport,
	p2p_key: P2PKey,
	port: Port,
	current_peers: Vec<PeerInfo>,
	our_account_id: AccountId,
	incoming_message_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	outgoing_message_receiver: UnboundedReceiver<OutgoingMessage>,
	peer_update_receiver: UnboundedReceiver<PeerUpdate>,
	shutdown: oneshot::Receiver<()>,
) -> anyhow::Result<()> {
	match transport {
		Transport::Zmq =>
			zmq::start(
				p2p_key,
				port,
				current_peers,
				our_account_id,
				incoming_message_sender,
				outgoing_message_receiver,
				peer_update_receiver,
				shutdown,
			)
			.await,
		Transport::Quic =>
			quic::start(
				p2p_key,
				port,
				current_peers,
				our_account_id,
				incoming_message_sender,
				outgoing_message_receiver,
				peer_update_receiver,
				shutdown,
			)
			.await,
	}
}

pub type EdPublicKey = ed25519::Public;
pub type XPublicKey = x25519_dalek::PublicKey;

pub fn pk_to_string(pk: &XPublicKey) -> String {
	hex::encode(pk.as_bytes())
}

/// X25519 key pair for ZMQ CURVE encryption.
#[derive(Clone)]
pub struct X25519KeyPair {
	pub public_key: XPublicKey,
	pub secret_key: x25519_dalek::StaticSecret,
}

/// Key material for P2P communication.
///
/// Contains both the Ed25519 signing key (used for TLS identity in QUIC)
/// and the derived X25519 encryption key (used for CURVE encryption in ZMQ).
///
/// Cloneable so the [`supervisor`] can hand the node identity to each transport
/// incarnation across restarts.
#[derive(Clone)]
pub struct P2PKey {
	pub signing_key: ed25519_dalek::SigningKey,
	pub encryption_key: X25519KeyPair,
}

impl P2PKey {
	pub fn new(ed25519_secret_key: &ed25519_dalek::SecretKey) -> Self {
		let x_secret_key = ed25519_secret_key_to_x25519_secret_key(ed25519_secret_key);
		P2PKey {
			signing_key: ed25519_dalek::SigningKey::from_bytes(ed25519_secret_key),
			encryption_key: X25519KeyPair {
				public_key: (&x_secret_key).into(),
				secret_key: x_secret_key,
			},
		}
	}
}

/// Convert an Ed25519 secret key to an X25519 secret key.
///
/// This derivation is used for ZMQ CURVE encryption.
pub fn ed25519_secret_key_to_x25519_secret_key(
	ed25519_sk: &ed25519_dalek::SecretKey,
) -> x25519_dalek::StaticSecret {
	use sha2::{Digest, Sha512};
	let mut h: Sha512 = Sha512::new();
	let mut hash: [u8; 64] = [0u8; 64];
	let mut digest: [u8; 32] = [0u8; 32];

	h.update(ed25519_sk);
	hash.copy_from_slice(h.finalize().as_slice());

	digest.copy_from_slice(&hash[..32]);
	x25519_dalek::StaticSecret::from(digest)
}

/// Convert an Ed25519 public key to an X25519 public key.
pub fn ed25519_public_key_to_x25519_public_key(
	ed25519_pk: &ed25519_dalek::VerifyingKey,
) -> x25519_dalek::PublicKey {
	use curve25519_dalek::edwards::CompressedEdwardsY;
	let ed_point = CompressedEdwardsY::from_slice(&ed25519_pk.to_bytes())
		.expect("VerifyingKey::to_bytes returns 32 bytes.")
		.decompress()
		.unwrap();
	let x_point = ed_point.to_montgomery();

	x25519_dalek::PublicKey::from(x_point.to_bytes())
}
