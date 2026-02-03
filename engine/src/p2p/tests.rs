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

use std::{net::Ipv4Addr, time::Duration};

use cf_primitives::AccountId;
use cf_utilities::Port;
use ed25519_dalek::SigningKey;
use engine_p2p::{quic::PeerInfo, P2PKey, TopicMuxer};
use multisig::{p2p::OutgoingMultisigStageMessages, ChainTag};
use rand::rngs::OsRng;
use sp_core::ed25519::Public;
use tokio::sync::mpsc;
use tracing::{info, info_span, Instrument};

use super::multisig_adapter::{create_multisig_channels, MultisigTopic};

const BASE_PORT: Port = 19000;

/// Create a P2P key and peer info for a test node.
fn create_test_node_info(idx: usize) -> (SigningKey, PeerInfo, AccountId) {
	let signing_key = SigningKey::generate(&mut OsRng);
	let account_id = AccountId::new([(idx + 1) as u8; 32]);
	let ip = "127.0.0.1".parse::<Ipv4Addr>().unwrap().to_ipv6_mapped();
	let port = BASE_PORT + idx as Port;
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
	let node_infos: Vec<_> = (0..NUM_NODES).map(create_test_node_info).collect();
	let all_peer_infos: Vec<_> = node_infos.iter().map(|(_, pi, _)| pi.clone()).collect();
	let account_ids: Vec<_> = node_infos.iter().map(|(_, _, id)| id.clone()).collect();

	// Create channels and spawn QUIC transport for each node
	// We'll set up TopicMuxer for each node to test the full stack
	let mut eth_senders = Vec::new();
	let mut eth_receivers = Vec::new();

	for (idx, (signing_key, peer_info, account_id)) in node_infos.into_iter().enumerate() {
		let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
		let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
		let (_peer_update_tx, peer_update_rx) = mpsc::unbounded_channel();

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
	for i in 1..NUM_NODES {
		let received =
			tokio::time::timeout(Duration::from_secs(5), eth_receivers[i].receiver.recv())
				.await
				.expect("timeout waiting for broadcast")
				.expect("channel closed");

		assert_eq!(received.0, account_ids[0]);
		assert_eq!(received.1.payload, broadcast_payload);
	}
	info!("Broadcast message routing works!");

	// Test 3: All-to-all communication (simulating a ceremony round)
	// Each node sends a message to all other nodes
	for sender_idx in 0..NUM_NODES {
		let payload = format!("from node {}", sender_idx).into_bytes();
		let recipients: Vec<_> = (0..NUM_NODES)
			.filter(|&i| i != sender_idx)
			.map(|i| account_ids[i].clone())
			.collect();

		eth_senders[sender_idx]
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
