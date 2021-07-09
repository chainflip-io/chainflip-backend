use futures::StreamExt;
use itertools::Itertools;
use log::*;
use rand::{
    prelude::{IteratorRandom, StdRng},
    SeedableRng,
};

use crate::{
    mq::{
        mq_mock::{MQMock, MQMockClientFactory},
        pin_message_stream, IMQClient, Subject,
    },
    p2p::{mock::NetworkMock, P2PConductor, ValidatorId},
};

use lazy_static::lazy_static;

use crate::signing::{
    client::{KeyId, KeygenInfo, MultisigClient, MultisigEvent, MultisigInstruction, SigningInfo},
    crypto::Parameters,
    MessageHash,
};

/// Number of parties participating in keygen
const N_PARTIES: usize = 2;
lazy_static! {
    static ref SIGNERS: Vec<usize> = (1..=N_PARTIES).collect();
    static ref VALIDATOR_IDS: Vec<ValidatorId> =
        SIGNERS.iter().map(|idx| ValidatorId::new(idx)).collect();
}

async fn coordinate_signing(mq_clients: Vec<impl IMQClient>, active_indices: &[usize]) {
    // subscribe to "ready to sign"
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

    let key_id = KeyId(0);

    let auction_info = KeygenInfo::new(key_id, VALIDATOR_IDS.clone());

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

    let signer_ids = active_indices
        .iter()
        .map(|i| VALIDATOR_IDS[*i - 1].clone())
        .collect_vec();

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

    // TODO: add a timeout here
    for i in active_indices {
        let stream = &mut streams[i - 1];

        while let Some(evt) = stream.next().await {
            if let Ok(MultisigEvent::MessageSigned(_, _)) = evt {
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

    let t = 1;

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

                let conductor_fut = conductor.start();

                let mq_factory = MQMockClientFactory::new(mq.clone());

                let client = MultisigClient::new(mq_factory, VALIDATOR_IDS[i - 1].clone());

                // "ready to sign" emitted here
                let client_fut = client.run();

                let mc = mq.get_client();

                (mc, futures::future::join(conductor_fut, client_fut))
            }
        })
        .collect_vec();

    let results = futures::future::join_all(mc_futs).await;

    let mut futs = vec![];
    let mut mc_clients = vec![];

    for (mc, fut) in results {
        futs.push(fut);
        mc_clients.push(mc);
    }

    futures::join!(
        futures::future::join_all(futs),
        coordinate_signing(mc_clients, &active_indices)
    );
}
