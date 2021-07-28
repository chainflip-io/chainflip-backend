use chainflip_engine::{
    eth,
    health::spawn_health_check,
    mq::{nats_client::NatsMQClientFactory, IMQClientFactory},
    p2p::{P2PConductor, RpcP2PClient, ValidatorId},
    settings::Settings,
    signing,
    signing::db::PersistentKeyDB,
    state_chain,
    temp_event_mapper::TempEventMapper,
};
use sp_core::Pair;

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("Start the engines! :broom: :broom: ");

    let settings = Settings::new().expect("Failed to initialise settings");

    spawn_health_check(settings.clone().health_check).await;

    let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

    let mq_client = *mq_factory
        .create()
        .await
        .expect("Could not connect MQ client");

    // This can be the same filepath as the p2p key --node-key-file <file> on the state chain
    // which won't necessarily always be the case, i.e. if we no longer have PeerId == ValidatorId
    let my_pair_signer =
        state_chain::get_signer_from_privkey_file(&settings.state_chain.p2p_priv_key_file);

    // TODO: Investigate whether we want to encrypt it on disk
    // TODO: This path should be a configuration option
    let db = PersistentKeyDB::new("data.db");

    let (_, p2p_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (_, shutdown_client_rx) = tokio::sync::oneshot::channel::<()>();

    futures::join!(
        signing::MultisigClient::new(
            db,
            mq_client.clone(),
            ValidatorId(my_pair_signer.signer().public().0)
        )
        .run(shutdown_client_rx),
        state_chain::sc_observer::start(&settings, mq_client.clone()),
        state_chain::sc_broadcaster::start(&settings, my_pair_signer, mq_client.clone()),
        eth::start(&settings, mq_client.clone()),
        TempEventMapper::run(&settings),
        P2PConductor::new(
            mq_client,
            RpcP2PClient::new(
                url::Url::parse(settings.state_chain.ws_endpoint.as_str()).expect(&format!(
                    "Should be valid ws endpoint: {}",
                    settings.state_chain.ws_endpoint
                ))
            )
        )
        .await
        .start(p2p_shutdown_rx),
    );
}
