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

//! QUIC connection management.
//!
//! Manages outgoing connections to peers, including connection lifecycle,
//! reconnection with exponential backoff, and stale connection handling.

use std::{
	cell::Cell,
	collections::BTreeMap,
	net::{IpAddr, Ipv6Addr, SocketAddr},
	sync::Arc,
	time::Duration,
};

use anyhow::Context;
use cf_utilities::{metrics::P2P_ACTIVE_CONNECTIONS, Port};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use rustls::pki_types::ServerName;
use tokio::{sync::mpsc::UnboundedSender, time::Instant};
use tracing::{debug, info, trace};

use crate::message::AccountId;

use super::{auth::AllowlistVerifier, cert::CertificateIdentity, PeerInfo};

/// Reconnection interval constants (same as ZMQ)
pub const RECONNECT_INTERVAL: Duration = Duration::from_millis(250);
pub const RECONNECT_INTERVAL_MAX: Duration = Duration::from_secs(30);

/// Maximum message size (same as ZMQ: 2MB)
pub const MAX_MESSAGE_SIZE: usize = 2 * 1024 * 1024;

/// How long before a connection is considered stale
pub const MAX_INACTIVITY_THRESHOLD: Duration = Duration::from_secs(60 * 60);

/// Active connection to a peer.
pub struct PeerConnection {
	pub connection: Connection,
}

/// Connection state for a peer.
pub enum ConnectionState {
	/// Active QUIC connection
	Connected(PeerConnection),
	/// Waiting to reconnect (exponential backoff)
	ReconnectionScheduled,
	/// No active connection due to inactivity (will reconnect on demand)
	Stale,
}

/// Full connection state including metadata.
pub struct ConnectionStateInfo {
	pub state: ConnectionState,
	pub last_activity: Cell<Instant>,
	pub info: PeerInfo,
}

/// Wrapper for active connections with metrics.
pub struct ActiveConnectionWrapper {
	metric: &'static P2P_ACTIVE_CONNECTIONS,
	pub map: BTreeMap<AccountId, ConnectionStateInfo>,
}

impl ActiveConnectionWrapper {
	pub fn new() -> Self {
		ActiveConnectionWrapper { metric: &P2P_ACTIVE_CONNECTIONS, map: Default::default() }
	}

	pub fn get(&self, account_id: &AccountId) -> Option<&ConnectionStateInfo> {
		self.map.get(account_id)
	}

	pub fn get_mut(&mut self, account_id: &AccountId) -> Option<&mut ConnectionStateInfo> {
		self.map.get_mut(account_id)
	}

	pub fn insert(
		&mut self,
		key: AccountId,
		value: ConnectionStateInfo,
	) -> Option<ConnectionStateInfo> {
		let result = self.map.insert(key, value);
		self.metric.set(self.map.len());
		result
	}

	pub fn remove(&mut self, key: &AccountId) -> Option<ConnectionStateInfo> {
		let result = self.map.remove(key);
		self.metric.set(self.map.len());
		result
	}
}

impl Default for ActiveConnectionWrapper {
	fn default() -> Self {
		Self::new()
	}
}

/// Manages reconnection delays with exponential backoff.
pub struct ReconnectContext {
	reconnect_delays: BTreeMap<AccountId, Duration>,
	reconnect_sender: UnboundedSender<AccountId>,
}

impl ReconnectContext {
	pub fn new(reconnect_sender: UnboundedSender<AccountId>) -> Self {
		ReconnectContext { reconnect_delays: BTreeMap::new(), reconnect_sender }
	}

	/// Get the delay for the next reconnection attempt (with exponential backoff).
	pub fn get_delay_for(&mut self, account_id: &AccountId) -> Duration {
		use std::collections::btree_map::Entry;

		match self.reconnect_delays.entry(account_id.clone()) {
			Entry::Occupied(mut entry) => {
				let new_delay = std::cmp::min(*entry.get() * 2, RECONNECT_INTERVAL_MAX);
				*entry.get_mut() = new_delay;
				new_delay
			},
			Entry::Vacant(entry) => {
				entry.insert(RECONNECT_INTERVAL);
				RECONNECT_INTERVAL
			},
		}
	}

	/// Schedule a reconnection attempt after the appropriate delay.
	pub fn schedule_reconnect(&mut self, account_id: AccountId) {
		let delay = self.get_delay_for(&account_id);

		debug!("Will reconnect to {} in {:?}", account_id, delay);

		let sender = self.reconnect_sender.clone();
		tokio::spawn(async move {
			tokio::time::sleep(delay).await;
			let _ = sender.send(account_id);
		});
	}

	/// Reset the reconnection delay for a peer (called on successful connection).
	pub fn reset(&mut self, account_id: &AccountId) {
		if self.reconnect_delays.remove(account_id).is_some() {
			debug!("Reconnection delay for {} is reset", account_id);
		}
	}
}

/// Configure the QUIC server (for accepting incoming connections).
pub fn configure_server(
	identity: &CertificateIdentity,
	verifier: Arc<AllowlistVerifier>,
) -> anyhow::Result<ServerConfig> {
	let mut server_crypto = rustls::ServerConfig::builder()
		.with_client_cert_verifier(verifier)
		.with_single_cert(vec![identity.cert_der.clone()], identity.key_der.clone_key())
		.context("Failed to configure server TLS")?;

	// Only allow TLS 1.3
	server_crypto.alpn_protocols = vec![b"cf-p2p".to_vec()];

	let mut server_config = ServerConfig::with_crypto(Arc::new(
		quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)?,
	));

	// Configure transport parameters
	let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
	transport_config.max_idle_timeout(Some(Duration::from_secs(60).try_into().unwrap()));

	Ok(server_config)
}

/// Configure the QUIC client (for outgoing connections).
pub fn configure_client(
	identity: &CertificateIdentity,
	verifier: Arc<AllowlistVerifier>,
) -> anyhow::Result<ClientConfig> {
	let mut client_crypto = rustls::ClientConfig::builder()
		.dangerous()
		.with_custom_certificate_verifier(verifier)
		.with_client_auth_cert(vec![identity.cert_der.clone()], identity.key_der.clone_key())
		.context("Failed to configure client TLS")?;

	// Only allow TLS 1.3
	client_crypto.alpn_protocols = vec![b"cf-p2p".to_vec()];

	let client_config = ClientConfig::new(Arc::new(
		quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)?,
	));

	Ok(client_config)
}

/// Create a QUIC endpoint that can both accept and initiate connections.
pub fn create_endpoint(
	port: Port,
	server_config: ServerConfig,
	client_config: ClientConfig,
) -> anyhow::Result<Endpoint> {
	let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port);

	let mut endpoint = Endpoint::server(server_config, addr)?;
	endpoint.set_default_client_config(client_config);

	info!("QUIC endpoint listening on {}", addr);

	Ok(endpoint)
}

/// Connect to a peer and return the connection.
pub async fn connect_to_peer(
	endpoint: &Endpoint,
	peer: &PeerInfo,
) -> anyhow::Result<PeerConnection> {
	let addr = SocketAddr::new(IpAddr::V6(peer.ip), peer.port);

	// Use a dummy server name - we verify via the certificate's embedded pubkey
	let server_name: ServerName<'_> = ServerName::try_from("chainflip-peer")
		.map_err(|_| anyhow::anyhow!("Invalid server name"))?;

	debug!("Connecting to peer {} at {}", peer.account_id, addr);

	let connection = endpoint.connect(addr, &server_name.to_str())?.await?;

	// Pin the peer identity. The allowlist verifier only proves the server is *some*
	// registered validator; ensure it is the exact peer we intended to reach, so this
	// account's private messages can never be sent to a different (but allowlisted) node.
	let presented_pubkey = {
		let peer_identity = connection.peer_identity();
		let certs = peer_identity
			.as_ref()
			.and_then(|id| id.downcast_ref::<Vec<rustls::pki_types::CertificateDer<'static>>>())
			.ok_or_else(|| anyhow::anyhow!("peer presented no certificate"))?;
		let cert = certs.first().ok_or_else(|| anyhow::anyhow!("empty peer certificate chain"))?;
		CertificateIdentity::extract_pubkey_from_cert(cert)?
	};
	if presented_pubkey != peer.ed_pubkey {
		connection.close(0u32.into(), b"unexpected peer identity");
		anyhow::bail!(
			"Connected to unexpected peer at {}: expected key {}, got {}",
			addr,
			hex::encode(peer.ed_pubkey),
			hex::encode(presented_pubkey),
		);
	}

	info!("Connected to peer {}", peer.account_id);

	Ok(PeerConnection { connection })
}

/// Send a message to a peer over QUIC.
///
/// Opens a unidirectional stream for each message (simple, no head-of-line blocking).
pub async fn send_message(connection: &Connection, payload: Vec<u8>) -> anyhow::Result<()> {
	if payload.len() > MAX_MESSAGE_SIZE {
		anyhow::bail!("Message too large: {} bytes (max {})", payload.len(), MAX_MESSAGE_SIZE);
	}

	let mut send_stream = connection.open_uni().await?;

	// Write length prefix (4 bytes big-endian)
	let len = (payload.len() as u32).to_be_bytes();
	send_stream.write_all(&len).await?;

	// Write payload
	send_stream.write_all(&payload).await?;

	// Finish the stream
	send_stream.finish()?;

	trace!("Sent {} bytes", payload.len());

	Ok(())
}

/// Receive a message from a QUIC stream.
pub async fn receive_message(recv_stream: &mut quinn::RecvStream) -> anyhow::Result<Vec<u8>> {
	// Read length prefix
	let mut len_buf = [0u8; 4];
	recv_stream.read_exact(&mut len_buf).await?;
	let len = u32::from_be_bytes(len_buf) as usize;

	if len > MAX_MESSAGE_SIZE {
		anyhow::bail!("Message too large: {} bytes (max {})", len, MAX_MESSAGE_SIZE);
	}

	// Read payload
	let mut payload = vec![0u8; len];
	recv_stream.read_exact(&mut payload).await?;

	trace!("Received {} bytes", len);

	Ok(payload)
}

#[cfg(test)]
mod tests {
	use super::*;

	const ACCOUNT_1: AccountId = AccountId::new([1; 32]);
	const ACCOUNT_2: AccountId = AccountId::new([2; 32]);

	#[test]
	fn reconnect_context_starts_with_initial_interval() {
		let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
		let mut ctx = ReconnectContext::new(sender);

		let delay = ctx.get_delay_for(&ACCOUNT_1);
		assert_eq!(delay, RECONNECT_INTERVAL);
	}

	#[test]
	fn reconnect_context_doubles_delay_on_each_call() {
		let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
		let mut ctx = ReconnectContext::new(sender);

		let delay1 = ctx.get_delay_for(&ACCOUNT_1);
		let delay2 = ctx.get_delay_for(&ACCOUNT_1);
		let delay3 = ctx.get_delay_for(&ACCOUNT_1);

		assert_eq!(delay1, Duration::from_millis(250));
		assert_eq!(delay2, Duration::from_millis(500));
		assert_eq!(delay3, Duration::from_millis(1000));
	}

	#[test]
	fn reconnect_context_caps_at_max_interval() {
		let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
		let mut ctx = ReconnectContext::new(sender);

		// Call enough times to exceed the max (250ms -> 500ms -> 1s -> 2s -> 4s -> 8s -> 16s ->
		// 32s)
		for _ in 0..10 {
			ctx.get_delay_for(&ACCOUNT_1);
		}

		let delay = ctx.get_delay_for(&ACCOUNT_1);
		assert_eq!(delay, RECONNECT_INTERVAL_MAX);
	}

	#[test]
	fn reconnect_context_reset_clears_delay() {
		let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
		let mut ctx = ReconnectContext::new(sender);

		// Build up delay
		ctx.get_delay_for(&ACCOUNT_1);
		ctx.get_delay_for(&ACCOUNT_1);
		ctx.get_delay_for(&ACCOUNT_1);

		// Reset
		ctx.reset(&ACCOUNT_1);

		// Should start fresh
		let delay = ctx.get_delay_for(&ACCOUNT_1);
		assert_eq!(delay, RECONNECT_INTERVAL);
	}

	#[test]
	fn reconnect_context_tracks_peers_independently() {
		let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
		let mut ctx = ReconnectContext::new(sender);

		// Build up delay for ACCOUNT_1
		ctx.get_delay_for(&ACCOUNT_1);
		ctx.get_delay_for(&ACCOUNT_1);
		ctx.get_delay_for(&ACCOUNT_1);

		// ACCOUNT_2 should still start fresh
		let delay2 = ctx.get_delay_for(&ACCOUNT_2);
		assert_eq!(delay2, RECONNECT_INTERVAL);

		// ACCOUNT_1 should continue from where it left off
		let delay1 = ctx.get_delay_for(&ACCOUNT_1);
		assert_eq!(delay1, Duration::from_millis(2000));
	}

	#[tokio::test]
	async fn schedule_reconnect_sends_account_id_after_delay() {
		let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
		let mut ctx = ReconnectContext::new(sender);

		let start = tokio::time::Instant::now();
		ctx.schedule_reconnect(ACCOUNT_1.clone());

		// Should receive the account ID after the initial delay
		let received = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
			.await
			.expect("timeout waiting for reconnect")
			.expect("channel closed");

		assert_eq!(received, ACCOUNT_1);
		assert!(start.elapsed() >= RECONNECT_INTERVAL);
	}
}
