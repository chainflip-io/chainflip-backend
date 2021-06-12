use chainflip_engine::{
    eth,
    mq::nats_client::NatsMQClientFactory,
    sc_observer,
    settings::Settings,
    signing::{self, crypto::Parameters},
};

#[tokio::main]
async fn main() {
    // init the logger
    env_logger::init();

    let settings = Settings::new().expect("Failed to initialise settings");

    log::info!("Start the engines! :broom: :broom: ");

    sc_observer::sc_observer::start(settings.clone()).await;

    eth::start(settings.clone())
        .await
        .expect("Should start ETH client");

    let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

    // TODO: clients need to be able to update their signer idx dynamically
    let signer_idx = 0;

    let params = Parameters {
        share_count: 150,
        threshold: 99,
    };

    let signing_client = signing::MultisigClient::new(mq_factory, signer_idx, params);

    signing_client.run().await;
}
