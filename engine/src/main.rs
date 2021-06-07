use chainflip_engine::{eth, sc_observer, settings::Settings, witness};

mod mq;
mod p2p;
mod settings;
mod signing;

#[tokio::main]
async fn main() {
    // init the logger
    env_logger::init();

    let settings = Settings::new().expect("Failed to initialise settings");

    log::info!("Start the engines! :broom: :broom: ");

    sc_observer::sc_observer::start(settings.clone()).await;

    eth::start(settings.clone()).await;

    // start witnessing other chains
    witness::witness::start(settings.message_queue).await;
}
