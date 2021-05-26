use anyhow::Result;
use substrate_subxt::{Client, ClientBuilder, EventSubscription};

use crate::{
    mq::{nats_client::NatsMQClient, IMQClient, Subject},
    settings::{self, Settings},
};

use log::{debug, error, info, trace};

use super::{
    helpers::create_subxt_client,
    runtime::StateChainRuntime,
    sc_event::{sc_event_from_raw_event, subject_from_raw_event},
};

/// Kick off the state chain observer process
pub async fn start(settings: Settings) {
    info!("Begin subscribing to state chain events");

    let mq_client = NatsMQClient::connect(settings.message_queue).await.unwrap();

    let subxt_client = create_subxt_client(settings.state_chain)
        .await
        .expect("Could not create subxt client");

    subscribe_to_events(*mq_client, subxt_client)
        .await
        .expect("Could not subscribe to state chain events");
}

async fn subscribe_to_events<M: 'static + IMQClient>(
    mq_client: M,
    subxt_client: Client<StateChainRuntime>,
) -> Result<()> {
    // subscribe to all finalised events, and then redirect them
    let sub = subxt_client
        .subscribe_finalized_events()
        .await
        .expect("Could not subscribe to state chain events");
    let decoder = subxt_client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);
    while let Some(res_event) = sub.next().await {
        let raw_event = match res_event {
            Ok(raw_event) => raw_event,
            Err(e) => {
                error!("Next event could not be read: {}", e);
                continue;
            }
        };

        let subject: Option<Subject> = subject_from_raw_event(&raw_event);

        if let Some(subject) = subject {
            let message = sc_event_from_raw_event(raw_event)?;
            match message {
                Some(event) => {
                    // Publish the message to the message queue
                    match mq_client.publish(subject, &event).await {
                        Err(_) => {
                            error!(
                                "Could not publish message `{:?}` to subject `{}`",
                                event,
                                subject.to_string()
                            );
                        }
                        Ok(_) => trace!("Event: {:#?} pushed to message queue", event),
                    };
                }
                None => {
                    debug!(
                        "Event decoding for an event under subject: {} doesn't exist",
                        subject
                    )
                }
            }
        } else {
            trace!("Not routing event {:?} to message queue", raw_event);
        };
    }
    Ok(())
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
