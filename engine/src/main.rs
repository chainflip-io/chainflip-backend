use chainflip_engine::{
    eth::{self, key_manager, stake_manager, EthBroadcaster},
    health::HealthMonitor,
    logging,
    multisig::{self, MultisigEvent, MultisigInstruction, PersistentKeyDB},
    p2p::{self, rpc as p2p_rpc, AccountId, P2PMessage, P2PMessageCommand},
    settings::{CommandLineOptions, Settings},
    state_chain,
};
use pallet_cf_validator::SemVer;
use structopt::StructOpt;

#[allow(clippy::eval_order_dependence)]
#[tokio::main]
async fn main() {
    let settings =
        Settings::new(CommandLineOptions::from_args()).expect("Failed to initialise settings");

    let root_logger = logging::utils::new_json_logger_with_tag_filter(
        settings.log.whitelist.clone(),
        settings.log.blacklist.clone(),
    );

    slog::info!(root_logger, "Start the engines! :broom: :broom: ");

    HealthMonitor::new(&settings.health_check, &root_logger)
        .run()
        .await;

    let (state_chain_client, state_chain_block_stream) =
        state_chain::client::connect_to_state_chain(&settings.state_chain)
            .await
            .unwrap();

    let account_id = AccountId(*state_chain_client.our_account_id.as_ref());

    state_chain_client
        .submit_extrinsic(
            &root_logger,
            pallet_cf_validator::Call::cfe_version(SemVer {
                major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
                minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
                patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
            }),
        )
        .await
        .expect("Should submit version to state chain");

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
        multisig::start_client(
            account_id.clone(),
            db,
            multisig_instruction_receiver,
            multisig_event_sender,
            p2p_message_receiver,
            p2p_message_command_sender,
            shutdown_client_rx,
            multisig::KeygenOptions::default(),
            &root_logger,
        ),
        p2p::conductor::start(
            p2p_rpc::connect(
                &url::Url::parse(settings.state_chain.ws_endpoint.as_str()).unwrap_or_else(
                    |e| panic!(
                        "Should be valid ws endpoint: {}: {}",
                        settings.state_chain.ws_endpoint, e
                    )
                ),
                account_id
            )
            .await
            .expect("unable to connect p2p rpc client"),
            p2p_message_sender,
            p2p_message_command_receiver,
            p2p_shutdown_rx,
            &root_logger
        ),
        // Start state chain components
        state_chain::sc_observer::start(
            state_chain_client.clone(),
            state_chain_block_stream,
            eth_broadcaster,
            multisig_instruction_sender,
            multisig_event_receiver,
            &root_logger
        ),
        // Start eth components
        stake_manager::start_stake_manager_witness(
            &web3,
            &settings,
            state_chain_client.clone(),
            &root_logger
        )
        .await
        .expect("Could not start StakeManager witness"),
        key_manager::start_key_manager_witness(
            &web3,
            &settings,
            state_chain_client.clone(),
            &root_logger
        )
        .await
        .expect("Could not start KeyManager witness"),
    );
}
