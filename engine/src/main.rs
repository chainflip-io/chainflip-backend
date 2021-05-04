use std::sync::Arc;

use tokio::sync::Mutex;

use crate::mq::{nats_client::NatsMQClient, IMQClient, Options};

mod mq;
mod p2p;
mod sc_observer;
mod witness;

#[tokio::main]
async fn main() {
    println!("Hello from the CFE!");

    // start the engines

    // set up the message queue
    // TODO: Use a config file:
    let options = Options {
        url: "localhost:9944".to_string(),
    };
    let mq_client = NatsMQClient::connect(options).await.unwrap();
    let mq_client = Arc::new(Mutex::new(*mq_client));

    sc_observer::sc_observer::start(mq_client.clone()).await;

    // start witnessing other chains
    witness::witness::start(mq_client.clone()).await;
}
