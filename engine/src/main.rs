use chainflip_engine::{
    eth,
    mq::nats_client::NatsMQClientFactory,
    sc_observer,
    settings::Settings,
    signing::{self, crypto::Parameters},
};
// use std::{io::Write, net::TcpListener};

use async_std::net::TcpListener;
use futures::{AsyncWriteExt, StreamExt};

async fn health_check(port: u16) {
    let bind_address = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(bind_address)
        .await
        .expect(format!("Could not bind TCP listener to port {}", port).as_str());

    let mut incoming = listener.incoming();

    while let Some(stream) = incoming.next().await {
        let mut stream = stream.expect("Could not open CFE health check TCP stream");
        let http_200_response = "HTTP/1.1 200 OK\r\n\r\n";
        stream
            .write(http_200_response.as_bytes())
            .await
            .expect("Could not write to health check stream");
        log::trace!("Responded to health check: CFE is healthy :heart: ");
        stream
            .flush()
            .await
            .expect("Could not flush health check TCP stream");
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("Start the engines! :broom: :broom: ");

    let settings = Settings::new().expect("Failed to initialise settings");

    health_check(settings.engine.health_check_port).await;

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

#[cfg(test)]
mod test {

    use super::*;

    #[tokio::test]
    #[ignore = "runs forever"]
    async fn health_check_test() {
        health_check(5555u16).await;
    }
}
