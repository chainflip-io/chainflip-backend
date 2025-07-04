// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![feature(ip)]
#![feature(result_flattening)]
#![feature(btree_extract_if)]
#![feature(extract_if)]
#![feature(map_try_insert)]
#![feature(step_trait)]

mod caching_request;
pub mod common;
pub mod constants;
pub mod db;
pub mod elections;
pub mod multisig;
pub mod p2p;
pub mod retrier;
pub mod settings;
pub mod state_chain_observer;
pub mod witness;

// Blockchains
pub mod btc;
pub mod dot;
pub mod evm;
pub mod sol;

use crate::state_chain_observer::client::CreateStateChainClientError;
use ::multisig::{
	bitcoin::BtcSigning, ed25519::SolSigning, eth::EthSigning, polkadot::PolkadotSigning,
};
use cf_primitives::CfeCompatibility;
use state_chain_observer::client::{
	chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	STATE_CHAIN_CONNECTION,
};

use self::{
	btc::retry_rpc::BtcRetryRpcClient,
	db::{KeyStore, PersistentKeyDB},
	dot::{retry_rpc::DotRetryRpcClient, PolkadotHash},
	evm::{retry_rpc::EvmRetryRpcClient, rpc::EvmRpcSigningClient},
	settings::{CommandLineOptions, Settings, DEFAULT_SETTINGS_DIR},
	sol::retry_rpc::SolRetryRpcClient,
};
use anyhow::Context;
use cf_chains::Chain;
use cf_primitives::AccountRole;
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use engine_upgrade_utils::{ExitStatus, ERROR_READING_SETTINGS, NO_START_FROM, SUCCESS};

use crate::btc::cached_rpc::BtcCachingClient;
use cf_utilities::{
	cached_stream::CachedStream, logging::ErrorType, metrics, task_scope::task_scope,
};
use futures::FutureExt;
use std::{
	sync::{atomic::AtomicBool, Arc},
	time::Duration,
};

pub fn settings_and_run_main(
	settings_strings: Vec<String>,
	start_from: state_chain_runtime::BlockNumber,
) -> ExitStatus {
	use_chainflip_account_id_encoding();
	let opts = CommandLineOptions::parse_from(settings_strings);

	let settings = match Settings::new_with_settings_dir(DEFAULT_SETTINGS_DIR, opts)
		.context("Error reading settings")
	{
		Ok(settings) => settings,
		Err(e) => {
			eprintln!("{:#}", e);
			return ExitStatus { status_code: ERROR_READING_SETTINGS, at_block: NO_START_FROM };
		},
	};

	match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            // Note: the greeting should only be printed in normal mode (i.e. not for short-lived
            // commands like `--version`), so we execute it only after the settings have been parsed.
            cf_utilities::print_start_and_end!(async run_main(settings, if start_from == NO_START_FROM { None } else { Some(start_from) }))
        }) {
        Ok(()) => ExitStatus { status_code: SUCCESS, at_block: NO_START_FROM },
        Err(ErrorType::Error(e)) => {
            if let Some(CreateStateChainClientError::CompatibilityError(block_compatibility)) =
                e.downcast_ref::<CreateStateChainClientError>()
            {
                match block_compatibility.compatibility {
                    // we're no longer compatible, so we want to pass on the start to the one that is
                    // now compatible so that it can start from that number, ensuring we don't miss any blocks.
                    CfeCompatibility::NoLongerCompatible => ExitStatus {
                        status_code: engine_upgrade_utils::NO_LONGER_COMPATIBLE,
                        at_block: block_compatibility.at_block.number,
                    },
                    CfeCompatibility::NotYetCompatible => ExitStatus {
                        status_code: engine_upgrade_utils::NOT_YET_COMPATIBLE,
                        at_block: NO_START_FROM,
                    },
                    _ => {
                        unreachable!("We should never get here");
                    },
                }
            } else {
                tracing::error!("Unknown error: {:?}", e);
                ExitStatus { status_code: engine_upgrade_utils::UNKNOWN_ERROR, at_block: NO_START_FROM }
            }
        },
        Err(ErrorType::Panic) =>
            ExitStatus { status_code: engine_upgrade_utils::PANIC, at_block: NO_START_FROM },
    }
}

async fn run_main(
	settings: Settings,
	start_from: Option<state_chain_runtime::BlockNumber>,
) -> anyhow::Result<()> {
	let _guard = cf_utilities::logging::init_json_logger(settings.logging.clone()).await;

	task_scope(|scope| {
		async move {
			let has_completed_initialising = Arc::new(AtomicBool::new(false));

			let (state_chain_stream, unfinalised_state_chain_stream, state_chain_client) =
				state_chain_observer::client::StateChainClient::connect_with_account(
					scope,
					&settings.state_chain.ws_endpoint,
					&settings.state_chain.signing_key_file,
					AccountRole::Validator,
					true,
					true,
					start_from,
				)
				.await?;

			// In case we are upgrading, this gives the old CFE more time to release system
			// resources.
			tokio::time::sleep(Duration::from_secs(4)).await;

			if let Some(health_check_settings) = &settings.health_check {
				cf_utilities::health::start(
					scope,
					health_check_settings,
					has_completed_initialising.clone(),
				)
				.await?;
			}

			if let Some(prometheus_settings) = &settings.prometheus {
				metrics::start(scope, prometheus_settings).await?;
			}

			let db = Arc::new(
				PersistentKeyDB::open_and_migrate_to_latest(
					&settings.signing.db_file,
					Some(state_chain_client.genesis_hash()),
				)
				.context("Failed to open database")?,
			);

			let (
				eth_outgoing_sender,
				eth_incoming_receiver,
				dot_outgoing_sender,
				dot_incoming_receiver,
				btc_outgoing_sender,
				btc_incoming_receiver,
				sol_outgoing_sender,
				sol_incoming_receiver,
				p2p_ready_receiver,
				p2p_fut,
			) = p2p::start(
				state_chain_client.clone(),
				state_chain_stream.clone(),
				settings.node_p2p.clone(),
				state_chain_stream.cache().hash,
			)
			.await
			.context("Failed to start p2p")?;

			scope.spawn(p2p_fut);

			// Use the ceremony id counters from before the initial block so the SCO can process the
			// events from the initial block.
			let ceremony_id_counters = state_chain_observer::get_ceremony_id_counters_before_block(
				state_chain_stream.cache().hash,
				state_chain_client.clone(),
			)
			.await?;

			let (eth_multisig_client, eth_multisig_client_backend_future) =
				multisig::start_client::<EthSigning>(
					state_chain_client.account_id(),
					KeyStore::new(db.clone()),
					eth_incoming_receiver,
					eth_outgoing_sender,
					ceremony_id_counters.ethereum,
				);

			scope.spawn(eth_multisig_client_backend_future);

			let (dot_multisig_client, dot_multisig_client_backend_future) =
				multisig::start_client::<PolkadotSigning>(
					state_chain_client.account_id(),
					KeyStore::new(db.clone()),
					dot_incoming_receiver,
					dot_outgoing_sender,
					ceremony_id_counters.polkadot,
				);

			scope.spawn(dot_multisig_client_backend_future);

			let (btc_multisig_client, btc_multisig_client_backend_future) =
				multisig::start_client::<BtcSigning>(
					state_chain_client.account_id(),
					KeyStore::new(db.clone()),
					btc_incoming_receiver,
					btc_outgoing_sender,
					ceremony_id_counters.bitcoin,
				);

			scope.spawn(btc_multisig_client_backend_future);

			let (sol_multisig_client, sol_multisig_client_backend_future) =
				multisig::start_client::<SolSigning>(
					state_chain_client.account_id(),
					KeyStore::new(db.clone()),
					sol_incoming_receiver,
					sol_outgoing_sender,
					ceremony_id_counters.solana,
				);

			scope.spawn(sol_multisig_client_backend_future);

			// Create all the clients
			let eth_client = {
				let expected_eth_chain_id = web3::types::U256::from(
					state_chain_client
						.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
							state_chain_client.latest_finalized_block().hash,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION),
				);
				EvmRetryRpcClient::<EvmRpcSigningClient>::new(
					scope,
					settings.eth.private_key_file,
					settings.eth.nodes,
					expected_eth_chain_id,
					"eth_rpc",
					"eth_subscribe_client",
					"Ethereum",
					cf_chains::Ethereum::WITNESS_PERIOD,
				)?
			};
			let arb_client = {
				let expected_arb_chain_id = web3::types::U256::from(
					state_chain_client
						.storage_value::<pallet_cf_environment::ArbitrumChainId<state_chain_runtime::Runtime>>(
							state_chain_client.latest_finalized_block().hash,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION),
				);
				EvmRetryRpcClient::<EvmRpcSigningClient>::new(
					scope,
					settings.arb.private_key_file,
					settings.arb.nodes,
					expected_arb_chain_id,
					"arb_rpc",
					"arb_subscribe_client",
					"Arbitrum",
					cf_chains::Arbitrum::WITNESS_PERIOD,
				)?
			};

			let btc_client = {
				let btc_client = {
					let expected_btc_network = cf_chains::btc::BitcoinNetwork::from(
						state_chain_client
							.storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<
								state_chain_runtime::Runtime,
							>>(state_chain_client.latest_finalized_block().hash)
							.await
							.expect(STATE_CHAIN_CONNECTION),
					);
					BtcRetryRpcClient::new(scope, settings.btc.nodes, expected_btc_network).await?
				};
				BtcCachingClient::new(scope, btc_client)
			};
			let dot_client = {
				let expected_dot_genesis_hash = PolkadotHash::from_slice(
					state_chain_client
						.storage_value::<pallet_cf_environment::PolkadotGenesisHash<state_chain_runtime::Runtime>>(
							state_chain_client.latest_finalized_block().hash,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION)
						.as_bytes(),
				);
				DotRetryRpcClient::new(scope, settings.dot.nodes, expected_dot_genesis_hash)?
			};

			let sol_client = {
				let expected_sol_genesis_hash = state_chain_client
					.storage_value::<pallet_cf_environment::SolanaGenesisHash<state_chain_runtime::Runtime>>(
						state_chain_client.latest_finalized_block().hash,
					)
					.await
					.expect(STATE_CHAIN_CONNECTION);

				SolRetryRpcClient::new(
					scope,
					settings.sol.nodes,
					expected_sol_genesis_hash,
					cf_chains::Solana::WITNESS_PERIOD,
				)
				.await?
			};

			let hub_client = {
				let expected_hub_genesis_hash = PolkadotHash::from_slice(
					state_chain_client
						.storage_value::<pallet_cf_environment::AssethubGenesisHash<state_chain_runtime::Runtime>>(
							state_chain_client.latest_finalized_block().hash,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION)
						.as_bytes(),
				);
				DotRetryRpcClient::new(scope, settings.hub.nodes, expected_hub_genesis_hash)?
			};

			witness::start::start(
				scope,
				eth_client.clone(),
				arb_client.clone(),
				btc_client.clone(),
				dot_client.clone(),
				sol_client.clone(),
				hub_client.clone(),
				state_chain_client.clone(),
				state_chain_stream.clone(),
				unfinalised_state_chain_stream.clone(),
				db.clone(),
			)
			.await?;

			scope.spawn(state_chain_observer::start(
				state_chain_client.clone(),
				state_chain_stream,
				eth_client,
				arb_client,
				dot_client,
				btc_client,
				sol_client,
				hub_client,
				eth_multisig_client,
				dot_multisig_client,
				btc_multisig_client,
				sol_multisig_client,
			));

			p2p_ready_receiver.await.unwrap();

			has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);

			tracing::info!("Engine finished initialising");

			Ok(())
		}
		.boxed()
	})
	.await
}
