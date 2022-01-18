use chainflip_engine::{
    eth::{
        self, key_manager::KeyManager, stake_manager::StakeManager, EthBroadcaster, EthRpcApi,
        EthRpcClient,
    },
    health::HealthMonitor,
    logging,
    multisig::{self, MultisigInstruction, MultisigOutcome, PersistentKeyDB},
    multisig_p2p,
    settings::{CommandLineOptions, Settings},
    state_chain,
};
use pallet_cf_validator::SemVer;
use pallet_cf_vaults::BlockHeightWindow;
use sp_core::{storage::StorageKey, U256};
use structopt::StructOpt;

#[allow(clippy::eval_order_dependence)]
#[tokio::main]
async fn main() {
    let settings = match Settings::new(CommandLineOptions::from_args()) {
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

    HealthMonitor::new(&settings.health_check, &root_logger)
        .run()
        .await;

    // Init web3 and eth broadcaster before connecting to SC, so we can diagnose these config errors, before
    // we connect to the SC (which requires the user to be staked)
    let eth_rpc_client = EthRpcClient::new(&settings.eth, &root_logger)
        .await
        .expect("Should create EthRpcClient");

    let eth_broadcaster = EthBroadcaster::new(&settings.eth, eth_rpc_client.clone(), &root_logger)
        .expect("Failed to create ETH broadcaster");

    let (latest_block_hash, state_chain_block_stream, state_chain_client) =
        state_chain::client::connect_to_state_chain(&settings.state_chain, true, &root_logger)
            .await
            .expect("Failed to connect to state chain");

    state_chain_client
        .submit_signed_extrinsic(
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
    let db = PersistentKeyDB::new(settings.signing.db_file.as_path(), &root_logger);

    let (_, shutdown_client_rx) = tokio::sync::oneshot::channel::<()>();
    let (multisig_instruction_sender, multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
    // TODO: Merge this into the MultisigInstruction channel
    let (account_peer_mapping_change_sender, account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigOutcome>();

    let (incoming_p2p_message_sender, incoming_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    // TODO: multi consumer, single producer?
    let (sm_window_sender, sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    {
        // ensure configured eth node is pointing to the correct chain id
        let chain_id_from_sc = U256::from(state_chain_client
        .get_environment_value::<u64>(
            latest_block_hash,
            StorageKey(
                pallet_cf_environment::EthereumChainId::<state_chain_runtime::Runtime>::hashed_key(
                )
                .into(),
            ),
        )
        .await
        .expect("Should get EthereumChainId from SC"));

        let chain_id_from_eth = eth_rpc_client
            .chain_id()
            .await
            .expect("Should fetch chain id");

        if chain_id_from_sc != chain_id_from_eth {
            slog::error!(
            &root_logger,
            "Ethereum node pointing to ChainId {}, which is incorrect. Please ensure your Ethereum node is pointing to the network with ChainId: {}",
            chain_id_from_eth,
            chain_id_from_sc
        );
            return;
        }
    }

    let stake_manager_address = state_chain_client
        .get_environment_value(
            latest_block_hash,
            StorageKey(pallet_cf_environment::StakeManagerAddress::<
                state_chain_runtime::Runtime,
            >::hashed_key().into()),
        )
        .await
        .expect("Should get StakeManager address from SC");
    let stake_manager_contract =
        StakeManager::new(stake_manager_address).expect("Should create StakeManager contract");

    let key_manager_address = state_chain_client
        .get_environment_value(latest_block_hash, StorageKey(pallet_cf_environment::KeyManagerAddress::<
            state_chain_runtime::Runtime,
        >::hashed_key().into()))
        .await
        .expect("Should get KeyManager address from SC");

    let key_manager_contract =
        KeyManager::new(key_manager_address).expect("Should create KeyManager contract");

    tokio::join!(
        // Start signing components
        multisig::start_client(
            state_chain_client.our_account_id.clone(),
            db,
            multisig_instruction_receiver,
            multisig_outcome_sender,
            incoming_p2p_message_receiver,
            outgoing_p2p_message_sender,
            shutdown_client_rx,
            multisig::KeygenOptions::default(),
            &root_logger,
        ),
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
            multisig_instruction_sender,
            account_peer_mapping_change_sender,
            multisig_outcome_receiver,
            // send messages to these channels to start witnessing
            sm_window_sender,
            km_window_sender,
            latest_block_hash,
            &root_logger
        ),
        // Start eth observors
        eth::start_contract_observer(
            stake_manager_contract,
            &eth_rpc_client,
            sm_window_receiver,
            state_chain_client.clone(),
            &root_logger,
        ),
        eth::start_contract_observer(
            key_manager_contract,
            &eth_rpc_client,
            km_window_receiver,
            state_chain_client.clone(),
            &root_logger,
        ),
    );
}
