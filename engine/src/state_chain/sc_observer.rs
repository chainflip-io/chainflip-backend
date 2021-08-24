use anyhow::Result;
use slog::o;
use substrate_subxt::{Client, EventSubscription};

use crate::{
    logging::COMPONENT_KEY,
    mq::{IMQClient, SubjectName},
};

use super::{runtime::StateChainRuntime, sc_event::raw_event_to_subject_and_sc_event};

pub async fn start<M: IMQClient>(
    mq_client: M,
    subxt_client: Client<StateChainRuntime>,
    logger: &slog::Logger,
) {
    SCObserver::new(mq_client, subxt_client, logger)
        .await
        .run()
        .await
        .expect("SC Observer has died!");
}

pub struct SCObserver<M: IMQClient> {
    mq_client: M,
    subxt_client: Client<StateChainRuntime>,
    logger: slog::Logger,
}

impl<M: IMQClient> SCObserver<M> {
    pub async fn new(
        mq_client: M,
        subxt_client: Client<StateChainRuntime>,
        logger: &slog::Logger,
    ) -> Self {
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

            let subject_and_sc_event = raw_event_to_subject_and_sc_event(&raw_event)?;

            if let None = subject_and_sc_event {
                slog::trace!(self.logger, "Discarding {:?}", raw_event);
                continue;
            }

            let (subject, sc_event) =
                subject_and_sc_event.expect("Must be Some due to condition above");

            match self.mq_client.publish(subject, &sc_event).await {
                Err(err) => {
                    slog::error!(
                        self.logger,
                        "Could not publish message `{:?}` to subject `{}`. Error: {}",
                        sc_event,
                        subject.to_subject_name(),
                        err
                    );
                }
                Ok(_) => {
                    slog::trace!(self.logger, "Event: {:?} pushed to message queue", sc_event)
                }
            }
        }

        let err_msg = "State Chain Observer stopped subscribing to events!";
        slog::error!(self.logger, "{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
    }
}

#[cfg(test)]
mod tests {
    use substrate_subxt::ClientBuilder;

    use crate::{logging, mq::nats_client::NatsMQClient, settings};

    use super::*;

    #[tokio::test]
    #[ignore = "runs forever, useful for testing without having to start the whole CFE"]
    async fn run_the_sc_observer() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        start(
            NatsMQClient::new(&settings.message_queue).await.unwrap(),
            ClientBuilder::<StateChainRuntime>::new()
                .set_url(&settings.state_chain.ws_endpoint)
                .build()
                .await
                .expect("Should create subxt client"),
            &logging::test_utils::create_test_logger(),
        )
        .await;
    }
}
