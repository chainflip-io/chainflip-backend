use chainflip_engine::{
    eth,
    health::HealthMonitor,
    mq::nats_client::NatsMQClient,
    p2p::{P2PConductor, RpcP2PClient, ValidatorId},
    settings::Settings,
    signing,
    signing::db::PersistentKeyDB,
    state_chain::{self, runtime::StateChainRuntime},
    temp_event_mapper,
};
use slog::{o, Drain};
use sp_core::Pair;
use substrate_subxt::ClientBuilder;

#[tokio::main]
async fn main() {
    let drain = slog_json::Json::new(std::io::stdout())
        .add_default_keys()
        .build()
        .fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let root_logger = slog::Logger::root(drain, o!());
    slog::info!(root_logger, "Start the engines! :broom: :broom: "; o!());

    let settings = Settings::new().expect("Failed to initialise settings");

    HealthMonitor::new(&settings.health_check, &root_logger)
        .run()
        .await;

    slog::info!(
        &root_logger,
        "Connecting to NatsMQ at: {}",
        &settings.message_queue.endpoint
    );
    let mq_client = NatsMQClient::new(&settings.message_queue)
        .await
        .expect("Should connect to message queue");

    let subxt_client = ClientBuilder::<StateChainRuntime>::new()
        .set_url(&settings.state_chain.ws_endpoint)
        .build()
        .await
        .expect("Should create subxt client");

    // This can be the same filepath as the p2p key --node-key-file <file> on the state chain
    // which won't necessarily always be the case, i.e. if we no longer have PeerId == ValidatorId
    let my_pair_signer =
        state_chain::get_signer_from_privkey_file(&settings.state_chain.p2p_priv_key_file);

    // TODO: Investigate whether we want to encrypt it on disk
    let db = PersistentKeyDB::new(&settings.signing.db_file, &root_logger);

    let (_, p2p_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (_, shutdown_client_rx) = tokio::sync::oneshot::channel::<()>();

    futures::join!(
        signing::MultisigClient::new(
            db,
            mq_client.clone(),
            ValidatorId(my_pair_signer.signer().public().0),
            &root_logger,
        )
        .run(shutdown_client_rx),
        state_chain::sc_observer::start(mq_client.clone(), subxt_client.clone(), &root_logger),
        state_chain::sc_broadcaster::start(
            my_pair_signer,
            mq_client.clone(),
            subxt_client.clone(),
            &root_logger
        ),
        eth::start(&settings, mq_client.clone(), &root_logger),
        temp_event_mapper::start(mq_client.clone(), &root_logger),
        P2PConductor::new(
            mq_client,
            RpcP2PClient::new(
                url::Url::parse(settings.state_chain.ws_endpoint.as_str()).expect(&format!(
                    "Should be valid ws endpoint: {}",
                    settings.state_chain.ws_endpoint
                )),
                &root_logger
            ),
            &root_logger
        )
        .await
        .start(p2p_shutdown_rx),
    );
}
