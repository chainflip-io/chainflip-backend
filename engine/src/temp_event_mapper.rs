use futures::StreamExt;

use crate::{
    mq::{pin_message_stream, IMQClient, Subject},
    p2p::{self},
    signing::{KeyId, KeygenInfo, MultisigInstruction},
    state_chain::{auction, runtime::StateChainRuntime},
};

pub async fn start<MQC: IMQClient + Send + Sync>(mq_client: MQC) {
    let temp_event_mapper = TempEventMapper::new(mq_client);
    temp_event_mapper.run().await
}

/// Temporary event mapper for the internal testnet
pub struct TempEventMapper<MQC: IMQClient + Send + Sync> {
    mq_client: MQC,
}

impl<MQC: IMQClient + Send + Sync> TempEventMapper<MQC> {
    pub fn new(mq_client: MQC) -> Self {
        Self { mq_client }
    }

    pub async fn run(&self) {
        log::info!("Starting temp event mapper");

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
                log::debug!(
                    "Temp event mapper received AuctionCompleted event: {:?}",
                    event
                );
                let validators: Vec<_> = event
                    .validators
                    .iter()
                    .map(|v| p2p::ValidatorId(v.clone().into()))
                    .collect();

                log::debug!(
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
        log::error!("Temp mapper has stopped. Whatever shall we do!");
    }
}
