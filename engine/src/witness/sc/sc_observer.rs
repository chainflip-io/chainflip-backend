use std::sync::Arc;

use anyhow::Result;
use chainflip_common::types::coin::Coin;
use sp_keyring::AccountKeyring;
use state_chain_runtime::{AccountId, System};
use substrate_subxt::{Client, ClientBuilder, EventSubscription, RawEvent};

use tokio::sync::Mutex;

use crate::mq::{IMQClient, Subject};

use super::runtime::StateChainRuntime;

/// Kick off the state chain observer process
pub async fn start<M: 'static + IMQClient + Send + Sync>(mq_client: Arc<Mutex<M>>) {
    subscribe_to_events(mq_client).await;
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

async fn subscribe_to_events<M: 'static + IMQClient + Send + Sync>(mq_client: Arc<Mutex<M>>) {
    let mq_c = mq_client.clone();
    tokio::spawn(async move {
        let client = create_subxt_client().await.unwrap();

        let sub = client.subscribe_finalized_events().await.unwrap();
        let decoder = client.events_decoder();
        let mut sub = EventSubscription::new(sub, decoder);

        println!("Awaiting event");
        let raw_event = sub.next().await.unwrap().unwrap();
        println!("Raw event:\n{:#?}", raw_event);

        let subject: Option<Subject> = subject_from_raw_event(&raw_event);

        if let Some(subject) = subject {
            mq_c.lock()
                .await
                .publish(subject, &raw_event.data)
                .await
                .unwrap();
        } else {
            println!(
                "Unable to resolve event: {:#?} to a known event type",
                raw_event
            )
        }
    });
}

/// Returns the subject to publish the data of a raw event to
fn subject_from_raw_event(event: &RawEvent) -> Option<Subject> {
    let subject = match event.module.as_str() {
        "System" => None,
        "Staking" => match event.variant.as_str() {
            "ClaimSigRequested" => Some(Subject::Claim),
            // All Stake refunds are ETH, how are these refunds coming out though?
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
            "Epoch"
            _ => None,
        }
        _ => None,
    };
    subject
}

#[cfg(test)]
mod tests {

    use std::{marker::PhantomData, str::FromStr};

    use chainflip_common::types::addresses::EthereumAddress;
    use codec::Decode;
    use hex::encode;
    use nats_test_server::NatsTestServer;
    use substrate_subxt::system::ExtrinsicSuccessEvent;

    use crate::{mq::mq_mock::MockMQ, witness::sc::validator::MaximumValidatorsChangedEvent};

    use frame_support::weights::{DispatchClass, DispatchInfo, Pays};

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
