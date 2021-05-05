use std::sync::Arc;

use anyhow::Result;
use chainflip_common::types::coin::Coin;
use codec::Decode;
use substrate_subxt::{Client, ClientBuilder, EventSubscription, RawEvent};

use tokio::sync::Mutex;

use crate::{
    mq::{IMQClient, Subject},
    settings,
};

use log::{error, info, trace};

use super::{
    runtime::StateChainRuntime,
    staking::{
        ClaimSigRequestedEvent, ClaimSignatureIssuedEvent, ClaimedEvent, StakeRefundEvent,
        StakedEvent,
    },
    SCEvent,
};

/// Kick off the state chain observer process
pub async fn start<M: 'static + IMQClient + Send + Sync>(
    mq_client: Arc<Mutex<M>>,
    subxt_settings: settings::StateChain,
) {
    info!("Begin subscribing to state chain events");

    let subxt_client = create_subxt_client(subxt_settings)
        .await
        .expect("Could not create subxt client");

    subscribe_to_events(mq_client, subxt_client)
        .await
        .expect("Could not subscribe to state chain events");
}

/// Create a substrate subxt client over the StateChainRuntime
async fn create_subxt_client(
    subxt_settings: settings::StateChain,
) -> Result<Client<StateChainRuntime>> {
    let client = ClientBuilder::<StateChainRuntime>::new()
        .set_url(format!(
            "ws://{}:{}",
            subxt_settings.hostname, subxt_settings.port
        ))
        .build()
        .await?;

    Ok(client)
}



async fn subscribe_to_events<M: 'static + IMQClient + Send + Sync>(
    mq_client: Arc<Mutex<M>>,
    subxt_client: Client<StateChainRuntime>,
) -> Result<()> {
    // subscribe to all finalised events, and then redirect them
    let sub = subxt_client
        .subscribe_finalized_events()
        .await
        .expect("Could not subscribe to state chain events");
    let decoder = subxt_client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);
    loop {
        let raw_event = if let Some(res_event) = sub.next().await {
            match res_event {
                Ok(evt) => evt,
                Err(e) => {
                    error!("Next event could not be read: {}", e);
                    continue;
                }
            }
        } else {
            info!("No further events from the state chain.");
            return Ok(());
        };

        let mq_c = mq_client.clone();
        let subject: Option<Subject> = subject_from_raw_event(&raw_event);

        if let Some(subject) = subject {
            match mq_c.lock().await.publish(subject, &raw_event.data).await {
                Err(_) => {
                    error!(
                        "Could not publish message `{:?}` to subject `{}`",
                        raw_event.data,
                        subject.to_string()
                    );
                }
                Ok(_) => trace!("Event: {:#?} pushed to message queue", raw_event.data),
            };
        } else {
            trace!("Not routing event {:?} to message queue", raw_event);
        };
    }
}

#[cfg(test)]
mod tests {

    use crate::settings::StateChain;

    use super::*;

    #[tokio::test]
    #[ignore = "depends on running state chain at the specifed url"]
    async fn create_subxt_client_test() {
        let subxt_settings = StateChain {
            hostname: "localhost".to_string(),
            port: 9944,
        };
        assert!(create_subxt_client(subxt_settings).await.is_ok())
    }
}
