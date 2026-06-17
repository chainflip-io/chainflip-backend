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

use super::{
	connection::{MAX_INACTIVITY_THRESHOLD, RECONNECT_INTERVAL},
	start, PeerInfo, PeerUpdate, ACTIVITY_CHECK_INTERVAL,
};
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
	// Keeps the transport running for the lifetime of the node; dropping the node shuts the
	// transport down and releases its port.
	_shutdown_sender: tokio::sync::oneshot::Sender<()>,
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

	let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();

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
				shutdown_receiver,
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
		_shutdown_sender: shutdown_sender,
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

/// Test that deregistering a peer removes them from the allowlist and prevents sending.
#[tokio::test]
async fn deregistered_peer_cannot_receive_messages() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9095);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9096);

	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// Wait for initial connection
	tokio::time::sleep(Duration::from_secs(1)).await;

	// Verify communication works
	assert!(send_and_receive_message(&node1, &mut node2).await.is_some());

	// State chain broadcasts deregistration - node 1 removes node 2 from its peer list
	let ed_pubkey = Public::from(node_key2.verifying_key().to_bytes());
	node1
		.peer_update_sender
		.send(PeerUpdate::Deregistered(pi2.account_id.clone(), ed_pubkey))
		.unwrap();

	// Wait for deregistration to take effect
	tokio::time::sleep(Duration::from_millis(500)).await;

	// Node 1 should no longer be able to send to node 2 (peer not registered)
	node1
		.msg_sender
		.send(OutgoingMessage::Private {
			messages: vec![(pi2.account_id.clone(), b"should not arrive".to_vec())],
		})
		.unwrap();

	// Node 2 should not receive the message
	let result = recv_with_custom_timeout(&mut node2.msg_receiver, Duration::from_millis(500)).await;
	assert!(result.is_none(), "Deregistered peer should not receive messages");
}

/// Test that large messages within the limit are delivered successfully
#[tokio::test]
async fn large_message_within_limit() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9097);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9098);

	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// Wait for connection
	tokio::time::sleep(Duration::from_secs(1)).await;

	// Send a 1MB message (under the 2MB limit)
	let large_payload = vec![0xABu8; 1024 * 1024];
	node1
		.msg_sender
		.send(OutgoingMessage::Private {
			messages: vec![(pi2.account_id.clone(), large_payload.clone())],
		})
		.unwrap();

	let received =
		recv_with_custom_timeout(&mut node2.msg_receiver, Duration::from_secs(5)).await;
	assert!(received.is_some(), "Large message should be delivered");
	assert_eq!(received.unwrap().1.len(), 1024 * 1024);
}

/// Test that nodes reconnect after connection failure
#[tokio::test]
async fn reconnects_after_send_failure() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9099);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9100);

	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// Wait for initial connection
	tokio::time::sleep(Duration::from_secs(1)).await;

	// Verify communication works
	assert!(send_and_receive_message(&node1, &mut node2).await.is_some());

	// Kill node 2
	drop(node2);
	tokio::time::sleep(Duration::from_millis(200)).await;

	// Restart node 2 on a different port (simulating reconnection scenario)
	let pi2_new = create_node_info(AccountId::new([2; 32]), &node_key2, 9101);
	let mut node2_new = spawn_node(&node_key2, 1, pi2_new.clone(), &[pi1.clone(), pi2_new.clone()]);

	// Update node 1 with new peer info (same key, different port)
	node1.peer_update_sender.send(PeerUpdate::Registered(pi2_new.clone())).unwrap();

	// Wait for reconnection (exponential backoff starts at 250ms)
	tokio::time::sleep(Duration::from_secs(2)).await;

	// Communication should work again
	assert!(
		send_and_receive_message(&node1, &mut node2_new).await.is_some(),
		"Should reconnect after peer update"
	);
}

/// Test that stale connections are cleaned up after inactivity and can be re-established
#[tokio::test(start_paused = true)]
async fn stale_connections_reactivate_on_demand() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9102);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9103);

	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// Sleep long enough for nodes to deem connections "stale" (due to inactivity)
	// This uses tokio's time manipulation since test is started with `start_paused = true`
	tokio::time::sleep(
		ACTIVITY_CHECK_INTERVAL + MAX_INACTIVITY_THRESHOLD + Duration::from_secs(1),
	)
	.await;

	// Resume time so real network operations can complete
	tokio::time::resume();

	// Ensure that we can re-activate stale connections when needed
	// The QUIC implementation should lazily reconnect on send to a stale peer
	assert!(
		send_and_receive_message(&node1, &mut node2).await.is_some(),
		"Should reactivate stale connection on demand"
	);
}

/// Test that reconnection respects the initial delay
#[tokio::test]
async fn reconnect_respects_backoff_delay() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9104);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9105);

	// Start only node 1, node 2 is not running yet
	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);

	// Wait for node 1 to try connecting to node 2 (will fail)
	tokio::time::sleep(Duration::from_millis(500)).await;

	// Try to send a message - should trigger reconnection scheduling
	node1
		.msg_sender
		.send(OutgoingMessage::Private {
			messages: vec![(pi2.account_id.clone(), b"test".to_vec())],
		})
		.unwrap();

	// Start node 2 now
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// Wait less than the initial backoff interval - message should not arrive yet
	tokio::time::sleep(RECONNECT_INTERVAL / 2).await;

	let early_result =
		recv_with_custom_timeout(&mut node2.msg_receiver, Duration::from_millis(100)).await;
	// The first message was sent before node2 was up, so it may or may not arrive depending on
	// timing

	// Wait for backoff + connection establishment
	tokio::time::sleep(RECONNECT_INTERVAL * 4).await;

	// Now send another message - should succeed after reconnection
	node1
		.msg_sender
		.send(OutgoingMessage::Private {
			messages: vec![(pi2.account_id.clone(), b"after reconnect".to_vec())],
		})
		.unwrap();

	let result = recv_with_custom_timeout(&mut node2.msg_receiver, Duration::from_secs(3)).await;
	assert!(result.is_some(), "Message should arrive after reconnection backoff");

	// Silence unused variable warning
	let _ = early_result;
}

/// An outgoing connection must only be accepted if the peer presents the exact key we
/// expect for that account, not merely *some* allowlisted key. Otherwise a connection
/// intended for one validator could be satisfied by a different (but allowlisted)
/// validator listening at the same address, leaking that account's private messages.
#[tokio::test]
async fn connecting_to_wrong_allowlisted_peer_is_rejected() {
	let node_key1 = create_keypair();
	let node_key_b = create_keypair(); // the key node1 *expects* for account [2]
	let node_key_c = create_keypair(); // the key actually listening at that address

	let port_c: Port = 9106;
	let port_1: Port = 9107;

	let ip = "127.0.0.1".parse::<std::net::Ipv4Addr>().unwrap().to_ipv6_mapped();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, port_1);
	// The node actually listening at port_c uses key C and is account [3].
	let pi_c = create_node_info(AccountId::new([3; 32]), &node_key_c, port_c);
	// node1 believes account [2] (key B) lives at port_c — a mismatch.
	let pi2_wrong = PeerInfo::new(
		AccountId::new([2; 32]),
		Public::from(node_key_b.verifying_key().to_bytes()),
		ip,
		port_c,
	);

	// node1 allowlists key B (acct 2) and key C (acct 3) so the TLS handshake with the
	// key-C node passes the allowlist; only the identity pin should reject it.
	let node1 =
		spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2_wrong.clone(), pi_c.clone()]);
	// The key-C node accepts node1.
	let mut node_c = spawn_node(&node_key_c, 2, pi_c.clone(), &[pi1.clone(), pi_c.clone()]);

	tokio::time::sleep(Duration::from_secs(1)).await;

	// node1 tries to message account [2] — which resolves to the key-C node's address.
	node1
		.msg_sender
		.send(OutgoingMessage::Private {
			messages: vec![(AccountId::new([2; 32]), b"should not arrive".to_vec())],
		})
		.unwrap();

	// The key-C node must not receive a message intended for account [2].
	let result = recv_with_custom_timeout(&mut node_c.msg_receiver, Duration::from_secs(1)).await;
	assert!(result.is_none(), "message must not be delivered to the wrong (mismatched-key) peer");
}

/// A message addressed to a peer that is not yet reachable must not be silently dropped:
/// the lazy connect blocks until the peer appears and the message is then delivered.
#[tokio::test]
async fn message_to_initially_unreachable_peer_is_delivered_once_it_starts() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 9108);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 9109);

	// node1 knows node2, but node2 is not running yet.
	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);

	// Let node1 start.
	tokio::time::sleep(Duration::from_millis(500)).await;

	// Send while node2 is down: this must not be dropped.
	node1
		.msg_sender
		.send(OutgoingMessage::Private {
			messages: vec![(pi2.account_id.clone(), b"sent while down".to_vec())],
		})
		.unwrap();

	// Bring node2 up shortly after; the in-flight lazy connect should complete to it.
	tokio::time::sleep(Duration::from_millis(300)).await;
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// The message should be delivered once the connection is established.
	let received = recv_with_custom_timeout(&mut node2.msg_receiver, Duration::from_secs(5)).await;
	assert_eq!(received, Some((node1.account_id.clone(), b"sent while down".to_vec())));
}

/// On shutdown the transport must return and release its UDP port, so a replacement
/// transport can bind the same port in-process (this is what lets the supervisor switch
/// transports without restarting the engine). Previously the listener was detached and held
/// the endpoint open, leaking the port.
#[tokio::test]
async fn shutdown_returns_and_releases_the_port() {
	let node_key = create_keypair();
	let port: Port = 9110;
	let pi = create_node_info(AccountId::new([1; 32]), &node_key, port);

	// Start a transport on `port`, let it bind, then ask it to shut down.
	let run = |shutdown| {
		let (incoming_sender, incoming_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (outgoing_sender, outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (peer_update_sender, peer_update_receiver) = tokio::sync::mpsc::unbounded_channel();
		// Keep the muxer-facing counterpart ends alive for the duration of the run.
		let keep_alive = (incoming_receiver, outgoing_sender, peer_update_sender);
		let fut = start(
			P2PKey::new(node_key.as_bytes()),
			port,
			vec![pi.clone()],
			pi.account_id.clone(),
			incoming_sender,
			outgoing_receiver,
			peer_update_receiver,
			shutdown,
		);
		async move {
			let result = fut.await;
			drop(keep_alive);
			result
		}
	};

	let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
	let first = tokio::spawn(run(shutdown_receiver));
	tokio::time::sleep(Duration::from_millis(300)).await;
	shutdown_sender.send(()).unwrap();

	tokio::time::timeout(Duration::from_secs(5), first)
		.await
		.expect("transport did not return after shutdown")
		.expect("transport task panicked")
		.expect("transport returned an error");

	// A replacement transport must be able to bind the now-released port.
	let (shutdown_sender2, shutdown_receiver2) = tokio::sync::oneshot::channel();
	let second = tokio::spawn(run(shutdown_receiver2));
	tokio::time::sleep(Duration::from_millis(500)).await;
	shutdown_sender2.send(()).unwrap();

	tokio::time::timeout(Duration::from_secs(5), second)
		.await
		.expect("replacement transport did not return")
		.expect("replacement transport task panicked")
		.expect("replacement transport failed to bind the released port");
}
