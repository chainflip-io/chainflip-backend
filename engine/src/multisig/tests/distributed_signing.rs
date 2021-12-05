use std::collections::HashMap;

use client::KeygenOutcome;
use itertools::Itertools;
use rand::prelude::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    logging,
    multisig::{
        client::{
            self, ensure_unsorted,
            keygen::{KeygenInfo, KeygenOptions},
            signing::SigningInfo,
            SigningOutcome,
        },
        KeyDBMock, KeyId, MessageHash, MultisigInstruction, MultisigOutcome,
    },
    p2p::{
        self,
        mock::{MockChannelEventHandler, NetworkMock},
        AccountId,
    },
};

use lazy_static::lazy_static;

// Store channels used by a node to communicate internally (*not* to peers)
#[derive(Debug)]
pub struct FakeNode {
    multisig_instruction_tx: UnboundedSender<MultisigInstruction>,
    multisig_event_rx: UnboundedReceiver<MultisigOutcome>,
}

/// Number of parties participating in keygen
const N_PARTIES: usize = 4;
lazy_static! {
    static ref ACCOUNT_IDS2: Vec<AccountId> = {
        let ids: Vec<_> = (1..=N_PARTIES)
            .map(|idx| AccountId([idx as u8; 32]))
            .collect();

        ensure_unsorted(ids, 0)
    };
}

async fn coordinate_keygen_and_signing(
    mut nodes: HashMap<AccountId, FakeNode>,
    logger: &slog::Logger,
) -> Result<(), ()> {
    // get a keygen request ready with all of the ACCOUNT_IDS

    // publish the MultisigInstruction::Keygen to all the clients
    for (i, (_id, node)) in nodes.iter().enumerate() {
        // Ensure that we don't rely on all parties receiving the list of
        // participants in the same order:
        let account_ids = ensure_unsorted(ACCOUNT_IDS2.clone(), i as u64);

        let keygen_request_info = KeygenInfo::new(0, account_ids);

        node.multisig_instruction_tx
            .send(MultisigInstruction::Keygen(keygen_request_info.clone()))
            .map_err(|_| "Receiver dropped")
            .unwrap();
    }

    slog::info!(logger, "Published key gen instruction to all the clients");

    // get a list of the signer_ids as a subset of ACCOUNT_IDS with an offset of 1

    // wait on the keygen ceremony so we can use the correct KeyId to sign with
    let key_id = {
        // Receive 1 event from each channel
        let results = futures::future::join_all(
            nodes
                .values_mut()
                .map(|n| n.multisig_event_rx.recv())
                .collect_vec(),
        )
        .await;

        if let Some(MultisigOutcome::Keygen(KeygenOutcome {
            id: _,
            result: Ok(pubkey),
        })) = results[0]
        {
            KeyId(pubkey.serialize().into())
        } else {
            panic!("Expecting a successful keygen result");
        }
    };

    // Only some clients should receive the instruction to sign, choose which ones:
    let signer_ids = {
        let mut rng = StdRng::seed_from_u64(0);

        // calculate how many parties will be in the signing (must be exact)
        let threshold = utilities::threshold_from_share_count(N_PARTIES as u32) as usize;

        let signer_ids = ACCOUNT_IDS2
            .iter()
            .cloned()
            .choose_multiple(&mut rng, threshold + 1);

        let signer_ids = ensure_unsorted(signer_ids, 0);

        slog::info!(logger, "Active parties: {:?}", signer_ids);

        assert!(signer_ids.len() <= N_PARTIES);

        signer_ids
    };

    for (i, id) in signer_ids.iter().enumerate() {
        let n = &nodes[id];

        n.multisig_instruction_tx
            .send(MultisigInstruction::Sign(SigningInfo::new(
                0, /* ceremony_id */
                key_id.clone(),
                MessageHash(super::fixtures::MESSAGE.clone()),
                ensure_unsorted(signer_ids.clone(), i as u64),
            )))
            .map_err(|_| "Receiver dropped")
            .unwrap();

        n.multisig_instruction_tx
            .send(MultisigInstruction::Sign(SigningInfo::new(
                1, /* ceremony_id */
                key_id.clone(),
                MessageHash(super::fixtures::MESSAGE2.clone()),
                ensure_unsorted(signer_ids.clone(), i as u64),
            )))
            .map_err(|_| "Receiver dropped")
            .unwrap();
    }

    slog::info!(logger, "Published two signing messages to all clients");

    // collect all of the signed messages
    let mut signed_count = 0;
    loop {
        // go through each node and get the multisig events from the receiver
        for id in &signer_ids {
            let multisig_events = &mut nodes.get_mut(id).unwrap().multisig_event_rx;

            match multisig_events.recv().await {
                Some(MultisigOutcome::Signing(SigningOutcome { result: Ok(_), .. })) => {
                    slog::info!(logger, "Message is signed from {}", id);
                    signed_count = signed_count + 1;
                }
                Some(MultisigOutcome::Signing(_)) => {
                    slog::error!(logger, "Messaging signing result failed :(");
                    return Err(());
                }
                None => slog::error!(
                    logger,
                    "Unexpected error: client stream returned early: {}",
                    id
                ),
                Some(res) => slog::error!(logger, "Unexpected result: {:?} from {}", res, id),
            };
        }
        // stop the test when all of the MessageSigned have come in
        if signed_count >= signer_ids.len() * 2 {
            break;
        }
        slog::info!(logger, "Not all messages signed, go around again");
    }
    slog::info!(logger, "All messages have been signed");
    return Ok(());
}

#[tokio::test]
async fn distributed_signing() {
    let logger = logging::test_utils::new_test_logger();

    let network = NetworkMock::new();

    // Start the futures for each node
    let mut node_client_and_conductor_futs = vec![];

    let mut shutdown_txs = vec![];
    let mut fake_nodes: HashMap<AccountId, FakeNode> = Default::default();
    for id in &*ACCOUNT_IDS2 {
        let p2p_client = network.new_client(id.clone());
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
        let client_fut = crate::multisig::start_client(
            id.clone(),
            db,
            multisig_instruction_rx,
            multisig_event_tx,
            mock_channel_event_handler_receiver,
            p2p_message_command_tx,
            shutdown_client_rx,
            KeygenOptions::allowing_high_pubkey(),
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

        fake_nodes.insert(
            id.clone(),
            FakeNode {
                multisig_instruction_tx,
                multisig_event_rx,
            },
        );
    }

    let test_fut = async move {
        assert_eq!(
            coordinate_keygen_and_signing(fake_nodes, &logger).await,
            Ok(()),
            "One of the clients failed to sign the message"
        );

        for tx in shutdown_txs {
            tx.send(()).unwrap();
        }
    };

    futures::join!(
        futures::future::join_all(node_client_and_conductor_futs),
        test_fut
    );
}
