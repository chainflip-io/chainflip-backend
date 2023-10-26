use super::{PeerInfo, PeerUpdate};
use crate::p2p::{OutgoingMultisigStageMessages, P2PKey};
use sp_core::ed25519::Public;
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info_span, Instrument};
use utilities::{
	testing::{expect_recv_with_timeout, recv_with_custom_timeout},
	Port,
};

fn create_node_info(id: AccountId, node_key: &ed25519_dalek::Keypair, port: Port) -> PeerInfo {
	use std::net::Ipv4Addr;
	let ip = "0.0.0.0".parse::<Ipv4Addr>().unwrap().to_ipv6_mapped();
	let pubkey = Public(node_key.public.to_bytes());
	PeerInfo::new(id, pubkey, ip, port)
}

use std::time::Duration;

/// This has to be large enough to account for the possibility of
/// the initial handshake failing and the node having to reconnect
/// after `RECONNECT_INTERVAL`
const MAX_CONNECTION_DELAY: Duration = Duration::from_millis(500);

struct Node {
	account_id: AccountId,
	msg_sender: UnboundedSender<OutgoingMultisigStageMessages>,
	peer_update_sender: UnboundedSender<PeerUpdate>,
	msg_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
}

fn spawn_node(
	key: &ed25519_dalek::Keypair,
	idx: usize,
	our_peer_info: PeerInfo,
	peer_infos: &[PeerInfo],
) -> Node {
	let account_id = AccountId::new([idx as u8 + 1; 32]);

	// Secret key does not implement clone:
	let secret = ed25519_dalek::SecretKey::from_bytes(&key.secret.to_bytes()).unwrap();
	let key = P2PKey::new(secret);

	let (incoming_message_sender, incoming_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (outgoing_message_sender, outgoing_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (peer_update_sender, peer_update_receiver) = tokio::sync::mpsc::unbounded_channel();

	tokio::spawn({
		super::start(
			key,
			our_peer_info.port,
			peer_infos.to_vec(),
			account_id.clone(),
			incoming_message_sender,
			outgoing_message_receiver,
			peer_update_receiver,
		)
		.instrument(info_span!("node", idx = idx))
	});

	Node {
		account_id,
		msg_sender: outgoing_message_sender,
		peer_update_sender,
		msg_receiver: incoming_message_receiver,
	}
}

// Create an x25519 keypair along with the corresponding ed25519 public key
fn create_keypair() -> ed25519_dalek::Keypair {
	use rand::RngCore;
	let mut secret_key_bytes = [0; 32];
	rand::thread_rng().fill_bytes(&mut secret_key_bytes);

	let secret = ed25519_dalek::SecretKey::from_bytes(&secret_key_bytes).expect("invalid key size");
	let public: ed25519_dalek::PublicKey = (&secret).into();

	ed25519_dalek::Keypair { secret, public }
}

/// Ensure that a node can (eventually) receive messages from a peer
/// even if the latter initially fails the authentication check
// TODO: consider breaking this into more granular tests
#[tokio::test]
async fn connect_two_nodes() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	// TODO: automatically select ports to avoid any potential conflicts
	// with other tests
	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 8087);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 8088);

	// Node 1 knows about node 2 from the startup
	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);
	// ----------------------------------------------------------------
	// At this point node 1 may already attempt to connect to node 2,
	// but fail due to node 2 possibly being offline. The reconnection
	// in this case will be automatically handled by ZMQ and we are not
	// testing it explicitly here.
	// ----------------------------------------------------------------

	// Node 2 only knows about itself from the startup
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi2.clone()]);

	// ----------------------------------------------------------------
	// Node 2 should start around this time, and receive a connection
	// request from node 1, which it will reject due to node 1 not being
	// on its allow list. ZMQ won't automatically reconnect node 1 in this
	// case, but we have custom logic to handle this case as this test
	// demonstrates.
	// ----------------------------------------------------------------

	let peer_sender = node2.peer_update_sender.clone();

	// After some delay, node 2 receives info about node 1 and will
	// then allow connection from that node.
	// TODO: make this test more robust by not relying on `sleep`
	tokio::time::sleep(std::time::Duration::from_secs(2)).await;
	peer_sender.send(PeerUpdate::Registered(pi1.clone())).unwrap();

	// Normally ZMQ allows sending messages before the connection
	// is established, but this isn't the case if we handle reconnection
	// manually (only in cases of authentication failures, which should
	// be rare). We add a small delay before sending a message to ensure
	// the connection is established first.
	// TODO: consider adding our own buffers to store messages before we
	// received authentication success.
	tokio::time::sleep(std::time::Duration::from_secs(2)).await;
	node1
		.msg_sender
		.send(OutgoingMultisigStageMessages::Private(vec![(
			pi2.account_id.clone(),
			b"test".to_vec(),
		)]))
		.unwrap();

	let _ = expect_recv_with_timeout(&mut node2.msg_receiver).await;
}

async fn send_and_receive_message(from: &Node, to: &mut Node) -> Option<(AccountId, Vec<u8>)> {
	from.msg_sender
		.send(OutgoingMultisigStageMessages::Private(vec![(
			to.account_id.clone(),
			b"test".to_vec(),
		)]))
		.unwrap();

	recv_with_custom_timeout(&mut to.msg_receiver, MAX_CONNECTION_DELAY).await
}

#[tokio::test]
async fn can_connect_after_pubkey_change() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	// TODO: automatically select ports to avoid any potential conflicts
	// with other tests
	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 8089);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 8090);

	let mut node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi1.clone(), pi2.clone()]);
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// Since we no longer buffer messages until nodes connect, we
	// need to explicitly wait for them to connect (this might take a
	// while since one of them is likely to fail on the first try)
	tokio::time::sleep(std::time::Duration::from_millis(500)).await;

	// Check that node 2 can communicate with node 1:
	send_and_receive_message(&node2, &mut node1).await.unwrap();
	send_and_receive_message(&node1, &mut node2).await.unwrap();

	// Node 2 disconnects:
	drop(node2);

	// Node 2 connects with a different key:
	let node_key2b = create_keypair();
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2b, 8091);
	let mut node2b = spawn_node(&node_key2b, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// Node 1 learn about Node 2's new key:
	node1.peer_update_sender.send(PeerUpdate::Registered(pi2.clone())).unwrap();

	// Wait for Node 1 to connect (this shouldn't take long since
	// Node 2 is already up and we should succeed on first try)
	tokio::time::sleep(std::time::Duration::from_millis(100)).await;

	// Node 2 should be able to send messages again:
	send_and_receive_message(&node2b, &mut node1).await.unwrap();
	send_and_receive_message(&node1, &mut node2b).await.unwrap();
}

/// Test the behaviour around receiving own registration: at first, if our node
/// is not registered, we delay connecting to other nodes; once we receive our
/// own registration, we connect to other registered nodes.
#[tokio::test]
async fn connects_after_registration() {
	let node_key1 = create_keypair();
	let node_key2 = create_keypair();

	let pi1 = create_node_info(AccountId::new([1; 32]), &node_key1, 8092);
	let pi2 = create_node_info(AccountId::new([2; 32]), &node_key2, 8093);

	// Node 1 doesn't get its own peer info at first and will wait for registration
	let node1 = spawn_node(&node_key1, 0, pi1.clone(), &[pi2.clone()]);
	let mut node2 = spawn_node(&node_key2, 1, pi2.clone(), &[pi1.clone(), pi2.clone()]);

	// For sanity, check that node 1 can't yet communicate with node 2:
	assert!(send_and_receive_message(&node1, &mut node2).await.is_none());

	// Update node 1 with its own peer info
	node1.peer_update_sender.send(PeerUpdate::Registered(pi1.clone())).unwrap();

	// Allow some time for the above command to propagate through the channel
	tokio::time::sleep(std::time::Duration::from_millis(100)).await;

	// It should now be able to communicate with node 2:
	assert!(send_and_receive_message(&node1, &mut node2).await.is_some());
}
