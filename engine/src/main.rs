use chainflip_engine::{
    eth,
    health::health_check,
    mq::nats_client::NatsMQClientFactory,
    settings::Settings,
    signing::{self, crypto::Parameters},
    state_chain::{self},
};

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("Start the engines! :broom: :broom: ");

    let settings = Settings::new().expect("Failed to initialise settings");

    // can use this sender to shut down the health check gracefully
    let _sender = health_check(settings.engine.health_check_port).await;

    let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

    state_chain::sc_observer::start(settings.clone()).await;
    state_chain::sc_broadcaster::start(&settings, mq_factory.clone()).await;

    eth::start(settings.clone())
        .await
        .expect("Should start ETH client");

    // TODO: clients need to be able to update their signer idx dynamically
    let signer_idx = 0;

    let params = Parameters {
        share_count: 150,
        threshold: 99,
    };

    let signing_client = signing::MultisigClient::new(mq_factory, signer_idx, params);

    signing_client.run().await;
}
