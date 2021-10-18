use std::sync::Arc;

use chainflip_engine::{
    eth::{self, key_manager, stake_manager, EthBroadcaster},
    health::HealthMonitor,
    p2p::{self, rpc as p2p_rpc, AccountId, P2PMessage, P2PMessageCommand},
    settings::{CommandLineOptions, Settings},
    signing::{self, MultisigEvent, MultisigInstruction, PersistentKeyDB},
    state_chain::{
        self,
        xt_submitter::{AtomicNonce, XtSubmitter},
    },
};
use slog::{o, Drain};
use structopt::StructOpt;
use substrate_subxt::Signer;

#[allow(clippy::eval_order_dependence)]
#[tokio::main]
async fn main() {
    let settings =
        Settings::new(CommandLineOptions::from_args()).expect("Failed to initialise settings");

    let drain = slog_json::Json::new(std::io::stdout())
        .add_default_keys()
        .build()
        .fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let root_logger = slog::Logger::root(drain, o!());
    slog::info!(root_logger, "Start the engines! :broom: :broom: "; o!());

    HealthMonitor::new(&settings.health_check, &root_logger)
        .run()
        .await;

    let (state_chain_client, state_chain_block_stream) =
        state_chain::client::connect_to_state_chain(&settings)
            .await
            .unwrap();
    let account_id = AccountId(*state_chain_client.signer.account_id().as_ref()); /*TODO: Use the correct sc types*/

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

    // XtSubmitter
    let nonce = state_chain_client
        .nonce_at_block(None)
        .await
        .expect("Could not get latest nonce from State Chain");
    let atomic_nonce = Arc::new(AtomicNonce::new(nonce));

    let (xt_sender, xt_receiver) =
        tokio::sync::mpsc::unbounded_channel::<state_chain_runtime::Call>();

    let mut xt_submitter = XtSubmitter::new(
        state_chain_client.clone(),
        xt_receiver,
        atomic_nonce.clone(),
        &root_logger,
    );

    let web3 = eth::new_synced_web3_client(&settings, &root_logger)
        .await
        .expect("Failed to create Web3 WebSocket");

    let eth_broadcaster =
        EthBroadcaster::new(&settings, web3.clone()).expect("Failed to create ETH broadcaster");

    tokio::join!(
        // Start signing components
        signing::start(
            account_id.clone(),
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
            &settings,
            state_chain_client.clone(),
            atomic_nonce.clone(),
            xt_sender.clone(),
            state_chain_block_stream,
            eth_broadcaster,
            multisig_instruction_sender,
            multisig_event_receiver,
            &root_logger
        ),
        xt_submitter.start(),
        // Start eth components
        stake_manager::start_stake_manager_witness(&web3, &settings, xt_sender, &root_logger)
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
