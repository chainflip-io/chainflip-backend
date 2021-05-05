use std::sync::Arc;

use tokio::sync::Mutex;

use crate::mq::{nats_client::NatsMQClient, IMQClient, Options};

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
    let mq_client = NatsMQClient::connect(mq_options).await.unwrap();
    let mq_client = Arc::new(Mutex::new(*mq_client));

    info!("Start the engines! :broom: :broom: ");

    sc_observer::sc_observer::start(mq_client.clone(), settings.state_chain).await;

    // start witnessing other chains
    witness::witness::start(mq_client.clone()).await;
}
