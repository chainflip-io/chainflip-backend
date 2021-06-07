use std::rc::Rc;

use futures::StreamExt;
use itertools::Itertools;
use log::*;
use rand::{
    prelude::{IteratorRandom, StdRng},
    SeedableRng,
};

// use parking_lot::Mutex;
use crate::{
    mq::{mq_mock2::MQMock2, pin_message_stream, IMQClient, Subject},
    p2p::{mock::NetworkMock, P2PConductor},
    signing::client::MultisigInstruction,
};

use super::{
    bitcoin_schnorr::Parameters,
    client::{MultisigClient, MultisigEvent},
};

async fn coordinate_signing(mc_clients: Vec<impl IMQClient>, active_indices: &[usize]) {
    // subscribe to "ready to sign"
    let streams = mc_clients
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

    let mut streams = futures::future::join_all(streams).await;

    let ready_to_keygen = async {
        for s in &mut streams {
            while let Some(evt) = s.next().await {
                if let Ok(MultisigEvent::ReadyToKeygen) = evt {
                    break;
                }
            }
        }
    };

    ready_to_keygen.await;

    for mc in &mc_clients {
        trace!("published keygen instruction");
        mc.publish(Subject::MultisigInstruction, &MultisigInstruction::KeyGen)
            .await
            .expect("Could not publish");
    }

    // TODO: investigate why this is necessary (remove if it is not)
    let ready_to_sign = async {
        for s in &mut streams {
            while let Some(evt) = s.next().await {
                if let Ok(MultisigEvent::ReadyToSign) = evt {
                    break;
                }
            }
        }
    };

    ready_to_sign.await;

    let data = Vec::from("Chainflip".as_bytes());
    let data2 = Vec::from("Chainflip2".as_bytes());

    // Only some clients should receive the instruction to sign
    for i in active_indices {
        let mc = &mc_clients[*i - 1];

        mc.publish(
            Subject::MultisigInstruction,
            &MultisigInstruction::Sign(data.clone(), active_indices.to_vec()),
        )
        .await
        .expect("Could not publish");

        // mc.publish(
        //     Subject::MultisigInstruction,
        //     &MultisigInstruction::Sign(data2.clone()),
        // )
        // .await
        // .expect("Could not publish");
    }

    // TODO: add a timeout here
    for i in active_indices {
        let stream = &mut streams[i - 1];

        while let Some(evt) = stream.next().await {
            if let Ok(MultisigEvent::MessageSigned(_)) = evt {
                info!("Message is signed!");
                break;
            }
        }
    }

    // TODO: terminate all clients
}

#[tokio::test]
#[ignore = "currently runs infinitely"]
async fn distributed_signing() {
    env_logger::init();

    let t = 3;
    let n = 6;

    // let t = 1;
    // let n = 3;

    let mut rng = StdRng::seed_from_u64(0);

    // Parties (from 1..=n that will participate in the signing process)
    let mut active_indices = (1..=n).into_iter().choose_multiple(&mut rng, t + 1); // make sure that it works for t+k (k!=1)
    active_indices.sort_unstable();

    info!("Active parties: {:?}", active_indices);

    assert!(active_indices.len() > t);
    assert!(active_indices.len() <= n);

    // Create a fake network
    let network = NetworkMock::new();

    // Start message queues for each party

    let mc_futs = (1..=n)
        .map(|i| {
            let p2p_client = network.new_client(i);

            async move {
                let message_queue = MQMock2::new();

                let mc = message_queue.get_client();

                let conductor = P2PConductor::new(mc, i, p2p_client).await;

                let conductor_fut = conductor.start();

                let params = Parameters {
                    threshold: t,
                    share_count: n,
                };

                let mc = message_queue.get_client();
                let mc2 = message_queue.get_client();

                // TODO: Pass something that implements MQClientCreator instead

                let client = MultisigClient::new(mc, mc2, i, params);

                // "ready to sign" emitted here
                let client_fut = client.run();

                let mc = message_queue.get_client();

                (
                    mc,
                    message_queue,
                    futures::future::join(conductor_fut, client_fut),
                )
            }
        })
        .collect_vec();

    let results = futures::future::join_all(mc_futs).await;

    let mut futs = vec![];
    let mut mc_clients = vec![];

    // Keep servers on the stack, so they keep running
    let mut servers = vec![];

    for (mc, server, fut) in results {
        futs.push(fut);
        mc_clients.push(mc);
        servers.push(server);
    }

    futures::join!(
        futures::future::join_all(futs),
        coordinate_signing(mc_clients, &active_indices)
    );
}
