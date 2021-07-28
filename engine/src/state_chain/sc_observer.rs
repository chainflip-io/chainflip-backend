use anyhow::Result;
use substrate_subxt::{Client, EventSubscription};

use crate::{
    mq::{nats_client::NatsMQClientFactory, IMQClient, Subject, SubjectName},
    settings::Settings,
};

use super::{
    helpers::create_subxt_client,
    runtime::StateChainRuntime,
    sc_event::{raw_event_to_subject, sc_event_from_raw_event},
};

/// Kick off the state chain observer process
pub async fn start<M: IMQClient>(settings: &Settings, mq_client: M) {
    log::info!("Start subscribing to state chain events");

    let subxt_client = create_subxt_client(settings.state_chain)
        .await
        .expect("Could not create subxt client");

    subscribe_to_events(mq_client, subxt_client)
        .await
        .expect("Could not subscribe to state chain events");
}

async fn subscribe_to_events<M: IMQClient>(
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
                log::error!("Next event could not be read: {}", e);
                continue;
            }
        };

        let subject: Option<Subject> = raw_event_to_subject(&raw_event);

        if let Some(subject) = subject {
            let message = sc_event_from_raw_event(raw_event)?;
            match message {
                Some(event) => {
                    // Publish the message to the message queue
                    match mq_client.publish(subject, &event).await {
                        Err(err) => {
                            log::error!(
                                "Could not publish message `{:?}` to subject `{}`. Error: {}",
                                event,
                                subject.to_subject_name(),
                                err
                            );
                        }
                        Ok(_) => log::trace!("Event: {:?} pushed to message queue", event),
                    };
                }
                None => {
                    log::debug!(
                        "Event decoding for an event under subject: {} doesn't exist",
                        subject.to_subject_name()
                    )
                }
            }
        }
        // we can ignore events we don't care about like ExtrinsicSuccess
    }

    let err_msg = "State Chain Observer stopped subscribing to events!";
    log::error!("{}", err_msg);
    Err(anyhow::Error::msg(err_msg))
}
