mod chain_tracking;

use std::{collections::HashMap, sync::Arc};

use cf_chains::Arbitrum;
use cf_primitives::EpochIndex;
use cf_utilities::task_scope::Scope;
use futures_core::Future;
use sp_core::H160;

use crate::{
	db::PersistentKeyDB,
	evm::{retry_rpc::EvmRetryRpcClient, rpc::EvmRpcSigningClient},
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
		STATE_CHAIN_CONNECTION,
	},
	witness::evm::erc20_deposits::usdc::UsdcEvents,
};

use super::{
	common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
	evm::source::EvmSource,
};

use anyhow::{Context, Result};

use chainflip_node::chain_spec::berghain::ARBITRUM_SAFETY_MARGIN;

pub async fn start<StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	arb_client: EvmRetryRpcClient<EvmRpcSigningClient>,
	process_call: ProcessCall,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StreamApi<FINALIZED> + Clone,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let supported_arb_erc20_assets: HashMap<cf_primitives::chains::assets::arb::Asset, H160> =
		state_chain_client
			.storage_map::<pallet_cf_environment::ArbitrumSupportedAssets<state_chain_runtime::Runtime>, _>(
				state_chain_client.latest_finalized_block().hash,
			)
			.await
			.context("Failed to fetch Arbitrum supported assets")?;

	let usdc_contract_address = *supported_arb_erc20_assets
		.get(&cf_primitives::chains::assets::arb::Asset::ArbUsdc)
		.context("ArbitrumSupportedAssets does not include USDC")?;

	let supported_arb_erc20_assets: HashMap<H160, cf_primitives::Asset> =
		supported_arb_erc20_assets
			.into_iter()
			.map(|(asset, address)| (address, asset.into()))
			.collect();

	let arb_source = EvmSource::<_, Arbitrum>::new(arb_client.clone())
		.strictly_monotonic()
		.shared(scope);

	arb_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
		.chain_tracking(state_chain_client.clone(), arb_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let vaults = epoch_source.vaults::<Arbitrum>().await;

	// ===== Full witnessing stream =====

	let arb_safety_margin = state_chain_client
		.storage_value::<pallet_cf_ingress_egress::WitnessSafetyMargin<
			state_chain_runtime::Runtime,
			state_chain_runtime::ArbitrumInstance,
		>>(state_chain_stream.cache().hash)
		.await?
		// Default to berghain in case the value is missing (e.g. during initial upgrade)
		.unwrap_or(ARBITRUM_SAFETY_MARGIN);

	tracing::info!("Safety margin for Arbitrum is set to {arb_safety_margin} blocks.",);

	let arb_safe_vault_source = arb_source
		.lag_safety(arb_safety_margin)
		.logging("safe block produced")
		.chunk_by_vault(vaults, scope);

	let arb_safe_vault_source_deposit_addresses = arb_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await;

	arb_safe_vault_source
		.clone()
		.key_manager_witnessing(process_call.clone(), arb_client.clone(), key_manager_address)
		.continuous("ArbitrumKeyManager".to_string(), db.clone())
		.logging("KeyManager")
		.spawn(scope);

	arb_safe_vault_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdcEvents>(
			process_call.clone(),
			arb_client.clone(),
			cf_primitives::chains::assets::arb::Asset::ArbUsdc,
			usdc_contract_address,
		)
		.await?
		.continuous("ArbitrumUSDCDeposits".to_string(), db.clone())
		.logging("USDCDeposits")
		.spawn(scope);

	arb_safe_vault_source_deposit_addresses
		.clone()
		.ethereum_deposits(
			process_call.clone(),
			arb_client.clone(),
			cf_primitives::chains::assets::arb::Asset::ArbEth,
			address_checker_address,
			vault_address,
		)
		.await
		.continuous("ArbitrumDeposits".to_string(), db.clone())
		.logging("Deposits")
		.spawn(scope);

	arb_safe_vault_source
		.vault_witnessing::<ArbCallBuilder, _, _, _>(
			process_call,
			arb_client.clone(),
			vault_address,
			cf_primitives::Asset::ArbEth,
			cf_primitives::ForeignChain::Arbitrum,
			supported_arb_erc20_assets,
		)
		.continuous("ArbitrumVault".to_string(), db)
		.logging("Vault")
		.spawn(scope);

	Ok(())
}

struct ArbCallBuilder {}

use cf_chains::{address::EncodedAddress, CcmDepositMetadata};
use cf_primitives::{Asset, AssetAmount, TransactionHash};

impl super::evm::vault::IngressCallBuilder for ArbCallBuilder {
	type Chain = Arbitrum;

	fn contract_swap_request(
		source_asset: Asset,
		deposit_amount: AssetAmount,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		deposit_metadata: Option<CcmDepositMetadata>,
		tx_hash: TransactionHash,
	) -> state_chain_runtime::RuntimeCall {
		state_chain_runtime::RuntimeCall::ArbitrumIngressEgress(
			if let Some(deposit_metadata) = deposit_metadata {
				pallet_cf_ingress_egress::Call::contract_ccm_swap_request {
					source_asset,
					destination_asset,
					deposit_amount,
					destination_address,
					deposit_metadata,
					tx_hash,
				}
			} else {
				pallet_cf_ingress_egress::Call::contract_swap_request {
					from: source_asset,
					to: destination_asset,
					deposit_amount,
					destination_address,
					tx_hash,
				}
			},
		)
	}

	fn vault_transfer_failed(
		asset: <Self::Chain as cf_chains::Chain>::ChainAsset,
		amount: <Self::Chain as cf_chains::Chain>::ChainAmount,
		destination_address: <Self::Chain as cf_chains::Chain>::ChainAccount,
	) -> state_chain_runtime::RuntimeCall {
		state_chain_runtime::RuntimeCall::ArbitrumIngressEgress(
			pallet_cf_ingress_egress::Call::vault_transfer_failed {
				asset,
				amount,
				destination_address,
			},
		)
	}
}

#[cfg(test)]
mod tests {

	use std::path::PathBuf;

	use cf_chains::{Arbitrum, Chain};
	use cf_primitives::AccountRole;

	use crate::{
		settings::{NodeContainer, WsHttpEndpoints},
		state_chain_observer,
		witness::common::epoch_source::EpochSource,
	};

	use cf_utilities::{
		logging::LoggingSettings, task_scope::task_scope,
		testing::new_temp_directory_with_nonexistent_file,
	};
	use futures::FutureExt;

	use super::*;

	#[ignore = "requires a running localnet"]
	#[tokio::test]
	async fn run_arb_witnessing() {
		let _start_logger_server_fn = Some(
			cf_utilities::logging::init_json_logger(LoggingSettings {
				span_lifecycle: false,
				command_server_port: 6666,
			})
			.await,
		);

		task_scope(|scope| {
			async move {
				let (state_chain_stream, _unfinalised_state_chain_stream, state_chain_client) =
					state_chain_observer::client::StateChainClient::connect_with_account(
						scope,
						"ws://localhost:9944",
						PathBuf::from("/Users/kylezs/Documents/cf-repos/chainflip-backend/localnet/init/keys/bashful/signing_key_file").as_path(),
						AccountRole::Validator,
						false,
						false,
						None,
					)
					.await.unwrap();

				let witness_call = {
					move |call, epoch_index| async move {
						println!("Witnessing epoch index {epoch_index} call: {call:?}");
					}
				};

				let epoch_source =
					EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone())
						.await
						.participating(state_chain_client.account_id())
						.await;

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
						PathBuf::from("/Users/kylezs/Documents/cf-repos/chainflip-backend/localnet/init/keys/bashful/eth_private_key_file"),
						NodeContainer { primary: WsHttpEndpoints { ws_endpoint: "ws://localhost:8548".into(), http_endpoint: "http://localhost:8547".into()}, backup: None },
						expected_arb_chain_id,
						"arb_rpc",
						"arb_subscribe",
						"Arbitrum",
						Arbitrum::WITNESS_PERIOD,
					).unwrap()
				};

				let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
				let db = Arc::new(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap());


				start(scope, arb_client, witness_call, state_chain_client, state_chain_stream, epoch_source, db).await.unwrap();

				Ok(())
			}
			.boxed()
		})
		.await.unwrap();
	}
}
