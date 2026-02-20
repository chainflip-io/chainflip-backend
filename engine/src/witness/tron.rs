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

mod tron_deposits;

pub use tron_deposits::*;

// use std::{collections::HashMap, sync::Arc};

// use cf_chains::{
// 	address::EncodedAddress,
// 	cf_parameters::VaultSwapParametersV1,
// 	evm::{DepositDetails, H256},
// 	Tron, CcmDepositMetadataUnchecked, ForeignChainAddress,
// };
// use cf_primitives::{chains::assets::tron::Asset as TronAsset, Asset, AssetAmount, EpochIndex};
// use cf_utilities::task_scope::Scope;
// use futures_core::Future;
// use itertools::Itertools;
// use pallet_cf_ingress_egress::VaultDepositWitness;
// use sp_core::H160;

// use crate::{
// 	db::PersistentKeyDB,
// 	tron::{retry_rpc::TronRetryRpcClient, rpc::TronRpcSigningClient},
// 	witness::evm::erc20_deposits::{usdc::UsdcEvents, usdt::UsdtEvents},
// };

// use engine_sc_client::{
// 	chain_api::ChainApi,
// 	extrinsic_api::signed::SignedExtrinsicApi,
// 	storage_api::StorageApi,
// 	stream_api::{StreamApi, FINALIZED},
// 	STATE_CHAIN_CONNECTION,
// };

// use super::{
// 	common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
// 	evm::{source::EvmSource, vault::vault_deposit_witness},
// };

// use chainflip_node::chain_spec::berghain::TRON_SAFETY_MARGIN;

// pub async fn start<StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
// 	scope: &Scope<'_, anyhow::Error>,
// 	tron_client: TronRetryRpcClient<TronRpcSigningClient>,
// 	process_call: ProcessCall,
// 	state_chain_client: Arc<StateChainClient>,
// 	state_chain_stream: StateChainStream,
// 	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
// 	db: Arc<PersistentKeyDB>,
// ) -> Result<()>
// where
// 	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
// 	StateChainStream: StreamApi<FINALIZED> + Clone,
// 	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
// 		+ Send
// 		+ Sync
// 		+ Clone
// 		+ 'static,
// 	ProcessingFut: Future<Output = ()> + Send + 'static,
// {
// 	let key_manager_address = state_chain_client
// 		.storage_value::<pallet_cf_environment::TronKeyManagerAddress<state_chain_runtime::Runtime>>(
// 			state_chain_client.latest_finalized_block().hash,
// 		)
// 		.await
// 		.context("Failed to get KeyManager address from SC")?;

// 	let vault_address = state_chain_client
// 		.storage_value::<pallet_cf_environment::TronVaultAddress<state_chain_runtime::Runtime>>(
// 			state_chain_client.latest_finalized_block().hash,
// 		)
// 		.await
// 		.context("Failed to get Vault contract address from SC")?;


// 	let supported_tron_erc20_assets: HashMap<TronAsset, H160> = state_chain_client
// 		.storage_map::<pallet_cf_environment::TronSupportedAssets<state_chain_runtime::Runtime>, _>(
// 			state_chain_client.latest_finalized_block().hash,
// 		)
// 		.await
// 		.context("Failed to fetch Tron supported assets")?;

// 	let usdt_contract_address = *supported_tron_erc20_assets
// 		.get(&TronAsset::TronUsdt)
// 		.context("TronSupportedAssets does not include USDT")?;

// 	let supported_tron_erc20_assets: HashMap<H160, Asset> = supported_tron_erc20_assets
// 		.into_iter()
// 		.map(|(asset, address)| (address, asset.into()))
// 		.collect();

// 	let tron_source = EvmSource::<_, Tron>::new(tron_client.clone())
// 		.strictly_monotonic()
// 		.shared(scope);

// 	// tron_source
// 	// 	.clone()
// 	// 	.chunk_by_time(epoch_source.clone(), scope)
// 	// 	.chain_tracking(state_chain_client.clone(), tron_client.clone())
// 	// 	.logging("chain tracking")
// 	// 	.spawn(scope);

// 	let vaults = epoch_source.vaults::<Tron>().await;

// 	// ===== Full witnessing stream =====

// 	let tron_safety_margin = state_chain_client
// 		.storage_value::<pallet_cf_ingress_egress::WitnessSafetyMargin<
// 			state_chain_runtime::Runtime,
// 			state_chain_runtime::TronInstance,
// 		>>(state_chain_stream.cache().hash)
// 		.await?
// 		// Default to berghain in case the value is missing (e.g. during initial upgrade)
// 		.unwrap_or(TRON_SAFETY_MARGIN);

// 	tracing::info!("Safety margin for Tron is set to {tron_safety_margin} blocks.",);

// 	let tron_safe_vault_source = tron_source
// 		.lag_safety(tron_safety_margin)
// 		.logging("safe block produced")
// 		.chunk_by_vault(vaults, scope);

// 	let tron_safe_vault_source_deposit_addresses = tron_safe_vault_source
// 		.clone()
// 		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
// 		.await;

// 	tron_safe_vault_source
// 		.clone()
// 		.key_manager_witnessing(process_call.clone(), tron_client.clone(), key_manager_address)
// 		.continuous("TronKeyManager".to_string(), db.clone())
// 		.logging("KeyManager")
// 		.spawn(scope);

//     // TODO: To update with the right witnessing
// 	// tron_safe_vault_source_deposit_addresses
// 	// 	.clone()
// 	// 	.erc20_deposits::<_, _, _, UsdcEvents>(
// 	// 		process_call.clone(),
// 	// 		tron_client.clone(),
// 	// 		TronAsset::TronUsdc,
// 	// 		usdc_contract_address,
// 	// 	)
// 	// 	.await?
// 	// 	.continuous("TronUSDCDeposits".to_string(), db.clone())
// 	// 	.logging("USDCDeposits")
// 	// 	.spawn(scope);

// 	// tron_safe_vault_source_deposit_addresses
// 	// 	.clone()
// 	// 	.erc20_deposits::<_, _, _, UsdtEvents>(
// 	// 		process_call.clone(),
// 	// 		tron_client.clone(),
// 	// 		TronAsset::TronUsdt,
// 	// 		usdt_contract_address,
// 	// 	)
// 	// 	.await?
// 	// 	.continuous("TronUSDTDeposits".to_string(), db.clone())
// 	// 	.logging("USDTDeposits")
// 	// 	.spawn(scope);

// 	// tron_safe_vault_source_deposit_addresses
// 	// 	.clone()
// 	// 	.ethereum_deposits(
// 	// 		process_call.clone(),
// 	// 		tron_client.clone(),
// 	// 		TronAsset::TronEth,
// 	// 		address_checker_address,
// 	// 		vault_address,
// 	// 	)
// 	// 	.await
// 	// 	.continuous("TronDeposits".to_string(), db.clone())
// 	// 	.logging("Deposits")
// 	// 	.spawn(scope);

// 	// tron_safe_vault_source
// 	// 	.vault_witnessing::<TronCallBuilder, _, _, _>(
// 	// 		process_call,
// 	// 		tron_client.clone(),
// 	// 		vault_address,
// 	// 		Asset::TronEth,
// 	// 		cf_primitives::ForeignChain::Tron,
// 	// 		supported_tron_erc20_assets,
// 	// 	)
// 	// 	.continuous("TronVault".to_string(), db)
// 	// 	.logging("Vault")
// 	// 	.spawn(scope);

// 	Ok(())
// }

// pub struct TronCallBuilder {}

// impl super::evm::vault::IngressCallBuilder for TronCallBuilder {
// 	type Chain = Tron;

// 	fn vault_swap_request(
// 		block_height: u64,
// 		source_asset: Asset,
// 		deposit_amount: AssetAmount,
// 		destination_asset: Asset,
// 		destination_address: EncodedAddress,
// 		deposit_metadata: Option<CcmDepositMetadataUnchecked<ForeignChainAddress>>,
// 		tx_id: H256,
// 		vault_swap_parameters: VaultSwapParametersV1<
// 			<Self::Chain as cf_chains::Chain>::ChainAccount,
// 		>,
// 	) -> state_chain_runtime::RuntimeCall {
// 		let deposit = vault_deposit_witness!(
// 			source_asset,
// 			deposit_amount,
// 			destination_asset,
// 			destination_address,
// 			deposit_metadata,
// 			tx_id,
// 			vault_swap_parameters
// 		);

// 		state_chain_runtime::RuntimeCall::TronIngressEgress(
// 			pallet_cf_ingress_egress::Call::vault_swap_request {
// 				block_height,
// 				deposit: Box::new(deposit),
// 			},
// 		)
// 	}

// 	fn vault_transfer_failed(
// 		asset: <Self::Chain as cf_chains::Chain>::ChainAsset,
// 		amount: <Self::Chain as cf_chains::Chain>::ChainAmount,
// 		destination_address: <Self::Chain as cf_chains::Chain>::ChainAccount,
// 	) -> state_chain_runtime::RuntimeCall {
// 		state_chain_runtime::RuntimeCall::TronIngressEgress(
// 			pallet_cf_ingress_egress::Call::vault_transfer_failed {
// 				asset,
// 				amount,
// 				destination_address,
// 			},
// 		)
// 	}
// }

// #[cfg(test)]
// mod tests {

// 	use std::path::PathBuf;

// 	use cf_chains::{Tron, Chain};
// 	use cf_primitives::AccountRole;

// 	use crate::{
// 		settings::{NodeContainer, WsHttpEndpoints},
// 		witness::common::epoch_source::EpochSource,
// 	};

// 	use cf_utilities::{
// 		logging::LoggingSettings, task_scope::task_scope,
// 		testing::new_temp_directory_with_nonexistent_file,
// 	};
// 	use futures::FutureExt;

// 	use super::*;

// 	#[ignore = "requires a running localnet"]
// 	#[tokio::test]
// 	async fn run_tron_witnessing() {
// 		let _start_logger_server_fn = Some(
// 			cf_utilities::logging::init_json_logger(LoggingSettings {
// 				span_lifecycle: false,
// 				command_server_port: 6666,
// 			})
// 			.await,
// 		);

// 		task_scope(|scope| {
// 			async move {
// 				let (state_chain_stream, _unfinalised_state_chain_stream, state_chain_client) =
// 					engine_sc_client::StateChainClient::connect_with_account(
// 						scope,
// 						"ws://localhost:9944",
// 						PathBuf::from("/Users/kylezs/Documents/cf-repos/chainflip-backend/localnet/init/keys/bashful/signing_key_file").as_path(),
// 						AccountRole::Validator,
// 						false,
// 						false,
// 						None,
// 					)
// 					.await.unwrap();

// 				let witness_call = {
// 					move |call, epoch_index| async move {
// 						println!("Witnessing epoch index {epoch_index} call: {call:?}");
// 					}
// 				};

// 				let epoch_source =
// 					EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone())
// 						.await
// 						.participating(state_chain_client.account_id())
// 						.await;

// 				let tron_client = {
// 					let expected_tron_chain_id = web3::types::U256::from(
// 						state_chain_client
// 							.storage_value::<pallet_cf_environment::TronChainId<state_chain_runtime::Runtime>>(
// 								state_chain_client.latest_finalized_block().hash,
// 							)
// 							.await
// 							.expect(STATE_CHAIN_CONNECTION),
// 					);

// 					TronRetryRpcClient::<TronRpcSigningClient>::new(
// 						scope,
// 						PathBuf::from("/Users/kylezs/Documents/cf-repos/chainflip-backend/localnet/init/keys/bashful/eth_private_key_file"),
// 						NodeContainer { primary: WsHttpEndpoints { ws_endpoint: "ws://localhost:8548".into(), http_endpoint: "http://localhost:8547".into()}, backup: None },
// 						expected_tron_chain_id,
// 						"tron_rpc",
// 						"tron_subscribe_client",
// 						"Tron",
// 						Tron::WITNESS_PERIOD,
// 					).unwrap()
// 				};

// 				let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
// 				let db = Arc::new(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap());


// 				start(scope, tron_client, witness_call, state_chain_client, state_chain_stream, epoch_source, db).await.unwrap();

// 				Ok(())
// 			}
// 			.boxed()
// 		})
// 		.await.unwrap();
// 	}
// }
