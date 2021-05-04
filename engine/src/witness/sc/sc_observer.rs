use std::sync::Arc;

use anyhow::Result;
use chainflip_common::types::coin::Coin;
use substrate_subxt::{Client, ClientBuilder, EventSubscription, RawEvent};

use tokio::sync::Mutex;

use crate::mq::{IMQClient, Subject};

use super::runtime::StateChainRuntime;

/// Kick off the state chain observer process
pub async fn start<M: 'static + IMQClient + Send + Sync>(mq_client: Arc<Mutex<M>>) {
    println!("Begin subsribing to state chain events");
    subscribe_to_events(mq_client)
        .await
        .expect("Could not subscribe to state chain events");
}

/// Create a substrate subxt client over the StateChainRuntime
async fn create_subxt_client() -> Result<Client<StateChainRuntime>> {
    let client = ClientBuilder::<StateChainRuntime>::new()
        // ideally don't use this, but we currently have a few types that aren't even used (transactions pallet), so this is to save
        // defining types for them.
        .skip_type_sizes_check()
        .build()
        .await?;

    Ok(client)
}

async fn subscribe_to_events<M: 'static + IMQClient + Send + Sync>(
    mq_client: Arc<Mutex<M>>,
) -> Result<()> {
    let client = create_subxt_client()
        .await
        .expect("Could not create subxt client");

    // subscribe to all finalised events, and then redirect them
    let sub = client
        .subscribe_finalized_events()
        .await
        .expect("Could not subscribe to state chain events");
    let decoder = client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);
    loop {
        let raw_event = if let Some(res_event) = sub.next().await {
            res_event?
        } else {
            println!("No event found on the state chain");
            continue;
        };

        let mq_c = mq_client.clone();
        tokio::spawn(async move {
            let subject: Option<Subject> = subject_from_raw_event(&raw_event);

            if let Some(subject) = subject {
                match mq_c.lock().await.publish(subject, &raw_event.data).await {
                    Err(_) => {
                        println!(
                            "Could not publish message `{:?}` to subject `{}`",
                            raw_event.data,
                            subject.to_string()
                        );
                    }
                    _ => (),
                };
            } else {
                println!("Not routing event {:?} to message queue", raw_event);
            };
        });
    }
}

/// Returns the subject to publish the data of a raw event to
fn subject_from_raw_event(event: &RawEvent) -> Option<Subject> {
    let subject = match event.module.as_str() {
        "System" => None,
        "Staking" => match event.variant.as_str() {
            "ClaimSigRequested" => Some(Subject::Claim),
            // All Stake refunds are ETH, how are these refunds coming out though? as batches or individual txs?
            "StakeRefund" => Some(Subject::Batch(Coin::ETH)),
            "ClaimSignatureIssued" => Some(Subject::Claim),
            // This doesn't need to go anywhere, this is just a confirmation emitted, perhaps for block explorers
            "Claimed" => None,
            _ => None,
        },
        "Validator" => match event.variant.as_str() {
            "AuctionEnded" => None,
            "AuctionStarted" => None,
            "ForceRotationRequested" => Some(Subject::Rotate),
            "EpochDurationChanged" => None,
            _ => None,
        },
        _ => None,
    };
    subject
}

#[cfg(test)]
mod tests {

    use nats_test_server::NatsTestServer;

    use crate::mq::mq_mock::MockMQ;

    use super::*;

    #[tokio::test]
    async fn run_test() {
        let server = NatsTestServer::build().spawn();
        let test_mq_client = MockMQ::new(&server).await;
        let test_mq_client = Arc::new(Mutex::new(test_mq_client));

        start(test_mq_client).await;
    }

    #[test]
    fn subject_from_raw_event_test() {
        // test success case
        let raw_event = substrate_subxt::RawEvent {
            // Module and variant are defined by the state chain node
            module: "Staking".to_string(),
            variant: "ClaimSigRequested".to_string(),
            data: "Test data".as_bytes().to_owned(),
        };

        let subject = subject_from_raw_event(&raw_event);
        assert_eq!(subject, Some(Subject::Claim));

        // test "fail" case
        let raw_event_invalid = substrate_subxt::RawEvent {
            // Module and variant are defined by the state chain node
            module: "NotAModule".to_string(),
            variant: "NotAVariant".to_string(),
            data: "Test data".as_bytes().to_owned(),
        };
        let subject = subject_from_raw_event(&raw_event_invalid);
        assert_eq!(subject, None);
    }
}
