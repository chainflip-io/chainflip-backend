use chainflip_engine::{
    eth,
    health::HealthMonitor,
    mq::{nats_client::NatsMQClientFactory, IMQClientFactory},
    p2p::{P2PConductor, RpcP2PClient, ValidatorId},
    settings::Settings,
    signing,
    signing::db::PersistentKeyDB,
    state_chain,
    temp_event_mapper::TempEventMapper,
};
use slog::Drain;
use sp_core::Pair;

#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_json;

#[tokio::main]
async fn main() {
    let drain = slog_json::Json::new(std::io::stdout())
        .add_default_keys()
        .build()
        .fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let root_logger = slog::Logger::root(drain, o!());
    slog::info!(root_logger, "Start the engines! :broom: :broom: "; o!());

    std::thread::sleep(std::time::Duration::from_secs(5));

    let settings = Settings::new().expect("Failed to initialise settings");

    let health_monitor = HealthMonitor::new(&settings.health_check, &root_logger);
    health_monitor.run().await;

    let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

    // This can be the same filepath as the p2p key --node-key-file <file> on the state chain
    // which won't necessarily always be the case, i.e. if we no longer have PeerId == ValidatorId
    let signer = state_chain::get_signer_from_privkey_file(&settings.state_chain.p2p_priv_key_file);
    let my_pubkey = signer.signer().public();
    let my_validator_id = ValidatorId(my_pubkey.0);
    let mq_client = *mq_factory
        .create()
        .await
        .expect("Could not connect MQ client");
    let sc_o = state_chain::sc_observer::SCObserver::new(
        mq_client.clone(),
        &settings.state_chain,
        &root_logger,
    )
    .await;
    let sc_o_fut = sc_o.run();
    let sc_b_fut =
        state_chain::sc_broadcaster::start(&settings, signer, mq_factory.clone(), &root_logger);

    let eth_fut = eth::start(&settings, &root_logger);

    let (_, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let substrate_node_endpoint = url::Url::parse(settings.state_chain.ws_endpoint.as_str())
        .expect(&format!(
            "Should be valid ws endpoint: {}",
            settings.state_chain.ws_endpoint
        ));
    let p2p_client = RpcP2PClient::new(substrate_node_endpoint);
    let mq_client = *mq_factory
        .create()
        .await
        .expect("Could not connect MQ client");
    let p2p_conductor_fut = P2PConductor::new(mq_client, p2p_client)
        .await
        .start(shutdown_rx);

    // TODO: Investigate whether we want to encrypt it on disk
    let db = PersistentKeyDB::new("data.db");

    let signing_client = signing::MultisigClient::new(db, mq_factory, my_validator_id);

    let temp_event_map_fut = TempEventMapper::run(&settings);

    let (_, shutdown_client_rx) = tokio::sync::oneshot::channel::<()>();

    let signing_client_fut = signing_client.run(shutdown_client_rx);

    futures::join!(
        sc_o_fut,
        sc_b_fut,
        eth_fut,
        temp_event_map_fut,
        p2p_conductor_fut,
        signing_client_fut,
    );
}
