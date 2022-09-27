use std::sync::Arc;

use crate::multisig::eth::EthSigning;
use anyhow::Context;

use chainflip_engine::{
    eth::{
        self, build_broadcast_channel, key_manager::KeyManager, rpc::EthDualRpcClient,
        stake_manager::StakeManager, EthBroadcaster,
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

            let (latest_block_hash, state_chain_block_stream, state_chain_client) =
                state_chain_observer::client::connect_to_state_chain(&settings.state_chain, true, &root_logger)
                    .await?;

            let eth_dual_rpc =
                EthDualRpcClient::new(&settings.eth, U256::from(state_chain_client
                    .get_storage_value::<pallet_cf_environment::EthereumChainId::<state_chain_runtime::Runtime>>(
                        latest_block_hash,
                    )
                    .await
                    .context("Failed to get EthereumChainId from state chain")?
                ),
                &root_logger)
                .await
                .context("Failed to create EthDualRpcClient")?;

            let eth_broadcaster = EthBroadcaster::new(&settings.eth, eth_dual_rpc.clone(), &root_logger)
                .context("Failed to create ETH broadcaster")?;

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
                epoch_start_sender,
                [epoch_start_receiver_1, epoch_start_receiver_2, epoch_start_receiver_3, _epoch_start_receiver_4, _epoch_start_receiver_5, _epoch_start_receiver_6]
            ) = build_broadcast_channel(10);

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
                    eth_dual_rpc.clone(),
                    epoch_start_receiver_1,
                    true,
                    state_chain_client.clone(),
                    &root_logger,
                )
            );
            scope.spawn(
                eth::contract_witnesser::start(
                    key_manager_contract,
                    eth_dual_rpc.clone(),
                    epoch_start_receiver_2,
                    false,
                    state_chain_client.clone(),
                    &root_logger,
                )
            );
            scope.spawn(
                eth::chain_data_witnesser::start(
                    eth_dual_rpc.clone(),
                    state_chain_client.clone(),
                    epoch_start_receiver_3,
                    cfe_settings_update_receiver,
                    &root_logger
                )
            );

            #[cfg(feature = "ibiza")]
            let (eth_monitor_ingress_sender, eth_monitor_ingress_receiver) = tokio::sync::mpsc::unbounded_channel();
            #[cfg(feature = "ibiza")]
            let (eth_monitor_flip_ingress_sender, eth_monitor_flip_ingress_receiver) = tokio::sync::mpsc::unbounded_channel();
            #[cfg(feature = "ibiza")]
            let (eth_monitor_usdc_ingress_sender, eth_monitor_usdc_ingress_receiver) = tokio::sync::mpsc::unbounded_channel();

            // Start state chain components
            scope.spawn(state_chain_observer::start(
                state_chain_client.clone(),
                state_chain_block_stream,
                eth_broadcaster,
                eth_multisig_client,
                account_peer_mapping_change_sender,
                epoch_start_sender,
                #[cfg(feature = "ibiza")] eth_monitor_ingress_sender,
                #[cfg(feature = "ibiza")] eth_monitor_flip_ingress_sender,
                #[cfg(feature = "ibiza")] eth_monitor_usdc_ingress_sender,
                cfe_settings_update_sender,
                latest_block_hash,
                root_logger.clone()
            ));

            #[cfg(feature = "ibiza")]
            {

                use std::collections::{BTreeSet, HashMap};
                use itertools::Itertools;
                use sp_core::H160;
                use chainflip_engine::eth::erc20_witnesser::Erc20Witnesser;
                use cf_primitives::{ForeignChain, ForeignChainAddress};
                use cf_primitives::Asset;

                let flip_contract_address = state_chain_client
                    .get_storage_map::<pallet_cf_environment::SupportedEthAssets::<
                        state_chain_runtime::Runtime,
                    >>(latest_block_hash, &Asset::Flip)
                    .await
                    .context("Failed to get FLIP address from SC")?
                    .expect("FLIP address must exist at genesis");

                let usdc_contract_address = state_chain_client
                    .get_storage_map::<pallet_cf_environment::SupportedEthAssets::<
                        state_chain_runtime::Runtime,
                    >>(latest_block_hash, &Asset::Usdc)
                    .await
                    .context("Failed to get USDC address from SC")?
                    .expect("USDC address must exist at genesis");

                let eth_chain_ingress_addresses = state_chain_client.get_all_storage_pairs::<pallet_cf_ingress::IntentIngressDetails<state_chain_runtime::Runtime>>(latest_block_hash)
                    .await
                    .context("Failed to get initial ingress details")?
                    .into_iter()
                    .filter_map(|(foreign_chain_address, intent)| {
                        if let ForeignChainAddress::Eth(address) = foreign_chain_address {
                            assert_eq!(intent.ingress_asset.chain, ForeignChain::Ethereum);
                            Some((intent.ingress_asset.asset, H160::from(address)))
                        } else {
                            None
                    }}).into_group_map();

                fn monitored_addresses_from_all_eth(eth_chain_ingress_addresses: &HashMap<Asset, Vec<H160>>, asset: Asset) -> BTreeSet<H160> {
                    eth_chain_ingress_addresses.get(&asset).expect("State Chain must contain these asset addresses at genesis").iter().cloned().collect()
                }

                scope.spawn(eth::ingress_witnesser::start(
                    eth_dual_rpc.clone(),
                    _epoch_start_receiver_4,
                    eth_monitor_ingress_receiver,
                    state_chain_client.clone(),
                    monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, Asset::Eth),
                    &root_logger
                ));
                scope.spawn(
                    eth::contract_witnesser::start(
                        Erc20Witnesser::new(flip_contract_address.into(), Asset::Flip, monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, Asset::Flip), eth_monitor_flip_ingress_receiver),
                        eth_dual_rpc.clone(),
                        _epoch_start_receiver_5,
                        false,
                        state_chain_client.clone(),
                        &root_logger,
                    )
                );
                scope.spawn(
                    eth::contract_witnesser::start(
                        Erc20Witnesser::new(usdc_contract_address.into(), Asset::Usdc, monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, Asset::Usdc), eth_monitor_usdc_ingress_receiver),
                        eth_dual_rpc,
                        _epoch_start_receiver_6,
                        false,
                        state_chain_client,
                        &root_logger,
                    )
                );
            }

            Ok(())
        }.boxed()
    })
}
