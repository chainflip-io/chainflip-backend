use crate::testing::expect_recv_with_timeout;

use super::{PeerInfo, PeerUpdate};
use crate::p2p::OutgoingMultisigStageMessages;
use sp_core::ed25519::Public;
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info_span, Instrument};
use utilities::Port;

fn create_node_info(id: AccountId, node_key: &ed25519_dalek::Keypair, port: Port) -> PeerInfo {
	use std::net::Ipv4Addr;
	let ip = "0.0.0.0".parse::<Ipv4Addr>().unwrap().to_ipv6_mapped();
	let pubkey = Public(node_key.public.to_bytes());
	PeerInfo::new(id, pubkey, ip, port)
}

struct Node {
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
	let (msg_sender, peer_update_sender, msg_receiver, _, fut) =
		super::start(key, our_peer_info.port, peer_infos.to_vec(), account_id);

	tokio::spawn(fut.instrument(info_span!("node", idx = idx)));

	Node { msg_sender, peer_update_sender, msg_receiver }
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
