use futures::StreamExt;
use slog::o;

use crate::{
    logging::COMPONENT_KEY,
    mq::{pin_message_stream, IMQClient, Subject},
    p2p::{self},
    signing::{KeyId, KeygenInfo, MultisigInstruction},
    state_chain::{auction, runtime::StateChainRuntime},
};

pub async fn start<MQC: IMQClient + Send + Sync>(mq_client: MQC, logger: &slog::Logger) {
    let temp_event_mapper = TempEventMapper::new(mq_client, logger);
    temp_event_mapper.run().await
}

/// Temporary event mapper for the internal testnet
pub struct TempEventMapper<MQC: IMQClient + Send + Sync> {
    mq_client: MQC,
    logger: slog::Logger,
}

impl<MQC: IMQClient + Send + Sync> TempEventMapper<MQC> {
    pub fn new(mq_client: MQC, logger: &slog::Logger) -> Self {
        Self {
            mq_client,
            logger: logger.new(o!(COMPONENT_KEY => "TempEventMapper")),
        }
    }

    pub async fn run(&self) {
        slog::info!(self.logger, "Starting");

        let auction_completed_event = self
            .mq_client
            .subscribe::<auction::AuctionCompletedEvent<StateChainRuntime>>(
                Subject::AuctionCompleted,
            )
            .await
            .unwrap();

        let auction_completed_event = pin_message_stream(auction_completed_event);

        auction_completed_event
            .for_each_concurrent(None, |evt| async {
                let event = evt.expect("Should be an event here");
                slog::debug!(
                    self.logger,
                    "Temp event mapper received AuctionCompleted event: {:?}",
                    event
                );
                let validators: Vec<_> = event
                    .validators
                    .iter()
                    .map(|v| p2p::ValidatorId(v.clone().into()))
                    .collect();

                slog::debug!(
                    self.logger,
                    "Validators in that were in the auction are: {:?}",
                    validators
                );

                let key_gen_info = KeygenInfo::new(KeyId(event.auction_index), validators);
                let gen_new_key_event = MultisigInstruction::KeyGen(key_gen_info);
                self.mq_client
                    .publish(Subject::MultisigInstruction, &gen_new_key_event)
                    .await
                    .expect("Should push new key gen event to multisig instruction queue");
            })
            .await;
        slog::error!(
            self.logger,
            "Temp mapper has stopped. Whatever shall we do!"
        );
    }
}
