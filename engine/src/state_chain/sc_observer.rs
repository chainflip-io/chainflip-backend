use anyhow::Result;
use slog::o;
use substrate_subxt::{Client, EventSubscription};

use crate::{
    logging::COMPONENT_KEY,
    mq::{IMQClient, Subject, SubjectName},
    settings,
};

use super::{
    helpers::create_subxt_client,
    runtime::StateChainRuntime,
    sc_event::{raw_event_to_subject, sc_event_from_raw_event},
};

pub struct SCObserver<M: IMQClient> {
    mq_client: M,
    subxt_client: Client<StateChainRuntime>,
    logger: slog::Logger,
}

impl<M: IMQClient> SCObserver<M> {
    pub async fn new(
        mq_client: M,
        state_chain_settings: &settings::StateChain,
        logger: &slog::Logger,
    ) -> Self {
        let subxt_client = create_subxt_client(state_chain_settings)
            .await
            .expect("Could not create subxt client");

        Self {
            mq_client,
            subxt_client,
            logger: logger.new(o!(COMPONENT_KEY => "SCObserver")),
        }
    }

    pub async fn run(&self) -> Result<()> {
        // subscribe to all finalised events, and then redirect them
        let sub = self
            .subxt_client
            .subscribe_finalized_events()
            .await
            .expect("Could not subscribe to state chain events");
        let decoder = self.subxt_client.events_decoder();
        let mut sub = EventSubscription::new(sub, decoder);
        while let Some(res_event) = sub.next().await {
            let raw_event = match res_event {
                Ok(raw_event) => raw_event,
                Err(e) => {
                    slog::error!(self.logger, "Next event could not be read: {}", e);
                    continue;
                }
            };

            let subject: Option<Subject> = raw_event_to_subject(&raw_event);

            if let Some(subject) = subject {
                let message = sc_event_from_raw_event(raw_event)?;
                match message {
                    Some(event) => {
                        // Publish the message to the message queue
                        match self.mq_client.publish(subject, &event).await {
                            Err(err) => {
                                slog::error!(
                                    self.logger,
                                    "Could not publish message `{:?}` to subject `{}`. Error: {}",
                                    event,
                                    subject.to_subject_name(),
                                    err
                                );
                            }
                            Ok(_) => {
                                slog::trace!(
                                    self.logger,
                                    "Event: {:?} pushed to message queue",
                                    event
                                )
                            }
                        };
                    }
                    None => {
                        slog::debug!(
                            self.logger,
                            "Event decoding for an event under subject: {} doesn't exist",
                            subject.to_subject_name()
                        )
                    }
                }
            }
            // we can ignore events we don't care about like ExtrinsicSuccess
        }

        let err_msg = "State Chain Observer stopped subscribing to events!";
        slog::error!(self.logger, "{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
    }
}
