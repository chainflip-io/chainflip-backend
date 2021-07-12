use chainflip_engine::{
    eth, health::health_check, mq::nats_client::NatsMQClientFactory, settings::Settings, signing,
    state_chain, temp_event_mapper::TempEventMapper,
};

#[tokio::main]
async fn main() {
    log4rs::init_file("./config/log4rs.yml", Default::default())
        .expect("Should have logging configuration at config/log4rs.yml");

    log::info!("Start the engines! :broom: :broom: ");

    let settings = Settings::new().expect("Failed to initialise settings");

    tokio::spawn(health_check(settings.clone().health_check));

    let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

    let sc_o_fut = state_chain::sc_observer::start(settings.clone());
    let sc_b_fut = state_chain::sc_broadcaster::start(&settings, mq_factory.clone());

    let eth_fut = eth::start(settings.clone());

    let signer_id = state_chain::node_id::get_peer_id(&settings.state_chain)
        .await
        .expect("Should receive a ValidatorId");
    let signing_client = signing::MultisigClient::new(mq_factory, signer_id);

    let temp_event_map_fut = TempEventMapper::run(&settings);

    let signing_client_fut = signing_client.run();

    futures::join!(
        sc_o_fut,
        sc_b_fut,
        eth_fut,
        temp_event_map_fut,
        signing_client_fut
    );
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_example_log_file_valid() {
        log4rs::init_file("config/log4rs.example.yml", Default::default()).unwrap();
    }
}
