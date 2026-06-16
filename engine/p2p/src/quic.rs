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

//! QUIC transport implementation for P2P networking.
//!
//! This module provides an alternative to the ZMQ transport using QUIC with
//! TLS 1.3 and Ed25519 certificates for peer authentication.

mod auth;
mod cert;
mod connection;
#[cfg(test)]
mod tests;

use std::{cell::Cell, net::Ipv6Addr, sync::Arc};

use anyhow::Context;
use cf_utilities::{make_periodic_tick, metrics::P2P_MSG_SENT, Port};
use connection::{
	configure_client, configure_server, connect_to_peer, create_endpoint, receive_message,
	send_message, ActiveConnectionWrapper, ConnectionState, ConnectionStateInfo, ReconnectContext,
	MAX_INACTIVITY_THRESHOLD,
};
use quinn::Endpoint;
use sp_core::ed25519::Public as EdPublicKey;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, info, info_span, trace, warn, Instrument};

pub use auth::AllowlistVerifier;
pub use cert::CertificateIdentity;

use crate::{
	message::{AccountId, OutgoingMessage},
	P2PKey,
};

/// How often to check for stale connections
const ACTIVITY_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Peer information for QUIC connections.
#[derive(Debug, Clone)]
pub struct PeerInfo {
	pub account_id: AccountId,
	/// The Ed25519 public key (used directly for TLS certificate verification)
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
		// The key is stored verbatim and only validated when it is used to verify a TLS
		// handshake, so an invalid key from on-chain registration simply fails to connect
		// rather than panicking here.
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

/// Peer update events from the state chain.
#[derive(Debug)]
pub enum PeerUpdate {
	Registered(PeerInfo),
	Deregistered(AccountId, EdPublicKey),
}

/// QUIC transport context holding all state.
struct QuicContext {
	endpoint: Endpoint,
	allowlist: Arc<AllowlistVerifier>,
	active_connections: ActiveConnectionWrapper,
	reconnect_context: ReconnectContext,
	our_account_id: AccountId,
}

impl QuicContext {
	async fn send_messages(&mut self, messages: OutgoingMessage) {
		match messages {
			OutgoingMessage::Broadcast { recipients, payload } => {
				trace!("Broadcasting a message to {} peers", recipients.len());
				for acc_id in recipients {
					self.send_message_to(acc_id, payload.clone()).await;
				}
			},
			OutgoingMessage::Private { messages } => {
				trace!("Sending private messages to {} peers", messages.len());
				for (acc_id, payload) in messages {
					self.send_message_to(acc_id, payload).await;
				}
			},
		}
	}

	async fn send_message_to(&mut self, account_id: AccountId, payload: Vec<u8>) {
		if let Some(peer) = self.active_connections.get(&account_id) {
			peer.last_activity.set(tokio::time::Instant::now());

			match &peer.state {
				ConnectionState::Connected(conn) => {
					if let Err(e) = send_message(&conn.connection, payload).await {
						warn!("Failed to send message to {}: {e}", account_id);
						// Connection might be dead, schedule reconnection
						if let Some(peer_mut) = self.active_connections.get_mut(&account_id) {
							peer_mut.state = ConnectionState::ReconnectionScheduled;
							self.reconnect_context.schedule_reconnect(account_id);
						}
					} else {
						P2P_MSG_SENT.inc();
					}
				},
				ConnectionState::ReconnectionScheduled => {
					warn!("Cannot send to {}: reconnection scheduled", account_id);
				},
				ConnectionState::Stale => {
					// Reconnect lazily
					let peer_info = peer.info.clone();
					let last_activity = peer.last_activity.get();
					self.connect_to_peer(peer_info, last_activity).await;
					// Retry send after connecting
					Box::pin(self.send_message_to(account_id, payload)).await;
				},
			}
		} else {
			warn!("Cannot send to {}: peer not registered", account_id);
		}
	}

	fn on_peer_update(&mut self, update: PeerUpdate) {
		match update {
			PeerUpdate::Registered(peer_info) => {
				let account_id = peer_info.account_id.clone();
				if account_id == self.our_account_id {
					return;
				}

				// Add to allowlist (this also revokes any previously-registered key for
				// this account, e.g. after a node-key rotation).
				self.allowlist.add_peer(peer_info.ed_pubkey, account_id.clone());

				// Remove any existing connection
				let connections = &mut self.active_connections;
				let reconnect_context = &mut self.reconnect_context;

				if let Some(existing) = connections.remove(&account_id) {
					match existing.state {
						ConnectionState::Connected(conn) => {
							conn.connection.close(0u32.into(), b"peer info updated");
						},
						ConnectionState::ReconnectionScheduled => {
							reconnect_context.reset(&account_id);
						},
						ConnectionState::Stale => {},
					}
				}

				// Add as stale - will connect on first message send
				connections.insert(
					account_id,
					ConnectionStateInfo {
						state: ConnectionState::Stale,
						last_activity: Cell::new(tokio::time::Instant::now()),
						info: peer_info,
					},
				);
			},
			PeerUpdate::Deregistered(account_id, ed_pubkey) => {
				if account_id == self.our_account_id {
					return;
				}

				self.allowlist.remove_peer(&ed_pubkey.0);
				self.reconnect_context.reset(&account_id);

				if let Some(peer) = self.active_connections.remove(&account_id) {
					if let ConnectionState::Connected(conn) = peer.state {
						conn.connection.close(0u32.into(), b"peer deregistered");
					}
				}
			},
		}
	}

	async fn connect_to_peer(
		&mut self,
		peer_info: PeerInfo,
		previous_activity: tokio::time::Instant,
	) {
		let account_id = peer_info.account_id.clone();

		match connect_to_peer(&self.endpoint, &peer_info).await {
			Ok(peer_conn) => {
				info!("Connected to peer {}", account_id);
				self.reconnect_context.reset(&account_id);

				self.active_connections.insert(
					account_id,
					ConnectionStateInfo {
						state: ConnectionState::Connected(peer_conn),
						last_activity: Cell::new(previous_activity),
						info: peer_info,
					},
				);
			},
			Err(e) => {
				warn!("Failed to connect to {}: {e}", account_id);
				self.reconnect_context.schedule_reconnect(account_id.clone());

				self.active_connections.insert(
					account_id,
					ConnectionStateInfo {
						state: ConnectionState::ReconnectionScheduled,
						last_activity: Cell::new(previous_activity),
						info: peer_info,
					},
				);
			},
		}
	}

	async fn reconnect_to_peer(&mut self, account_id: &AccountId) {
		if let Some(peer) = self.active_connections.remove(account_id) {
			match peer.state {
				ConnectionState::ReconnectionScheduled => {
					info!("Reconnecting to peer: {account_id}");
					self.connect_to_peer(peer.info.clone(), peer.last_activity.get()).await;
				},
				ConnectionState::Connected(_) => {
					debug!("Reconnection cancelled for {}: already connected", account_id);
					self.active_connections.insert(account_id.clone(), peer);
				},
				ConnectionState::Stale => {
					debug!("Reconnection cancelled for {}: connection is stale", account_id);
					self.active_connections.insert(account_id.clone(), peer);
				},
			}
		} else {
			debug!("Will not reconnect to deregistered peer: {}", account_id);
		}
	}

	fn check_activity(&mut self) {
		for (account_id, state) in &mut self.active_connections.map {
			if !matches!(state.state, ConnectionState::Stale) &&
				state.last_activity.get().elapsed() > MAX_INACTIVITY_THRESHOLD
			{
				debug!("Peer connection is deemed stale due to inactivity: {}", account_id);
				self.reconnect_context.reset(account_id);

				// Close the connection if active
				if let ConnectionState::Connected(conn) =
					std::mem::replace(&mut state.state, ConnectionState::Stale)
				{
					conn.connection.close(0u32.into(), b"stale connection");
				}
			}
		}
	}
}

/// Start the QUIC P2P transport.
///
/// This function has the same interface as `core::start` for ZMQ transport.
pub async fn start(
	p2p_key: P2PKey,
	port: Port,
	current_peers: Vec<PeerInfo>,
	our_account_id: AccountId,
	incoming_message_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	outgoing_message_receiver: UnboundedReceiver<OutgoingMessage>,
	peer_update_receiver: UnboundedReceiver<PeerUpdate>,
) -> anyhow::Result<()> {
	// Generate TLS certificate from Ed25519 signing key
	let identity = CertificateIdentity::from_ed25519(&p2p_key.signing_key)
		.context("Failed to generate TLS certificate")?;

	debug!("Our Ed25519 pubkey: {}", hex::encode(identity.ed25519_pubkey));

	// Create allowlist verifier
	let allowlist = Arc::new(AllowlistVerifier::new());

	// Configure QUIC endpoint
	let server_config = configure_server(&identity, allowlist.clone())?;
	let client_config = configure_client(&identity, allowlist.clone())?;
	let endpoint = create_endpoint(port, server_config, client_config)?;

	let (reconnect_sender, reconnect_receiver) = tokio::sync::mpsc::unbounded_channel();

	let mut context = QuicContext {
		endpoint: endpoint.clone(),
		allowlist: allowlist.clone(),
		active_connections: ActiveConnectionWrapper::new(),
		reconnect_context: ReconnectContext::new(reconnect_sender),
		our_account_id: our_account_id.clone(),
	};

	// Register initial peers
	debug!("Registering peer info for {} peers", current_peers.len());
	for peer_info in current_peers {
		context.on_peer_update(PeerUpdate::Registered(peer_info));
	}

	// Spawn listener task
	let listener_sender = incoming_message_sender;
	let listener_allowlist = allowlist;
	tokio::spawn(
		run_listener(endpoint, listener_sender, listener_allowlist)
			.instrument(info_span!("quic_listener")),
	);

	// Run control loop
	control_loop(context, outgoing_message_receiver, peer_update_receiver, reconnect_receiver)
		.instrument(info_span!("quic"))
		.await;

	Ok(())
}

/// Main control loop for the QUIC transport.
async fn control_loop(
	mut context: QuicContext,
	mut outgoing_message_receiver: UnboundedReceiver<OutgoingMessage>,
	mut peer_update_receiver: UnboundedReceiver<PeerUpdate>,
	mut reconnect_receiver: UnboundedReceiver<AccountId>,
) {
	let mut check_activity_interval = make_periodic_tick(ACTIVITY_CHECK_INTERVAL, false);

	loop {
		tokio::select! {
			Some(messages) = outgoing_message_receiver.recv() => {
				context.send_messages(messages).await;
			}
			Some(peer_update) = peer_update_receiver.recv() => {
				context.on_peer_update(peer_update);
			}
			Some(account_id) = reconnect_receiver.recv() => {
				context.reconnect_to_peer(&account_id).await;
			}
			_ = check_activity_interval.tick() => {
				context.check_activity();
			}
		}
	}
}

/// Listen for incoming QUIC connections and forward messages.
async fn run_listener(
	endpoint: Endpoint,
	incoming_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	allowlist: Arc<AllowlistVerifier>,
) {
	info!("QUIC listener started");

	while let Some(incoming) = endpoint.accept().await {
		let sender = incoming_sender.clone();
		let allowlist = allowlist.clone();

		tokio::spawn(async move {
			match incoming.await {
				Ok(connection) => {
					// Extract peer's Ed25519 pubkey from their certificate
					// We need to extract before any await to avoid Send issues
					let account_id = {
						let peer_certs = connection.peer_identity();
						match extract_peer_identity(&peer_certs, &allowlist) {
							Ok(id) => id,
							Err(e) => {
								warn!("Failed to verify peer identity: {e}");
								return;
							},
						}
					};

					debug!("Accepted connection from {}", account_id);

					// Handle incoming streams from this connection
					handle_incoming_connection(connection, account_id, sender).await;
				},
				Err(e) => {
					warn!("Incoming connection failed: {e}");
				},
			}
		});
	}
}

/// Extract and verify peer identity from TLS certificates.
fn extract_peer_identity(
	peer_identity: &Option<Box<dyn std::any::Any>>,
	allowlist: &AllowlistVerifier,
) -> anyhow::Result<AccountId> {
	let certs = peer_identity
		.as_ref()
		.and_then(|id| id.downcast_ref::<Vec<rustls_pki_types::CertificateDer<'static>>>())
		.ok_or_else(|| anyhow::anyhow!("No peer certificates"))?;

	let cert = certs.first().ok_or_else(|| anyhow::anyhow!("Empty certificate chain"))?;

	let pubkey = CertificateIdentity::extract_pubkey_from_cert(cert)?;

	allowlist
		.get_account_id(&pubkey)
		.ok_or_else(|| anyhow::anyhow!("Peer not on allowlist"))
}

/// Handle incoming streams from a connected peer.
async fn handle_incoming_connection(
	connection: quinn::Connection,
	account_id: AccountId,
	incoming_sender: UnboundedSender<(AccountId, Vec<u8>)>,
) {
	loop {
		match connection.accept_uni().await {
			Ok(mut recv_stream) => match receive_message(&mut recv_stream).await {
				Ok(payload) =>
					if incoming_sender.send((account_id.clone(), payload)).is_err() {
						debug!("Incoming message channel closed");
						return;
					},
				Err(e) => {
					trace!("Error receiving message: {e}");
				},
			},
			Err(quinn::ConnectionError::ApplicationClosed(_)) => {
				debug!("Connection closed by peer {}", account_id);
				return;
			},
			Err(e) => {
				warn!("Error accepting stream from {}: {e}", account_id);
				return;
			},
		}
	}
}
