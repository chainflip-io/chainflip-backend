use std::sync::Arc;

use anyhow::Context;
use chainflip_engine::{
    eth::{
        self, build_broadcast_channel,
        key_manager::KeyManager,
        rpc::{EthDualRpcClient, EthHttpRpcClient, EthRpcApi, EthWsRpcClient},
        stake_manager::StakeManager,
        EthBroadcaster,
    },
    health::HealthChecker,
    logging,
    multisig::{self, client::key_store::KeyStore, PersistentKeyDB},
    multisig_p2p,
    p2p_muxer::P2PMuxer,
    settings::{CommandLineOptions, Settings},
    state_chain,
    task_scope::with_main_task_scope,
};
use clap::Parser;
use futures::FutureExt;
use pallet_cf_validator::SemVer;
use sp_core::{
    crypto::{set_default_ss58_version, Ss58AddressFormat},
    U256,
};
use state_chain_runtime::constants::common::CHAINFLIP_SS58_PREFIX;

use crate::multisig::eth::EthSigning;

#[allow(clippy::eval_order_dependence)]
fn main() -> anyhow::Result<()> {
    set_default_ss58_version(Ss58AddressFormat::custom(CHAINFLIP_SS58_PREFIX)); // Sets global that ensures SC AccountId's are printed correctly

    let settings = Settings::new(CommandLineOptions::parse()).context("Error reading settings")?;

    let root_logger = logging::utils::new_json_logger_with_tag_filter(
        settings.log.whitelist.clone(),
        settings.log.blacklist.clone(),
    );

    slog::info!(root_logger, "Start the engines! :broom: :broom: ");

    with_main_task_scope(|scope| {
        async {

            if let Some(health_check_settings) = &settings.health_check {
                scope.spawn(HealthChecker::new(health_check_settings, &root_logger).await?.run());
            }

            // Init web3 and eth broadcaster before connecting to SC, so we can diagnose these config errors, before
            // we connect to the SC (which requires the user to be staked)
            let eth_ws_rpc_client = EthWsRpcClient::new(&settings.eth, &root_logger)
                .await
                .context("Failed to create EthWsRpcClient")?;

            let eth_http_rpc_client =
                EthHttpRpcClient::new(&settings.eth, &root_logger).context("Failed to create EthHttpRpcClient")?;

            let eth_dual_rpc =
                EthDualRpcClient::new(eth_ws_rpc_client.clone(), eth_http_rpc_client.clone());

            let eth_broadcaster = EthBroadcaster::new(&settings.eth, eth_dual_rpc.clone(), &root_logger)
                .context("Failed to create ETH broadcaster")?;

            let (latest_block_hash, state_chain_block_stream, state_chain_client) =
                state_chain::client::connect_to_state_chain(&settings.state_chain, true, &root_logger)
                    .await
                    .context("Failed to connect to state chain")?;

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
                .context("Failed to submit version to state chain")?;

            // TODO: Merge this into the MultisigClientApi
            let (account_peer_mapping_change_sender, account_peer_mapping_change_receiver) =
                tokio::sync::mpsc::unbounded_channel();

            let (
                witnessing_instruction_sender,
                [witnessing_instruction_receiver_1, witnessing_instruction_receiver_2, witnessing_instruction_receiver_3],
            ) = build_broadcast_channel(10);

            {
                // ensure configured eth node is pointing to the correct chain id
                let chain_id_from_sc = U256::from(state_chain_client
                    .get_storage_value::<pallet_cf_environment::EthereumChainId::<state_chain_runtime::Runtime>>(
                        latest_block_hash,
                    )
                    .await
                    .context("Failed to get EthereumChainId from SC")?);

                let chain_id_from_eth_ws = eth_ws_rpc_client
                    .chain_id()
                    .await
                    .context("Failed to fetch chain id")?;

                let chain_id_from_eth_http = eth_http_rpc_client
                    .chain_id()
                    .await
                    .context("Failed to fetch chain id")?;

                let ws_wrong_network = chain_id_from_sc != chain_id_from_eth_ws;
                let http_wrong_network = chain_id_from_sc != chain_id_from_eth_http;

                if ws_wrong_network || http_wrong_network {
                    return Err(anyhow::Error::msg(format!(
                        "the ETH nodes are NOT pointing to the ETH network with ChainId {}, Please ensure they are.{}{}",
                        chain_id_from_sc,
                        lazy_format::lazy_format!(
                            if ws_wrong_network => (" The WS ETH node is currently pointing to an ETH network with ChainId: {}.", chain_id_from_eth_ws)
                            else => ("")
                        ),
                        lazy_format::lazy_format!(
                            if http_wrong_network => (" The HTTP ETH node is currently pointing to an ETH network with ChainId: {}.", chain_id_from_eth_http)
                            else => ("")
                        ),
                    )));
                }
            }

            let cfe_settings = state_chain_client
                .get_storage_value::<pallet_cf_environment::CfeSettings<state_chain_runtime::Runtime>>(
                    latest_block_hash,
                )
                .await
                .context("Failed to get on chain CFE settings from SC")?;

            let (cfe_settings_update_sender, cfe_settings_update_receiver) =
                tokio::sync::watch::channel(cfe_settings);

            let stake_manager_address = state_chain_client
                .get_storage_value::<pallet_cf_environment::StakeManagerAddress::<
                    state_chain_runtime::Runtime,
                >>(latest_block_hash)
                .await
                .context("Failed to get StakeManager address from SC")?;
            let stake_manager_contract = StakeManager::new(stake_manager_address.into());

            let key_manager_address = state_chain_client
                .get_storage_value::<pallet_cf_environment::KeyManagerAddress::<
                    state_chain_runtime::Runtime,
                >>(latest_block_hash)
                .await
                .context("Failed to get KeyManager address from SC")?;

            let key_manager_contract =
                KeyManager::new(key_manager_address.into());

            let latest_ceremony_id = state_chain_client
            .get_storage_value::<pallet_cf_validator::CeremonyIdCounter<state_chain_runtime::Runtime>>(
                latest_block_hash,
            )
            .await
            .context("Failed to get CeremonyIdCounter from SC")?;

            let db = Arc::new(
                PersistentKeyDB::new_and_migrate_to_latest(
                    settings.signing.db_file.as_path(),
                    Some(state_chain_client.get_genesis_hash()),
                    &root_logger,
                )
                .context("Failed to open database")?,
            );

            // p2p -> muxer
            let (incoming_p2p_message_sender, incoming_p2p_message_receiver) =
                tokio::sync::mpsc::unbounded_channel();

            // muxer -> p2p
            let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
                tokio::sync::mpsc::unbounded_channel();

            let (eth_outgoing_sender, eth_incoming_receiver, muxer_future) = P2PMuxer::start(
                incoming_p2p_message_receiver,
                outgoing_p2p_message_sender,
                &root_logger,
            );

            scope.spawn(async move {
                muxer_future.await;
                Ok(())
            });

            let (eth_multisig_client, eth_multisig_client_backend_future) =
                multisig::start_client::<EthSigning>(
                    state_chain_client.our_account_id.clone(),
                    KeyStore::new(db),
                    eth_incoming_receiver,
                    eth_outgoing_sender,
                    latest_ceremony_id,
                    &root_logger,
                );
            scope.spawn(
                multisig_p2p::start(
                    &settings,
                    state_chain_client.clone(),
                    latest_block_hash,
                    incoming_p2p_message_sender,
                    outgoing_p2p_message_receiver,
                    account_peer_mapping_change_receiver,
                    &root_logger,
                )
            );
            // TODO Handle errors/panics from backend
            scope.spawn(async move {
                eth_multisig_client_backend_future.await;
                Ok(())
            });

            // Start eth observers
            scope.spawn(
                eth::start_contract_observer(
                    stake_manager_contract,
                    eth_ws_rpc_client.clone(),
                    eth_http_rpc_client.clone(),
                    witnessing_instruction_receiver_1,
                    state_chain_client.clone(),
                    &root_logger,
                )
            );
            scope.spawn(
                eth::start_contract_observer(
                    key_manager_contract,
                    eth_ws_rpc_client,
                    eth_http_rpc_client,
                    witnessing_instruction_receiver_2,
                    state_chain_client.clone(),
                    &root_logger,
                )
            );
            scope.spawn(
                eth::start_chain_data_witnesser(
                    eth_dual_rpc,
                    state_chain_client.clone(),
                    witnessing_instruction_receiver_3,
                    cfe_settings_update_receiver,
                    eth::ETH_CHAIN_TRACKING_POLL_INTERVAL,
                    &root_logger
                )
            );

            // Start state chain components
            let sc_observer_future = state_chain::sc_observer::start(
                state_chain_client.clone(),
                state_chain_block_stream,
                eth_broadcaster,
                eth_multisig_client,
                account_peer_mapping_change_sender,
                witnessing_instruction_sender,
                cfe_settings_update_sender,
                latest_block_hash,
                &root_logger
            );
            scope.spawn(async move {
                sc_observer_future.await;
                Ok(()) // TODO Handle errors/panics from sc_observer
            });

            Ok(())
        }.boxed()
    })
}
