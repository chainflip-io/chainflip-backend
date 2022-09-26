use crate::{p2p::KeyPair, testing::expect_recv_with_timeout};

use super::{P2PContext, PeerInfo, PeerUpdate};
use crate::multisig_p2p::OutgoingMultisigStageMessages;
use sp_core::ed25519::Public;
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

fn create_node_info(id: AccountId, pubkey: Public, port: u16) -> PeerInfo {
    use std::net::Ipv4Addr;
    let ip = "0.0.0.0".parse::<Ipv4Addr>().unwrap().to_ipv6_mapped();
    PeerInfo::new(id, pubkey, ip, port)
}

struct Node {
    msg_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    peer_update_sender: UnboundedSender<PeerUpdate>,
    msg_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
}

fn spawn_node(
    key: KeyPair,
    idx: usize,
    our_peer_info: PeerInfo,
    peer_infos: &[PeerInfo],
    logger: &slog::Logger,
) -> Node {
    let account_id = AccountId::new([idx as u8 + 1; 32]);
    let (msg_sender, peer_update_sender, msg_receiver, fut) = P2PContext::start(
        key,
        our_peer_info.ip.to_ipv4().unwrap(),
        our_peer_info.port,
        peer_infos.to_vec(),
        account_id,
        &logger.new(slog::o!("node" => idx)),
    );

    tokio::spawn(fut);

    Node {
        msg_sender,
        peer_update_sender,
        msg_receiver,
    }
}

// Create an x25519 keypair along with the corresponding ed25519 public key
fn create_keypair() -> (KeyPair, Public) {
    use rand::RngCore;
    let mut secret_key_bytes = [0; 32];
    rand::thread_rng().fill_bytes(&mut secret_key_bytes);

    let ed_secret_key =
        ed25519_dalek::SecretKey::from_bytes(&secret_key_bytes).expect("invalid key size");
    let ed_public_key: ed25519_dalek::PublicKey = (&ed_secret_key).into();

    let secret_key = super::ed25519_secret_key_to_x25519_secret_key(&ed_secret_key);
    let public_key: x25519_dalek::PublicKey = (&secret_key).into();

    (
        KeyPair {
            public_key,
            secret_key,
        },
        Public(*ed_public_key.as_bytes()),
    )
}

/// Ensure that a node can (eventually) receive messages from a peer
/// even if the latter initially fails the authentication check
// TODO: consider breaking this into more granular tests
#[tokio::test]
async fn connect_two_nodes() {
    use crate::logging::test_utils::new_test_logger;
    let logger = new_test_logger();

    let (keypair1, ed_pk1) = create_keypair();
    let (keypair2, ed_pk2) = create_keypair();

    // TODO: automatically select ports to avoid any potential conflicts
    // with other tests
    let pi1 = create_node_info(AccountId::new([1; 32]), ed_pk1, 8087);
    let pi2 = create_node_info(AccountId::new([2; 32]), ed_pk2, 8088);

    // Node 1 knows about node 2 from the startup
    let node1 = spawn_node(
        keypair1,
        0,
        pi1.clone(),
        &[pi1.clone(), pi2.clone()],
        &logger,
    );
    // ----------------------------------------------------------------
    // At this point node 1 may already attempt to connect to node 2,
    // but fail due to node 2 possibly being offline. The reconnection
    // in this case will be automatically handled by ZMQ and we are not
    // testing it explicitly here.
    // ----------------------------------------------------------------

    // Node 2 only knows about itself from the startup
    let mut node2 = spawn_node(keypair2, 1, pi2.clone(), &[pi2.clone()], &logger);

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
    peer_sender
        .send(PeerUpdate::Registered(pi1.clone()))
        .unwrap();

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
