use itertools::Itertools;
use log::*;
use rand::{
    prelude::{IteratorRandom, StdRng},
    SeedableRng,
};
use slog::o;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    logging::{self, COMPONENT_KEY},
    p2p::{
        self,
        mock::{MockChannelEventHandler, NetworkMock},
        ValidatorId,
    },
    signing::db::KeyDBMock,
};

use lazy_static::lazy_static;

use crate::signing::{
    client::{
        self, KeyId, KeygenInfo, MultisigEvent, MultisigInstruction, SigningInfo, SigningOutcome,
    },
    MessageHash,
};

// Store channels used by a node to communicate internally (*not* to peers)
#[derive(Debug)]
pub struct FakeNode {
    multisig_instruction_tx: UnboundedSender<MultisigInstruction>,
    multisig_event_rx: UnboundedReceiver<MultisigEvent>,
}

/// Number of parties participating in keygen
const N_PARTIES: usize = 2;
lazy_static! {
    static ref SIGNERS: Vec<usize> = (1..=N_PARTIES).collect();
    static ref VALIDATOR_IDS: Vec<ValidatorId> = SIGNERS
        .iter()
        .map(|idx| ValidatorId([*idx as u8; 32]))
        .collect();
}

async fn coordinate_signing(
    mut nodes: Vec<FakeNode>,
    active_indices: &[usize],
    logger: &slog::Logger,
) -> Result<(), ()> {
    let logger = logger.new(o!(COMPONENT_KEY => "CoordinateSigning"));
    // get a keygen request ready with all of the VALIDATOR_IDS
    let key_id = KeyId(0);
    let keygen_request_info = KeygenInfo::new(key_id, VALIDATOR_IDS.clone());

    // publish the MultisigInstruction::KeyGen to all the clients
    for node in &nodes {
        node.multisig_instruction_tx
            .send(MultisigInstruction::KeyGen(keygen_request_info.clone()))
            .map_err(|_| "Receiver dropped")
            .unwrap();
    }

    slog::info!(logger, "Published key gen instruction to all the clients");

    // get a list of the signer_ids as a subset of VALIDATOR_IDS with an offset of 1
    let signer_ids = active_indices
        .iter()
        .map(|i| VALIDATOR_IDS[*i].clone())
        .collect_vec();

    // get a signing request ready with the list of signer_ids
    let sign_info = SigningInfo::new(key_id, signer_ids);

    // Only some clients should receive the instruction to sign
    for i in active_indices {
        let n = &nodes[*i];

        n.multisig_instruction_tx
            .send(MultisigInstruction::Sign(
                MessageHash(super::fixtures::MESSAGE.clone()),
                sign_info.clone(),
            ))
            .map_err(|_| "Receiver dropped")
            .unwrap();

        n.multisig_instruction_tx
            .send(MultisigInstruction::Sign(
                MessageHash(super::fixtures::MESSAGE2.clone()),
                sign_info.clone(),
            ))
            .map_err(|_| "Receiver dropped")
            .unwrap();
    }

    slog::info!(logger, "Published two signing messages to all clients");

    // collect all of the signed messages
    let mut signed_count = 0;
    loop {
        // go through each node and get the multisig events from the receiver
        for i in active_indices {
            let multisig_events = &mut nodes[*i].multisig_event_rx;

            match multisig_events.recv().await {
                Some(MultisigEvent::MessageSigningResult(SigningOutcome {
                    result: Ok(_), ..
                })) => {
                    slog::info!(logger, "Message is signed from {}", i);
                    signed_count = signed_count + 1;
                }
                Some(MultisigEvent::MessageSigningResult(_)) => {
                    slog::error!(logger, "Messaging signing result failed :(");
                    return Err(());
                }
                None => slog::error!(
                    logger,
                    "Unexpected error: client stream returned early: {}",
                    i
                ),
                _ => { /* Ignore all other messages, just wait for the MessageSigningResult or timeout*/
                }
            };
        }
        // stop the test when all of the MessageSigned have come in
        if signed_count >= active_indices.len() * 2 {
            break;
        }
        slog::info!(logger, "Not all messages signed, go around again");
    }
    slog::info!(logger, "All messages have been signed");
    return Ok(());
}

#[tokio::test]
async fn distributed_signing() {
    let logger = logging::test_utils::create_test_logger();
    // calculate how many parties will be in the signing (must be exact)
    // TODO: use the threshold_from_share_count function in keygen manager here.
    let threshold = (2 * N_PARTIES - 1) / 3;

    let mut rng = StdRng::seed_from_u64(0);

    // Parties (from 0..n that will participate in the signing process)
    let mut active_indices = (0..N_PARTIES)
        .into_iter()
        .choose_multiple(&mut rng, threshold + 1);
    active_indices.sort_unstable();

    slog::info!(
        logger,
        "There are {} active parties: {:?}",
        active_indices.len(),
        active_indices
    );

    assert!(active_indices.len() <= N_PARTIES);

    let network = NetworkMock::new();

    // Start the futures for each node
    let mut node_client_and_conductor_futs = vec![];
    let mut shutdown_txs = vec![];
    let mut fake_nodes = vec![];
    for i in 0..N_PARTIES {
        let p2p_client = network.new_client(VALIDATOR_IDS[i].clone());
        let logger = logger.clone();

        let db = KeyDBMock::new();

        let (multisig_instruction_tx, multisig_instruction_rx) =
            tokio::sync::mpsc::unbounded_channel();
        let (multisig_event_tx, multisig_event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (p2p_message_command_tx, p2p_message_command_rx) =
            tokio::sync::mpsc::unbounded_channel();

        let (mock_channel_event_handler, mock_channel_event_handler_receiver) =
            MockChannelEventHandler::new();
        let (shutdown_client_tx, shutdown_client_rx) = tokio::sync::oneshot::channel::<()>();
        let client_fut = client::start(
            VALIDATOR_IDS[i].clone(),
            db,
            multisig_instruction_rx,
            multisig_event_tx,
            mock_channel_event_handler_receiver,
            p2p_message_command_tx,
            shutdown_client_rx,
            &logger,
        );

        let (shutdown_conductor_tx, shutdown_conductor_rx) = tokio::sync::oneshot::channel::<()>();

        let conductor_fut = p2p::conductor::start_with_handler(
            mock_channel_event_handler,
            p2p_client,
            p2p_message_command_rx,
            shutdown_conductor_rx,
            &logger,
        );

        node_client_and_conductor_futs.push(futures::future::join(conductor_fut, client_fut));
        shutdown_txs.push(shutdown_conductor_tx);
        shutdown_txs.push(shutdown_client_tx);

        fake_nodes.push(FakeNode {
            multisig_instruction_tx,
            multisig_event_rx,
        })
    }

    let test_fut = async move {
        assert_eq!(
            coordinate_signing(fake_nodes, &active_indices, &logger).await,
            Ok(()),
            "One of the clients failed to sign the message"
        );

        info!("Graceful shutdown of distributed_signing test");

        for tx in shutdown_txs {
            tx.send(()).unwrap();
        }
    };

    futures::join!(
        futures::future::join_all(node_client_and_conductor_futs),
        test_fut
    );
}
