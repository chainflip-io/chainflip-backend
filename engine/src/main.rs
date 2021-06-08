use chainflip_engine::{
    eth,
    mq::nats_client::NatsMQClientFactory,
    sc_observer,
    settings::Settings,
    signing::{self, crypto::Parameters},
    witness,
};

mod mq;
mod p2p;
mod settings;

#[tokio::main]
async fn main() {
    // init the logger
    env_logger::init();

    let settings = Settings::new().expect("Failed to initialise settings");

    log::info!("Start the engines! :broom: :broom: ");

    sc_observer::sc_observer::start(settings.clone()).await;

    eth::start(settings.clone()).await;

    let mq_factory = NatsMQClientFactory::new(settings.message_queue.clone());

    // TODO: clients need to be able to update their signer idx dynamically
    let signer_idx = 0;

    let params = Parameters {
        share_count: 150,
        threshold: 99,
    };

    let signing_client = signing::MultisigClient::new(mq_factory, signer_idx, params);

    let _ = futures::join!(
        signing_client.run(),
        witness::witness::start(settings.message_queue)
    );

    // start witnessing other chains
}
