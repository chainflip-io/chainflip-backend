use futures::StreamExt;
use itertools::Itertools;
use log::*;
use rand::{
    prelude::{IteratorRandom, StdRng},
    SeedableRng,
};
use tokio::time::Duration;

use crate::{
    mq::{
        mq_mock::{MQMock, MQMockClientFactory},
        pin_message_stream, IMQClient, Subject,
    },
    p2p::{mock::NetworkMock, P2PConductor, ValidatorId},
};

use lazy_static::lazy_static;

use crate::signing::{
    client::{
        KeyId, KeygenInfo, MultisigClient, MultisigEvent, MultisigInstruction, SigningInfo,
        SigningOutcome,
    },
    crypto::Parameters,
    MessageHash,
};

/// Number of parties participating in keygen
const N_PARTIES: usize = 5;
lazy_static! {
    static ref SIGNERS: Vec<usize> = (1..=N_PARTIES).collect();
    static ref VALIDATOR_IDS: Vec<ValidatorId> =
        SIGNERS.iter().map(|idx| ValidatorId::new(idx)).collect();
}

async fn coordinate_signing(
    mq_clients: Vec<impl IMQClient>,
    active_indices: &[usize],
) -> Result<(), ()> {
    // get all the streams from the clients and subscribe them to MultisigEvent
    let streams = mq_clients
        .iter()
        .map(|mc| {
            let mc = mc.clone();
            async move {
                let stream = mc
                    .subscribe::<MultisigEvent>(Subject::MultisigEvent)
                    .await
                    .expect("Could not subscribe");

                pin_message_stream(stream)
            }
        })
        .collect_vec();

    // join all the MultisigEvent streams together
    let mut streams = futures::future::join_all(streams).await;

    // wait until all of the clients have sent the MultisigEvent::ReadyToKeygen signal
    let mut key_ready_count = 0;
    loop {
        for s in &mut streams {
            match tokio::time::timeout(Duration::from_millis(200 * N_PARTIES as u64), s.next())
                .await
            {
                Ok(Some(Ok(MultisigEvent::ReadyToKeygen))) => {
                    key_ready_count = key_ready_count + 1;
                }
                Err(_) => {
                    info!("client timed out on ReadyToKeygen.");
                    return Err(());
                }
                _ => {}
            }
        }
        if key_ready_count >= streams.len() {
            info!("All clients ReadyToKeygen.");
            break;
        }
    }

    // get a keygen request ready with all of the VALIDATOR_IDS
    let key_id = KeyId(0);
    let auction_info = KeygenInfo::new(key_id, VALIDATOR_IDS.clone());

    // publish the MultisigInstruction::KeyGen to all the clients
    for mc in &mq_clients {
        trace!("published keygen instruction");
        mc.publish(
            Subject::MultisigInstruction,
            &MultisigInstruction::KeyGen(auction_info.clone()),
        )
        .await
        .expect("Could not publish");
    }

    let data = MessageHash(super::fixtures::MESSAGE.clone());
    let data2 = MessageHash(super::fixtures::MESSAGE2.clone());

    // get a list of the signer_ids as a subset of VALIDATOR_IDS with an offset of 1
    let signer_ids = active_indices
        .iter()
        .map(|i| VALIDATOR_IDS[*i - 1].clone())
        .collect_vec();

    // get a signing request ready with the list of signer_ids
    let sign_info = SigningInfo::new(key_id, signer_ids);

    // Only some clients should receive the instruction to sign
    for i in active_indices {
        let mc = &mq_clients[*i - 1];

        mc.publish(
            Subject::MultisigInstruction,
            &MultisigInstruction::Sign(data.clone(), sign_info.clone()),
        )
        .await
        .expect("Could not publish");

        mc.publish(
            Subject::MultisigInstruction,
            &MultisigInstruction::Sign(data2.clone(), sign_info.clone()),
        )
        .await
        .expect("Could not publish");
    }

    // collect all of the signed messages
    let mut signed_count = 0;
    loop {
        for i in active_indices {
            let stream = &mut streams[*i - 1];
            match tokio::time::timeout(Duration::from_millis(1000), stream.next()).await {
                Ok(Some(Ok(MultisigEvent::MessageSigningResult(
                    SigningOutcome::MessageSigned(_),
                )))) => {
                    info!("Message is signed from {}", i);
                    signed_count = signed_count + 1;
                }
                Ok(Some(Ok(MultisigEvent::MessageSigningResult(_)))) => {
                    return Err(());
                }
                Ok(None) => info!("Unexpected error: client stream returned early: {}", i),
                Err(_) => {
                    info!(
                        "client {} timed out. {}/{} messages signed",
                        i,
                        signed_count,
                        active_indices.len() * 2
                    );
                    return Err(());
                }
                _ => {}
            };
        }
        // stop the test when all of the MessageSigned have come in
        if signed_count >= active_indices.len() * 2 {
            break;
        }
    }
    info!("All messages have been signed");
    return Ok(());
}

#[tokio::test]
async fn distributed_signing() {
    env_logger::init();

    let t = 1 + ((N_PARTIES - 1) as f64 * 0.66) as usize;

    let mut rng = StdRng::seed_from_u64(0);

    // Parties (from 1..=n that will participate in the signing process)
    let mut active_indices = (1..=N_PARTIES).into_iter().choose_multiple(&mut rng, t + 1); // make sure that it works for t+k (k!=1)
    active_indices.sort_unstable();

    info!("Active parties: {:?}", active_indices);

    assert!(active_indices.len() > t);
    assert!(active_indices.len() <= N_PARTIES);

    // Create a fake network
    let network = NetworkMock::new();

    // Start message queues for each party
    let mc_futs = (1..=N_PARTIES)
        .map(|i| {
            let p2p_client = network.new_client(VALIDATOR_IDS[i - 1].clone());

            async move {
                let mq = MQMock::new();

                let mc = mq.get_client();

                let conductor = P2PConductor::new(mc, p2p_client).await;

                let (shutdown_conductor_tx, shutdown_conductor_rx) =
                    tokio::sync::oneshot::channel::<()>();

                let conductor_fut = conductor.start(shutdown_conductor_rx);

                let mq_factory = MQMockClientFactory::new(mq.clone());

                let client = MultisigClient::new(mq_factory, VALIDATOR_IDS[i - 1].clone());

                let (shutdown_client_tx, shutdown_client_rx) =
                    tokio::sync::oneshot::channel::<()>();

                // "ready to sign" emitted here
                let client_fut = client.run(shutdown_client_rx);

                let mc = mq.get_client();

                (
                    mc,
                    futures::future::join(conductor_fut, client_fut),
                    shutdown_client_tx,
                    shutdown_conductor_tx,
                )
            }
        })
        .collect_vec();

    let results = futures::future::join_all(mc_futs).await;

    let mut futs = vec![];
    let mut mc_clients = vec![];
    let mut shutdown_txs = vec![];

    for (mc, fut, shut_client_tx, shut_conduct_tx) in results {
        futs.push(fut);
        mc_clients.push(mc);
        shutdown_txs.push(shut_client_tx);
        shutdown_txs.push(shut_conduct_tx);
    }

    let test_fut = async move {
        // run the signing test and get the result
        let res = coordinate_signing(mc_clients, &active_indices).await;

        assert_eq!(res, Ok(()), "One of the clients failed to sign the message");

        info!("Graceful shutdown");
        // send a message to all the clients and the conductors to shut down
        for tx in shutdown_txs {
            tx.send(()).unwrap();
        }
    };

    futures::join!(futures::future::join_all(futs), test_fut);
}
