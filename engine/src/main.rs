use chainflip_engine::{
    eth::{
        self,
        key_manager::KeyManager,
        rpc::{EthDualRpcClient, EthHttpRpcClient, EthRpcApi, EthWsRpcClient},
        stake_manager::StakeManager,
        EthBroadcaster,
    },
    health::HealthMonitor,
    logging,
    multisig::{self, PersistentKeyDB},
    multisig_p2p,
    settings::{CommandLineOptions, Settings},
    state_chain,
};
use clap::Parser;
use pallet_cf_validator::SemVer;
use sp_core::U256;

#[allow(clippy::eval_order_dependence)]
#[tokio::main]
async fn main() {
    let settings = match Settings::new(CommandLineOptions::parse()) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("Error reading settings: {}", error);
            return;
        }
    };

    let root_logger = logging::utils::new_json_logger_with_tag_filter(
        settings.log.whitelist.clone(),
        settings.log.blacklist.clone(),
    );

    slog::info!(root_logger, "Start the engines! :broom: :broom: ");

    if let Some(health_check_settings) = &settings.health_check {
        HealthMonitor::new(health_check_settings, &root_logger)
            .run()
            .await;
    }

    // Init web3 and eth broadcaster before connecting to SC, so we can diagnose these config errors, before
    // we connect to the SC (which requires the user to be staked)
    let eth_ws_rpc_client = EthWsRpcClient::new(&settings.eth, &root_logger)
        .await
        .expect("Should create EthWsRpcClient");

    let eth_http_rpc_client =
        EthHttpRpcClient::new(&settings.eth, &root_logger).expect("Should create EthHttpRpcClient");

    let eth_dual_rpc =
        EthDualRpcClient::new(eth_ws_rpc_client.clone(), eth_http_rpc_client.clone());

    let eth_broadcaster = EthBroadcaster::new(&settings.eth, eth_dual_rpc.clone(), &root_logger)
        .expect("Failed to create ETH broadcaster");

    let (latest_block_hash, state_chain_block_stream, state_chain_client) =
        state_chain::client::connect_to_state_chain(&settings.state_chain, true, &root_logger)
            .await
            .expect("Failed to connect to state chain");

    state_chain_client
        .submit_signed_extrinsic(
            pallet_cf_validator::Call::cfe_version {
                version: SemVer {
                    major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
                    minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
                    patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
                },
            },
            &root_logger,
        )
        .await
        .expect("Should submit version to state chain");

    // TODO: Investigate whether we want to encrypt it on disk
    let db = PersistentKeyDB::new_and_migrate_to_latest(
        settings.signing.db_file.as_path(),
        &root_logger,
    )
    .expect("Failed to open database");

    // TODO: Merge this into the MultisigClientApi
    let (account_peer_mapping_change_sender, account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (incoming_p2p_message_sender, incoming_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    // TODO: multi consumer, single producer?
    let (sm_instruction_sender, sm_instruction_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (km_instruction_sender, km_instruction_receiver) = tokio::sync::mpsc::unbounded_channel();

    {
        // ensure configured eth node is pointing to the correct chain id
        let chain_id_from_sc = U256::from(state_chain_client
            .get_storage_value::<pallet_cf_environment::EthereumChainId::<state_chain_runtime::Runtime>>(
                latest_block_hash,
            )
            .await
            .expect("Should get EthereumChainId from SC"));

        let chain_id_from_eth_ws = eth_ws_rpc_client
            .chain_id()
            .await
            .expect("Should fetch chain id");

        let chain_id_from_eth_http = eth_http_rpc_client
            .chain_id()
            .await
            .expect("Should fetch chain id");

        let mut has_wrong_chain_id = false;
        if chain_id_from_sc != chain_id_from_eth_ws {
            slog::error!(
                &root_logger,
                "The WS ETH node is pointing to ETH network with ChainId: {}. Please ensure it's pointing to network with ChainId {}",
                chain_id_from_eth_ws,
                chain_id_from_sc,
            );
            has_wrong_chain_id = true;
        }
        if chain_id_from_sc != chain_id_from_eth_http {
            slog::error!(
                &root_logger,
                "The HTTP ETH node is pointing to ETH network with ChainId: {}. Please ensure it's pointing to network with ChainId {}",
                chain_id_from_eth_http,
                chain_id_from_sc,
            );
            has_wrong_chain_id = true;
        }
        if has_wrong_chain_id {
            return;
        }
    }

    let stake_manager_address = state_chain_client
        .get_storage_value::<pallet_cf_environment::StakeManagerAddress::<
            state_chain_runtime::Runtime,
        >>(latest_block_hash)
        .await
        .expect("Should get StakeManager address from SC");
    let stake_manager_contract = StakeManager::new(stake_manager_address.into())
        .expect("Should create StakeManager contract");

    let key_manager_address = state_chain_client
        .get_storage_value::<pallet_cf_environment::KeyManagerAddress::<
            state_chain_runtime::Runtime,
        >>(latest_block_hash)
        .await
        .expect("Should get KeyManager address from SC");

    let key_manager_contract = KeyManager::new(key_manager_address.into(), eth_dual_rpc)
        .expect("Should create KeyManager contract");

    use crate::multisig::eth::EthSigning;

    let (eth_multisig_client, eth_multisig_client_backend_future) =
        multisig::start_client::<_, EthSigning>(
            state_chain_client.our_account_id.clone(),
            db,
            incoming_p2p_message_receiver,
            outgoing_p2p_message_sender,
            &root_logger,
        );

    tokio::join!(
        eth_multisig_client_backend_future,
        async {
            multisig_p2p::start(
                &settings,
                state_chain_client.clone(),
                latest_block_hash,
                incoming_p2p_message_sender,
                outgoing_p2p_message_receiver,
                account_peer_mapping_change_receiver,
                &root_logger,
            )
            .await
            .expect("Error in P2P component")
        },
        // Start state chain components
        state_chain::sc_observer::start(
            state_chain_client.clone(),
            state_chain_block_stream,
            eth_broadcaster,
            eth_multisig_client,
            account_peer_mapping_change_sender,
            // send messages to these channels to start witnessing
            sm_instruction_sender,
            km_instruction_sender,
            latest_block_hash,
            &root_logger
        ),
        // Start eth observers
        eth::start_contract_observer(
            stake_manager_contract,
            &eth_ws_rpc_client,
            &eth_http_rpc_client,
            sm_instruction_receiver,
            state_chain_client.clone(),
            &root_logger,
        ),
        eth::start_contract_observer(
            key_manager_contract,
            &eth_ws_rpc_client,
            &eth_http_rpc_client,
            km_instruction_receiver,
            state_chain_client.clone(),
            &root_logger,
        ),
    );
}
