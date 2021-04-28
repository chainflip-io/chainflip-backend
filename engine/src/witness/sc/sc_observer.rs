use std::sync::Arc;

use anyhow::Result;
use sp_keyring::AccountKeyring;
use state_chain_runtime::{AccountId, System};
use substrate_subxt::{
    balances::{TransferCallExt, TransferEvent},
    extrinsic::DefaultExtra,
    register_default_type_sizes,
    sp_core::Decode,
    Client, ClientBuilder, EventSubscription, RawEvent,
};

use tokio::sync::Mutex;

use crate::{
    mq::{IMQClient, Subject},
    witness::sc::transactions::DataAddedEvent,
};

use super::{runtime::StateChainRuntime, staking::ClaimSigRequested};

/// TODO: Make this sync
/// Kick of the state chain observer process
pub async fn start<M: 'static + IMQClient + Send + Sync>(mq_client: Arc<Mutex<M>>) {
    println!("Start the state chain witness with subxt");

    subscribe_to_events(mq_client).await;
}

// can't borrow decoder and then return the EventSub object :(
// async fn create_subscription<'a, E: substrate_subxt::Event<StateChainRuntime>>(
//     client: Client<StateChainRuntime>,
// ) -> Result<EventSubscription<'a, StateChainRuntime>> {
//     let sub = client.subscribe_finalized_events().await?;
//     let decoder = client.events_decoder();
//     let mut sub = EventSubscription::new(sub, decoder);
//     sub.filter_event::<E>();
//     Ok(sub)
// }

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
    let client = create_subxt_client().await.unwrap();

    // TODO: subscribe_events -> finalised events

    // ===== DataAddedEvents - for easy testing ====
    let client = client.clone();
    let sub = client.subscribe_events().await.unwrap();
    let decoder = client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);
    // data_added_sub.filter_event::<DataAddedEvent<_>>();

    // SigClaimRequested
    // let client_2 = client.clone();
    // let sig_claim_requested_events = client_2.subscribe_finalized_events().await.unwrap();
    // let decoder_more = client_2.events_decoder();
    // let mut sig_claim_requested_events =
    //     EventSubscription::new(sig_claim_requested_events, decoder_more);
    // sig_claim_requested_events.filter_event::<ClaimSigRequested<_>>();

    // TOOD: Spawn a thread. For each? or for all subscriptions? atm I think the latter, not much to gain for extra threads here

    loop {
        let raw_event = sub.next().await.unwrap().unwrap();
        // let raw_sig_claim_requested = sig_claim_requested_events.next().await.unwrap().unwrap();
        let mq_c = mq_client.clone();

        tokio::spawn(async move {
            println!("Raw event:\n{:#?}", raw_event);

            let subject: Option<Subject> = subject_from_raw_event(&raw_event);

            // Have some example consumer somewhere, of how to do this, but I think the raw bytes should be sent straight to the message queue
            // why serialize / deserialize when we can just decode?
            // let event =
            //     DataAddedEvent::<StateChainRuntime>::decode(&mut &raw_data_added.data[..]).unwrap();

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

            // Sig claim request
            // let raw = sig_claim_requested_events.next().await.unwrap().unwrap();
            // println!("the raw event is: {:#?}", raw);
            // let event = ClaimSigRequested::<StateChainRuntime>::decode(&mut &raw.data[..]).unwrap();
            // mq_c.lock()
            //     .await
            //     .publish(Subject::Claim, &event)
            //     .await
            //     .unwrap();
            // println!("The sender is {:#?}", event.who);

            //     println!("Adding event: {:#?} to the message queue", "Event");
        });
    }
}

fn subject_from_raw_event(event: &RawEvent) -> Option<Subject> {
    let subject = match event.module.as_str() {
        "System" => None,
        "Transactions" => match event.variant.as_str() {
            "DataAdded" => Some(Subject::Claim),
            _ => None,
        },
        "Staking" => match event.variant.as_str() {
            "ClaimSigRequested" => Some(Subject::Claim),
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
        // let event = substrate_subxt::RawEvent {
        //     module: "Transactions".to_string(),
        //     variant: "DataAdded".to_string(),
        //     data: "Hello".as_bytes().to_owned(),
        // };
        let server = NatsTestServer::build().spawn();
        let test_mq_client = MockMQ::new(&server).await;
        let test_mq_client = Arc::new(Mutex::new(test_mq_client));

        start(test_mq_client).await;
    }

    // TODO: Test decodinng of each of the custom events using some raw data
}

// RawEvent {
//     module: "Transactions",
//     variant: "DataAdded",
//     data: "8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480c617364",
// }

// RawEvent {
//     module: "System",
//     variant: "ExtrinsicSuccess",
//     data: "482d7c09000000000200",
// }
// Here's the event to be added: ExtrinsicSuccessEvent {
//     _runtime: PhantomData,
//     info: DispatchInfo {
//         weight: 159133000,
//         class: DispatchClass::Mandatory,
//         pays_fee: Pays::Yes,
//     },
// }
