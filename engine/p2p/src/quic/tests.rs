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

use std::time::Duration;

use cf_utilities::{
	testing::{expect_recv_with_timeout, recv_with_custom_timeout},
	Port,
};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use sp_core::ed25519::Public;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info_span, Instrument};

use super::{start, PeerInfo, PeerUpdate};
use crate::{message::AccountId, OutgoingMessage, P2PKey};

/// Maximum time to wait for connection establishment
const MAX_CONNECTION_DELAY: Duration = Duration::from_millis(2000);

fn create_node_info(id: AccountId, node_key: &SigningKey, port: Port) -> PeerInfo {
	use std::net::Ipv4Addr;
	let ip = "127.0.0.1".parse::<Ipv4Addr>().unwrap().to_ipv6_mapped();
	let pubkey = Public::from(node_key.verifying_key().to_bytes());
	PeerInfo::new(id, pubkey, ip, port)
}

struct Node {
	account_id: AccountId,
	msg_sender: UnboundedSender<OutgoingMessage>,
	peer_update_sender: UnboundedSender<PeerUpdate>,
	msg_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
}

fn spawn_node(
	key: &SigningKey,
	idx: usize,
	our_peer_info: PeerInfo,
	peer_infos: &[PeerInfo],
) -> Node {
	let account_id = AccountId::new([idx as u8 + 1; 32]);

	let (incoming_message_sender, incoming_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (outgoing_message_sender, outgoing_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (peer_update_sender, peer_update_receiver) = tokio::sync::mpsc::unbounded_channel();

	// Create P2PKey from the secret key bytes
	let p2p_key = P2PKey::new(key.as_bytes());

	tokio::spawn({
		let peer_infos = peer_infos.to_vec();
		let account_id = account_id.clone();
		let port = our_peer_info.port;
		async move {
			start(
				p2p_key,
				port,
				peer_infos,
				account_id,
				incoming_message_sender,
				outgoing_message_receiver,
				peer_update_receiver,
			)
			.await
			.expect("QUIC transport failed");
		}
		.instrument(info_span!("node", idx = idx))
	});

	Node {
		account_id,
		msg_sender: outgoing_message_sender,
		peer_update_sender,
		msg_receiver: incoming_message_receiver,
	}
}

fn create_keypair() -> SigningKey {
	SigningKey::generate(&mut OsRng)
}

async fn send_and_receive_message(from: &Node, to: &mut Node) -> Option<(AccountId, Vec<u8>)> {
	from.msg_sender
		.send(OutgoingMessage::Private {
			messages: vec![(to.account_id.clone(), b"test".to_vec())],
		})
		.unwrap();

	recv_with_custom_timeout(&mut to.msg_receiver, MAX_CONNECTION_DELAY).await
}

/// Test that two nodes can connect and exchange messages
#[tokio::test]
async fn connect_two_nodes() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9087);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9088);

	// Node 1 knows about node 2 from startup
	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);

	// Node 2 only knows about itself initially
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), std::slice::from_ref(&pi2));

	// Wait for nodes to start
	tokio::time::sleep(Duration::from_millis(500)).await;

	// Node 2 learns about node 1 via peer update
	node2.peer_update_sender.send(PeerUpdate::Registered(pi1.clone())).unwrap();

	// Wait for connection establishment
	tokio::time::sleep(Duration::from_secs(1)).await;

	// Send message from node 1 to node 2
	node1
		.msg_sender
		.send(OutgoingMessage::Private {
			messages: vec![(pi2.account_id.clone(), b"hello from node 1".to_vec())],
		})
		.unwrap();

	let received = expect_recv_with_timeout(&mut node2.msg_receiver).await;
	assert_eq!(received.0, node1.account_id);
	assert_eq!(received.1, b"hello from node 1".to_vec());
}

/// Test that nodes can reconnect after a pubkey change
#[tokio::test]
async fn can_connect_after_pubkey_change() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9089);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9090);

	let mut node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// Wait for initial connection
	tokio::time::sleep(Duration::from_secs(1)).await;

	// Check bidirectional communication
	assert!(send_and_receive_message(&node2, &mut node1).await.is_some());
	assert!(send_and_receive_message(&node1, &mut node2).await.is_some());

	// Node 2 disconnects
	drop(node2);
	tokio::time::sleep(Duration::from_millis(200)).await;

	// Node 2 reconnects with a different key
	let node_key2b = create_keypair();
	let pi2b = create_node_info(AccountId::new([2; 32]), &node_key2b, 9091);
	let mut node2b = spawn_node(&node_key2b, 1, pi2b.clone(), &[pi1.clone(), pi2b.clone()]);

	// Node 1 learns about Node 2's new key
	node1.peer_update_sender.send(PeerUpdate::Registered(pi2b.clone())).unwrap();

	// Wait for reconnection
	tokio::time::sleep(Duration::from_secs(1)).await;

	// Communication should work again
	assert!(send_and_receive_message(&node2b, &mut node1).await.is_some());
	assert!(send_and_receive_message(&node1, &mut node2b).await.is_some());
}

/// Test broadcast message delivery
#[tokio::test]
async fn broadcast_message() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();
	let node_key3 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9092);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9093);
	let pi3 = create_node_info(AccountId::new([3; 32]), &node_key3, 9094);

	let all_peers = vec![pi1.clone(), pi2.clone(), pi3.clone()];

	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &all_peers);
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &all_peers);
	let mut node3 = spawn_node(&node_key3, 2, pi3.clone(), &all_peers);

	// Wait for connections
	tokio::time::sleep(Duration::from_secs(2)).await;

	// Broadcast from node 1 to nodes 2 and 3
	node1
		.msg_sender
		.send(OutgoingMessage::Broadcast {
			recipients: vec![pi2.account_id.clone(), pi3.account_id.clone()],
			payload: b"broadcast message".to_vec(),
		})
		.unwrap();

	// Both should receive the message
	let received2 = expect_recv_with_timeout(&mut node2.msg_receiver).await;
	let received3 = expect_recv_with_timeout(&mut node3.msg_receiver).await;

	assert_eq!(received2.1, b"broadcast message".to_vec());
	assert_eq!(received3.1, b"broadcast message".to_vec());
}
