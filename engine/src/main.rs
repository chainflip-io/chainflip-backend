use std::sync::Arc;

use crate::multisig::eth::EthSigning;
use anyhow::{bail, Context};
use chainflip_engine::{
    common::format_iterator,
    eth::{
        self, build_broadcast_channel,
        key_manager::KeyManager,
        rpc::{validate_client_chain_id, EthDualRpcClient, EthHttpRpcClient, EthWsRpcClient},
        stake_manager::StakeManager,
        EthBroadcaster,
    },
    health::HealthChecker,
    logging,
    multisig::{self, client::key_store::KeyStore, PersistentKeyDB},
    multisig_p2p,
    p2p_muxer::P2PMuxer,
    settings::{CommandLineOptions, Settings},
    state_chain_observer,
    task_scope::with_main_task_scope,
};
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use futures::FutureExt;
use pallet_cf_validator::SemVer;
use sp_core::U256;
use utilities::print_chainflip_ascii_art;

fn main() -> anyhow::Result<()> {
    print_chainflip_ascii_art();
    use_chainflip_account_id_encoding();

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
                EthDualRpcClient::new(eth_ws_rpc_client.clone(), eth_http_rpc_client.clone(), &root_logger);

            let eth_broadcaster = EthBroadcaster::new(&settings.eth, eth_dual_rpc.clone(), &root_logger)
                .context("Failed to create ETH broadcaster")?;

            let (latest_block_hash, state_chain_block_stream, state_chain_client) =
                state_chain_observer::client::connect_to_state_chain(&settings.state_chain, true, &root_logger)
                    .await?;

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

            // validate chain ids
            {
                let expected_chain_id = U256::from(state_chain_client
                    .get_storage_value::<pallet_cf_environment::EthereumChainId::<state_chain_runtime::Runtime>>(
                        latest_block_hash,
                    )
                    .await
                    .context("Failed to get EthereumChainId from state chain")?);

                let mut errors = [
                    validate_client_chain_id(
                        &eth_ws_rpc_client,
                        expected_chain_id,
                    ).await,
                    validate_client_chain_id(
                        &eth_http_rpc_client,
                        expected_chain_id,
                    ).await]
                    .into_iter()
                    .filter_map(|res| res.err())
                    .peekable();

                if errors.peek().is_some() {
                    bail!("Inconsistent chain configuration. Terminating.{}", format_iterator(errors));
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
            scope.spawn(
                eth_multisig_client_backend_future
            );

            // Start eth witnessers
            scope.spawn(
                eth::contract_witnesser::start(
                    stake_manager_contract,
                    eth_ws_rpc_client.clone(),
                    eth_http_rpc_client.clone(),
                    witnessing_instruction_receiver_1,
                    true,
                    state_chain_client.clone(),
                    &root_logger,
                )
            );
            scope.spawn(
                eth::contract_witnesser::start(
                    key_manager_contract,
                    eth_ws_rpc_client,
                    eth_http_rpc_client,
                    witnessing_instruction_receiver_2,
                    false,
                    state_chain_client.clone(),
                    &root_logger,
                )
            );
            scope.spawn(
                eth::chain_data_witnesser::start(
                    eth_dual_rpc,
                    state_chain_client.clone(),
                    witnessing_instruction_receiver_3,
                    cfe_settings_update_receiver,
                    &root_logger
                )
            );

            // Start state chain components
            scope.spawn(state_chain_observer::start(
                state_chain_client.clone(),
                state_chain_block_stream,
                eth_broadcaster,
                eth_multisig_client,
                account_peer_mapping_change_sender,
                witnessing_instruction_sender,
                cfe_settings_update_sender,
                latest_block_hash,
                root_logger.clone()
            ));

            Ok(())
        }.boxed()
    })
}
