use chainflip_engine::{
    eth,
    health::spawn_health_check,
    mq::{nats_client::NatsMQClientFactory, IMQClientFactory},
    p2p::{P2PConductor, RpcP2PClient, RpcP2PClientMapper, ValidatorId},
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

    // This can be the same filepath as the p2p key --node-key-file <file> on the state chain
    // which won't necessarily always be the case, i.e. if we no longer have PeerId == ValidatorId
    let signer = state_chain::get_signer_from_privkey_file(&settings.state_chain.p2p_priv_key_file);
    let my_pubkey = signer.signer().public();
    let signer_id = ValidatorId(my_pubkey.0);

    // ==== State chain components ====
    let sc_o_fut = state_chain::sc_observer::start(settings.clone());
    let sc_b_fut = state_chain::sc_broadcaster::start(&settings, signer, mq_factory.clone());

    let eth_fut = eth::start(settings.clone());

    let (_, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let ws_port = settings.state_chain.ws_port;

    let url = url::Url::parse(&format!(
        "ws://{}:{}",
        &settings.state_chain.hostname, ws_port
    ))
    .expect("should be a valid ws url");
    let mq_client = *mq_factory.create().await.unwrap();

    // ==== P2P RPC components ====
    let rpc_p2p_client_mapper = RpcP2PClientMapper::init(&settings.state_chain, mq_client.clone())
        .await
        .expect("Should initialise the P2P mapper");
    let peer_id_validator_map = rpc_p2p_client_mapper.clone_map();
    let rpc_p2p_client_mapper_sync_fut = rpc_p2p_client_mapper.sync_validator_mapping();
    let p2p_client = RpcP2PClient::new(url, peer_id_validator_map);
    let p2p_conductor_fut = P2PConductor::new(mq_client, p2p_client)
        .await
        .start(shutdown_rx);

    // TODO: Investigate whether we want to encrypt it on disk
    let db = PersistentKeyDB::new("data.db");

    let signing_client = signing::MultisigClient::new(db, mq_factory, signer_id);

    let temp_event_map_fut = TempEventMapper::run(&settings);

    let (_, shutdown_client_rx) = tokio::sync::oneshot::channel::<()>();

    let signing_client_fut = signing_client.run(shutdown_client_rx);

    futures::join!(
        sc_o_fut,
        sc_b_fut,
        eth_fut,
        temp_event_map_fut,
        rpc_p2p_client_mapper_sync_fut,
        p2p_conductor_fut,
        signing_client_fut,
    );
}
