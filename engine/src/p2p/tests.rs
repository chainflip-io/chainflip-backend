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

//! Integration tests for P2P + Multisig over real QUIC transport.
//!
//! These tests verify that the P2P layer correctly routes multisig messages
//! through actual QUIC connections.

use std::{collections::BTreeSet, net::Ipv4Addr, sync::Arc, time::Duration};

use cf_primitives::{AccountId, GENESIS_EPOCH};
use cf_utilities::{testing::new_temp_directory_with_nonexistent_file, Port};
use ed25519_dalek::SigningKey;
use engine_p2p::{quic::PeerInfo, P2PKey, TopicMuxer, Transport};
use futures::future::join_all;
use multisig::{
	client::MultisigClientApi,
	eth::{EthSigning, EvmCryptoScheme},
	p2p::OutgoingMultisigStageMessages,
	ChainTag, CryptoScheme, KeyId, MultisigClient,
};
use rand::rngs::OsRng;
use sp_core::ed25519::Public;
use tokio::sync::mpsc;
use tracing::{info, info_span, Instrument};

use super::multisig_adapter::{create_multisig_channels, MultisigTopic};
use crate::{
	db::{KeyStore, PersistentKeyDB},
	multisig::start_client,
};

const BASE_PORT: Port = 19000;
/// A separate port range for the ceremony test so it never clashes with the routing test
/// when the suite runs in parallel.
const CEREMONY_BASE_PORT: Port = 19100;
/// And another range for the mid-ceremony switch test.
const MID_CEREMONY_BASE_PORT: Port = 19200;

/// Create a P2P key and peer info for a test node.
fn create_test_node_info(idx: usize, base_port: Port) -> (SigningKey, PeerInfo, AccountId) {
	let signing_key = SigningKey::generate(&mut OsRng);
	let account_id = AccountId::new([(idx + 1) as u8; 32]);
	let ip = "127.0.0.1".parse::<Ipv4Addr>().unwrap().to_ipv6_mapped();
	let port = base_port + idx as Port;
	let pubkey = Public::from(signing_key.verifying_key().to_bytes());
	let peer_info = PeerInfo::new(account_id.clone(), pubkey, ip, port);

	(signing_key, peer_info, account_id)
}

/// Test that messages flow correctly through the full P2P + TopicMuxer + MultisigChannels stack.
///
/// This test verifies:
/// 1. QUIC transport can establish connections between nodes
/// 2. TopicMuxer correctly routes messages by topic
/// 3. MultisigChannels adapter correctly translates message formats
#[tokio::test]
async fn multisig_messages_over_quic_with_muxer() {
	const NUM_NODES: usize = 3;

	// Create peer infos for all nodes
	let node_infos: Vec<_> = (0..NUM_NODES).map(|idx| create_test_node_info(idx, BASE_PORT)).collect();
	let all_peer_infos: Vec<_> = node_infos.iter().map(|(_, pi, _)| pi.clone()).collect();
	let account_ids: Vec<_> = node_infos.iter().map(|(_, _, id)| id.clone()).collect();

	// Create channels and spawn QUIC transport for each node
	// We'll set up TopicMuxer for each node to test the full stack
	let mut eth_senders = Vec::new();
	let mut eth_receivers = Vec::new();
	// Keep the shutdown senders alive so the transports keep running for the whole test.
	let mut shutdown_senders = Vec::new();

	for (idx, (signing_key, peer_info, account_id)) in node_infos.into_iter().enumerate() {
		let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
		let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
		let (_peer_update_tx, peer_update_rx) = mpsc::unbounded_channel();
		let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
		shutdown_senders.push(shutdown_tx);

		let p2p_key = P2PKey::new(signing_key.as_bytes());
		let port = peer_info.port;
		let our_account_id = account_id.clone();
		let peers = all_peer_infos.clone();

		// Spawn the QUIC transport
		tokio::spawn(
			async move {
				engine_p2p::quic::start(
					p2p_key,
					port,
					peers,
					our_account_id,
					incoming_tx,
					outgoing_rx,
					peer_update_rx,
					shutdown_rx,
				)
				.await
				.expect("QUIC transport failed");
			}
			.instrument(info_span!("quic_node", idx = idx)),
		);

		// Set up TopicMuxer for this node
		let (muxer_future, mut handles) =
			TopicMuxer::start(incoming_rx, outgoing_tx, [MultisigTopic(ChainTag::Ethereum)]);
		tokio::spawn(muxer_future.instrument(info_span!("muxer", idx = idx)));

		// Create multisig channels for Ethereum
		let eth_handle = handles.remove(&MultisigTopic(ChainTag::Ethereum)).unwrap();
		let (eth_sender, eth_receiver) =
			create_multisig_channels::<cf_chains::evm::EvmCrypto>(eth_handle);

		eth_senders.push(eth_sender);
		eth_receivers.push(eth_receiver);
	}

	// Wait for connections to establish
	tokio::time::sleep(Duration::from_secs(2)).await;

	// Test 1: Send a private message from node 0 to node 1
	let test_payload = b"test multisig stage message".to_vec();
	eth_senders[0]
		.inner()
		.send(OutgoingMultisigStageMessages::Private(vec![(
			account_ids[1].clone(),
			test_payload.clone(),
		)]))
		.unwrap();

	// Node 1 should receive it
	let received = tokio::time::timeout(Duration::from_secs(5), eth_receivers[1].receiver.recv())
		.await
		.expect("timeout waiting for message")
		.expect("channel closed");

	assert_eq!(received.0, account_ids[0]);
	assert_eq!(received.1.payload, test_payload);
	info!("Private message routing works!");

	// Test 2: Broadcast from node 0 to nodes 1 and 2
	let broadcast_payload = b"broadcast stage message".to_vec();
	let recipients = vec![account_ids[1].clone(), account_ids[2].clone()];

	eth_senders[0]
		.inner()
		.send(OutgoingMultisigStageMessages::Broadcast(recipients, broadcast_payload.clone()))
		.unwrap();

	// Both node 1 and node 2 should receive it
	for eth_receiver in &mut eth_receivers[1..NUM_NODES] {
		let received =
			tokio::time::timeout(Duration::from_secs(5), eth_receiver.receiver.recv())
				.await
				.expect("timeout waiting for broadcast")
				.expect("channel closed");

		assert_eq!(received.0, account_ids[0]);
		assert_eq!(received.1.payload, broadcast_payload);
	}
	info!("Broadcast message routing works!");

	// Test 3: All-to-all communication (simulating a ceremony round)
	// Each node sends a message to all other nodes
	for (sender_idx, eth_sender) in eth_senders.iter().enumerate() {
		let payload = format!("from node {}", sender_idx).into_bytes();
		let recipients: Vec<_> = (0..NUM_NODES)
			.filter(|&i| i != sender_idx)
			.map(|i| account_ids[i].clone())
			.collect();

		eth_sender
			.inner()
			.send(OutgoingMultisigStageMessages::Broadcast(recipients, payload))
			.unwrap();
	}

	// Each node should receive messages from all other nodes (order not guaranteed)
	for receiver_idx in 0..NUM_NODES {
		let expected_count = NUM_NODES - 1; // Messages from all other nodes
		let mut received_from = std::collections::HashSet::new();

		for _ in 0..expected_count {
			let received = tokio::time::timeout(
				Duration::from_secs(5),
				eth_receivers[receiver_idx].receiver.recv(),
			)
			.await
			.expect("timeout waiting for all-to-all message")
			.expect("channel closed");

			// Verify the message content matches the sender
			let sender_idx =
				account_ids.iter().position(|id| *id == received.0).expect("unknown sender");
			let expected_payload = format!("from node {}", sender_idx).into_bytes();
			assert_eq!(received.1.payload, expected_payload);

			received_from.insert(received.0);
		}

		// Verify we received from all other nodes
		assert_eq!(received_from.len(), expected_count);
		assert!(!received_from.contains(&account_ids[receiver_idx]));
	}
	info!("All-to-all communication works! (simulates ceremony round)");
}

/// A self-contained multisig node: a real `MultisigClient` wired to a real transport via the
/// supervisor and topic muxer, with no engine or State Chain involved.
struct CeremonyNode {
	account_id: AccountId,
	client: MultisigClient<EthSigning, KeyStore<EthSigning>>,
	/// Send a transport here to make the supervisor switch to it in-process.
	restart_sender: mpsc::UnboundedSender<Transport>,
	// Kept alive for the lifetime of the node.
	_peer_update_sender: mpsc::UnboundedSender<engine_p2p::PeerUpdate>,
	_tempdir: tempfile::TempDir,
}

/// Assemble one node: transport supervisor (started on `initial_transport`) -> topic muxer ->
/// Ethereum multisig client, all over a real loopback socket.
fn spawn_ceremony_node(
	idx: usize,
	signing_key: SigningKey,
	account_id: AccountId,
	port: Port,
	all_peers: Vec<PeerInfo>,
	initial_transport: Transport,
) -> CeremonyNode {
	// Channels between the transport (owned by the supervisor) and the muxer.
	let (to_muxer_sender, to_muxer_receiver) = mpsc::unbounded_channel();
	let (from_muxer_sender, from_muxer_receiver) = mpsc::unbounded_channel();

	// Topic muxer with a single Ethereum topic.
	let (muxer_future, mut handles) =
		TopicMuxer::start(to_muxer_receiver, from_muxer_sender, [MultisigTopic(ChainTag::Ethereum)]);
	tokio::spawn(muxer_future.instrument(info_span!("muxer", idx = idx)));

	let eth_handle = handles.remove(&MultisigTopic(ChainTag::Ethereum)).unwrap();
	let (eth_sender, eth_receiver) =
		create_multisig_channels::<cf_chains::evm::EvmCrypto>(eth_handle);

	// Real multisig client backed by a temp on-disk key store.
	let (tempdir, db_file) = new_temp_directory_with_nonexistent_file();
	let key_store = KeyStore::<EthSigning>::new(Arc::new(
		PersistentKeyDB::open_and_migrate_to_latest(&db_file, None).expect("Failed to open database"),
	));
	let (client, client_future) =
		start_client::<EthSigning>(account_id.clone(), key_store, eth_receiver, eth_sender, 0);
	tokio::spawn(client_future.instrument(info_span!("multisig", idx = idx)));

	// Transport supervisor (owns the restartable ZMQ/QUIC transport).
	let (peer_update_sender, peer_update_receiver) = mpsc::unbounded_channel();
	let (restart_sender, restart_receiver) = mpsc::unbounded_channel();
	let p2p_key = P2PKey::new(signing_key.as_bytes());
	let supervisor_account_id = account_id.clone();
	tokio::spawn(
		async move {
			engine_p2p::supervisor::run_transport_supervisor(
				initial_transport,
				p2p_key,
				port,
				supervisor_account_id,
				all_peers,
				to_muxer_sender,
				from_muxer_receiver,
				peer_update_receiver,
				restart_receiver,
			)
			.await
			.expect("transport supervisor exited");
		}
		.instrument(info_span!("supervisor", idx = idx)),
	);

	CeremonyNode {
		account_id,
		client,
		restart_sender,
		_peer_update_sender: peer_update_sender,
		_tempdir: tempdir,
	}
}

/// End-to-end, engine-free test that signing keeps working *after* an in-process transport
/// switch: stand up real multisig clients over a real transport, run a keygen + signing on ZMQ,
/// switch every node to QUIC in place (between ceremonies), and run another signing — proving
/// the clients and the key survive the switch without restarting anything above the transport
/// layer. (For what happens to a ceremony that is in flight *during* a switch, see
/// `signing_fails_then_recovers_when_transport_switches_mid_ceremony`.)
#[tokio::test]
async fn signing_works_after_in_place_transport_switch() {
	const NUM_NODES: usize = 3;

	let node_infos: Vec<_> =
		(0..NUM_NODES).map(|idx| create_test_node_info(idx, CEREMONY_BASE_PORT)).collect();
	let all_peer_infos: Vec<_> = node_infos.iter().map(|(_, pi, _)| pi.clone()).collect();

	// Start every node on ZMQ (the network-wide default).
	let nodes: Vec<CeremonyNode> = node_infos
		.into_iter()
		.enumerate()
		.map(|(idx, (signing_key, peer_info, account_id))| {
			spawn_ceremony_node(
				idx,
				signing_key,
				account_id,
				peer_info.port,
				all_peer_infos.clone(),
				Transport::Zmq,
			)
		})
		.collect();

	let participants: BTreeSet<AccountId> = nodes.iter().map(|n| n.account_id.clone()).collect();

	// Give the ZMQ transports time to connect.
	tokio::time::sleep(Duration::from_secs(3)).await;

	// --- Keygen over ZMQ ---
	let public_keys = with_timeout(
		"keygen",
		join_all(
			nodes
				.iter()
				.map(|n| n.client.initiate_keygen(1, GENESIS_EPOCH, participants.clone())),
		),
	)
	.await
	.into_iter()
	.map(|r| r.expect("keygen failed"))
	.collect::<Vec<_>>();

	let public_key = public_keys[0];
	assert!(public_keys.iter().all(|pk| *pk == public_key), "all nodes must agree on the key");
	let key_id = KeyId::new(GENESIS_EPOCH, public_key);
	info!("Keygen succeeded over ZMQ");

	let payload = EvmCryptoScheme::signing_payload_for_test();

	// --- Signing over ZMQ ---
	run_signing(&nodes, 2, &participants, &key_id, &payload, &public_key).await;
	info!("Signing succeeded over ZMQ");

	// --- Switch every node to QUIC in-process ---
	for node in &nodes {
		node.restart_sender.send(Transport::Quic).expect("supervisor still running");
	}
	// Allow the transports to tear down ZMQ and re-establish QUIC connections.
	tokio::time::sleep(Duration::from_secs(5)).await;
	info!("Switched all nodes to QUIC");

	// --- Signing over QUIC, using the key generated before the switch ---
	run_signing(&nodes, 3, &participants, &key_id, &payload, &public_key).await;
	info!("Signing succeeded over QUIC after the in-process transport switch");
}

/// Characterise what happens to a ceremony that is in flight *while* the transport changes —
/// the scenario when QUIC has to be reverted to ZMQ (or vice versa) under us.
///
/// A network-wide switch is not atomic: nodes pick up the new transport at slightly different
/// finalized blocks, so for a window the validators are split across two non-interoperable
/// transports. We reproduce that split (one node on QUIC, the rest on ZMQ) and show:
///
///  1. A ceremony that needs a node on the far side of the split does **not** hang or panic —
///     it self-times-out (MAX_STAGE_DURATION) and resolves to an error, which the State Chain
///     would observe and retry.
///  2. Once the nodes converge onto one transport, signing works again (here with the two
///     nodes that ended up together, which meet the 2-of-3 success threshold) — so the switch
///     is recoverable, not a dead end.
#[tokio::test]
async fn signing_fails_then_recovers_when_transport_switches_mid_ceremony() {
	const NUM_NODES: usize = 3;

	let node_infos: Vec<_> =
		(0..NUM_NODES).map(|idx| create_test_node_info(idx, MID_CEREMONY_BASE_PORT)).collect();
	let all_peer_infos: Vec<_> = node_infos.iter().map(|(_, pi, _)| pi.clone()).collect();

	// Everyone starts on ZMQ.
	let nodes: Vec<CeremonyNode> = node_infos
		.into_iter()
		.enumerate()
		.map(|(idx, (signing_key, peer_info, account_id))| {
			spawn_ceremony_node(
				idx,
				signing_key,
				account_id,
				peer_info.port,
				all_peer_infos.clone(),
				Transport::Zmq,
			)
		})
		.collect();

	let all_signers: BTreeSet<AccountId> = nodes.iter().map(|n| n.account_id.clone()).collect();

	tokio::time::sleep(Duration::from_secs(3)).await;

	// Generate a key while everyone is still on ZMQ.
	let public_keys = with_timeout(
		"keygen",
		join_all(nodes.iter().map(|n| n.client.initiate_keygen(1, GENESIS_EPOCH, all_signers.clone()))),
	)
	.await
	.into_iter()
	.map(|r| r.expect("keygen failed"))
	.collect::<Vec<_>>();
	let public_key = public_keys[0];
	let key_id = KeyId::new(GENESIS_EPOCH, public_key);
	let payload = EvmCryptoScheme::signing_payload_for_test();

	// Create the transient split: node 0 moves to QUIC while nodes 1 and 2 stay on ZMQ. The two
	// transports cannot talk to each other, so node 0 is now isolated.
	nodes[0].restart_sender.send(Transport::Quic).expect("supervisor running");
	tokio::time::sleep(Duration::from_secs(3)).await;
	info!("Node 0 switched to QUIC; nodes 1 and 2 still on ZMQ (split)");

	// A signing ceremony that includes the isolated node cannot complete across the split. The
	// important property: it fails (by self-timeout) rather than hanging or panicking.
	let split_results = with_timeout(
		"signing across the transport split",
		join_all(nodes.iter().map(|n| {
			n.client.initiate_signing(2, all_signers.clone(), vec![(key_id.clone(), payload.clone())])
		})),
	)
	.await;
	for result in split_results {
		assert!(
			result.is_err(),
			"a ceremony spanning a transport split must fail cleanly, not hang"
		);
	}
	info!("Ceremony failed cleanly while the nodes were split across transports");

	// Converge nodes 1 and 2 onto QUIC. They were together on ZMQ throughout, so once both are
	// on QUIC they can sign without the isolated node (success threshold is 2 of 3). This models
	// the State Chain retrying the ceremony once the network has settled on one transport.
	nodes[1].restart_sender.send(Transport::Quic).expect("supervisor running");
	nodes[2].restart_sender.send(Transport::Quic).expect("supervisor running");
	tokio::time::sleep(Duration::from_secs(5)).await;

	let recovery_signers: BTreeSet<AccountId> =
		nodes[1..3].iter().map(|n| n.account_id.clone()).collect();
	run_signing(&nodes[1..3], 3, &recovery_signers, &key_id, &payload, &public_key).await;
	info!("Signing recovered on QUIC after the mid-ceremony transport split");
}

/// Run a signing ceremony across all nodes and assert every node produces a signature that
/// verifies against the group public key.
async fn run_signing(
	nodes: &[CeremonyNode],
	ceremony_id: cf_primitives::CeremonyId,
	signers: &BTreeSet<AccountId>,
	key_id: &KeyId,
	payload: &<EvmCryptoScheme as CryptoScheme>::SigningPayload,
	public_key: &<EvmCryptoScheme as CryptoScheme>::PublicKey,
) {
	let results = with_timeout(
		"signing",
		join_all(nodes.iter().map(|n| {
			n.client.initiate_signing(
				ceremony_id,
				signers.clone(),
				vec![(key_id.clone(), payload.clone())],
			)
		})),
	)
	.await;

	for result in results {
		let signatures = result.expect("signing failed");
		EvmCryptoScheme::verify_signature(&signatures[0], public_key, payload)
			.expect("produced signature must verify against the group key");
	}
}

/// Await a future, failing the test with a clear message if a ceremony stalls. The bound is
/// generous enough to allow a ceremony to fail by self-timeout (MAX_STAGE_DURATION = 30s) while
/// still catching a genuine hang.
async fn with_timeout<F: std::future::Future>(what: &str, fut: F) -> F::Output {
	tokio::time::timeout(Duration::from_secs(90), fut)
		.await
		.unwrap_or_else(|_| panic!("{what} timed out"))
}
