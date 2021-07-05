use futures::StreamExt;

use crate::{
    mq::{
        nats_client::NatsMQClientFactory, pin_message_stream, IMQClient, IMQClientFactory, Subject,
    },
    p2p::{self},
    settings::Settings,
    signing::{KeyId, KeygenInfo, MultisigInstruction},
    state_chain::{auction, runtime::StateChainRuntime},
};

/// Temporary event mapper for the internal testnet
pub struct TempEventMapper {}

impl TempEventMapper {
    pub async fn run(settings: &Settings) {
        let nats_client_factory = NatsMQClientFactory::new(&settings.message_queue);
        let mq_client = *nats_client_factory.create().await.unwrap();

        let auction_confirmed_event_stream = mq_client
            .subscribe::<auction::AuctionCompletedEvent<StateChainRuntime>>(
                Subject::AuctionCompleted,
            )
            .await
            .unwrap();

        let auction_confirmed_event_stream = pin_message_stream(auction_confirmed_event_stream);

        auction_confirmed_event_stream
            .for_each_concurrent(None, |evt| async {
                let event = evt.expect("Should be an event here");

                let validators: Vec<_> = event
                    .validators
                    .iter()
                    .map(|v| p2p::ValidatorId(v.to_string()))
                    .collect();

                let key_gen_info = KeygenInfo::new(KeyId(event.auction_index), validators);
                let gen_new_key_event = MultisigInstruction::KeyGen(key_gen_info);
                mq_client
                    .publish(Subject::MultisigInstruction, &gen_new_key_event)
                    .await
                    .expect("Should push event new key gen event to multisig instruction queue");
            })
            .await;
        log::error!("Temp mapper has stopped. Whatever shall we do!");
    }
}
