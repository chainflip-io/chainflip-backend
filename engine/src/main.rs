use chainflip_engine::{
    eth::{self, key_manager, stake_manager, EthBroadcaster},
    health::HealthMonitor,
    heartbeat,
    p2p::{self, rpc as p2p_rpc, AccountId, P2PMessage, P2PMessageCommand},
    settings::Settings,
    signing::{self, MultisigEvent, MultisigInstruction, PersistentKeyDB},
    state_chain,
};
use slog::{o, Drain};

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

    let (account_id, sc_state_chain_client, sc_event_stream, sc_block_stream) =
        state_chain::client::connect_to_state_chain(&settings)
            .await
            .unwrap();

    // TODO: Investigate whether we want to encrypt it on disk
    let db = PersistentKeyDB::new(&settings.signing.db_file.as_path(), &root_logger);

    let (_, p2p_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (_, shutdown_client_rx) = tokio::sync::oneshot::channel::<()>();
    let (multisig_instruction_sender, multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();

    let (multisig_event_sender, multisig_event_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigEvent>();

    let (p2p_message_sender, p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel::<P2PMessage>();
    let (p2p_message_command_sender, p2p_message_command_receiver) =
        tokio::sync::mpsc::unbounded_channel::<P2PMessageCommand>();

    let web3 = eth::new_synced_web3_client(&settings, &root_logger)
        .await
        .expect("Failed to create Web3 WebSocket");

    let eth_broadcaster =
        EthBroadcaster::new(&settings, web3.clone()).expect("Failed to create ETH broadcaster");

    tokio::join!(
        // Start signing components
        signing::start(
            AccountId(*account_id.as_ref()), /*TODO*/
            db,
            multisig_instruction_receiver,
            multisig_event_sender,
            p2p_message_receiver,
            p2p_message_command_sender,
            shutdown_client_rx,
            &root_logger,
        ),
        p2p::conductor::start(
            p2p_rpc::connect(
                &url::Url::parse(settings.state_chain.ws_endpoint.as_str()).expect(&format!(
                    "Should be valid ws endpoint: {}",
                    settings.state_chain.ws_endpoint
                )),
                AccountId(*account_id.as_ref()) /*TODO*/
            )
            .await
            .expect("unable to connect p2p rpc client"),
            p2p_message_sender,
            p2p_message_command_receiver,
            p2p_shutdown_rx,
            &root_logger.clone()
        ),
        heartbeat::start(sc_state_chain_client.clone(), sc_block_stream, &root_logger),
        // Start state chain components
        state_chain::sc_observer::start(
            &settings,
            sc_state_chain_client.clone(),
            sc_event_stream,
            eth_broadcaster,
            multisig_instruction_sender,
            multisig_event_receiver,
            &root_logger
        ),
        // Start eth components
        stake_manager::start_stake_manager_witness(
            &web3,
            &settings,
            sc_state_chain_client.clone(),
            &root_logger
        )
        .await
        .expect("Could not start StakeManager witness"),
        key_manager::start_key_manager_witness(
            &web3,
            &settings,
            sc_state_chain_client.clone(),
            &root_logger
        )
        .await
        .expect("Could not start KeyManager witness"),
    );
}
