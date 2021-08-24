use anyhow::Result;
use slog::o;
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use substrate_subxt::{Client, EventSubscription};

use crate::{
    logging::COMPONENT_KEY,
    mq::{IMQClient, Subject},
    p2p,
    signing::{KeyId, KeygenInfo, MessageHash, MultisigInstruction},
    state_chain::{
        pallets::vaults::VaultsEvent::{EthSigningTxRequestEvent, KeygenRequestEvent},
        sc_event::SCEvent::{AuctionEvent, StakingEvent, ValidatorEvent, VaultsEvent},
    },
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

            let (_subject, sc_event) =
                subject_and_sc_event.expect("Must be Some due to condition above");

            match sc_event {
                AuctionEvent(_) => todo!(),
                ValidatorEvent(_) => todo!(),
                StakingEvent(_) => todo!(),
                VaultsEvent(event) => match event {
                    KeygenRequestEvent(keygen_request_event) => {
                        let validators: Vec<_> = keygen_request_event
                            .keygen_request
                            .validator_candidates
                            .iter()
                            .map(|v| p2p::ValidatorId(v.clone().into()))
                            .collect();
                        // TODO: Should this be request index? @andy
                        let key_gen_info =
                            KeygenInfo::new(KeyId(keygen_request_event.request_index), validators);
                        let gen_new_key_event = MultisigInstruction::KeyGen(key_gen_info);
                        self.mq_client
                            .publish(Subject::MultisigInstruction, &gen_new_key_event)
                            .await
                            .expect("Should publish to MQ");
                    }
                    EthSigningTxRequestEvent(eth_signing_tx_request) => {
                        let validators: Vec<_> = eth_signing_tx_request
                            .validators
                            .iter()
                            .map(|v| p2p::ValidatorId(v.clone().into()))
                            .collect();

                        // TODO: Should this hash be on the state chain or the signing module?
                        let hash = Keccak256::hash(&eth_signing_tx_request.payload[..]);
                        let message_hash = MessageHash(hash.0);

                        let signing_info = SigningInfo::new(
                            key_id,
                            self.validators
                                .get(&key_id)
                                .expect("validators should exist for current KeyId")
                                .clone(),
                        );

                        let sign_tx = MultisigInstruction::Sign(message_hash, validators);

                        self.mq_client
                            .publish(Subject::MultisigInstruction, &sign_tx)
                            .await
                            .expect("should publish to MQ");
                    }
                },
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
