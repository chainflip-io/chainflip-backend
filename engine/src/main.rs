use crate::mq::Options;

use log::info;

mod mq;
mod p2p;
mod sc_observer;
mod settings;
mod witness;

use settings::Settings;

#[tokio::main]
async fn main() {
    // init the logger
    env_logger::init();

    let settings = Settings::new().expect("Failed to initialise settings");

    // set up the message queue
    let mq_options = Options {
        url: format!(
            "{}:{}",
            settings.message_queue.hostname, settings.message_queue.port
        ),
    };

    info!("Start the engines! :broom: :broom: ");

    sc_observer::sc_observer::start(mq_options.clone(), settings.state_chain).await;

    // start witnessing other chains
    witness::witness::start(mq_options).await;
}
