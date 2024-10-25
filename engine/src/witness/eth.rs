mod chain_tracking;
mod state_chain_gateway;

use std::{collections::HashMap, sync::Arc};

use cf_chains::{evm::DepositDetails, Ethereum};
use cf_primitives::{chains::assets::eth::Asset as EthAsset, EpochIndex};
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
	witness::evm::erc20_deposits::{flip::FlipEvents, usdc::UsdcEvents, usdt::UsdtEvents},
};

use super::{common::epoch_source::EpochSourceBuilder, evm::source::EvmSource};
use crate::witness::common::chain_source::extension::ChainSourceExt;

use anyhow::{Context, Result};

use chainflip_node::chain_spec::berghain::ETHEREUM_SAFETY_MARGIN;

pub async fn start<StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	eth_client: EvmRetryRpcClient<EvmRpcSigningClient>,
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
	let state_chain_gateway_address = state_chain_client
        .storage_value::<pallet_cf_environment::EthereumStateChainGatewayAddress<state_chain_runtime::Runtime>>(
            state_chain_client.latest_finalized_block().hash,
        )
        .await
        .context("Failed to get StateChainGateway address from SC")?;

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let supported_erc20_tokens: HashMap<EthAsset, H160> = state_chain_client
		.storage_map::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>, _>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to fetch Ethereum supported assets")?;

	let usdc_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Usdc).context("USDC not supported")?;

	let flip_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Flip).context("FLIP not supported")?;

	let usdt_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Usdt).context("USDT not supported")?;

	let supported_erc20_tokens: HashMap<H160, cf_primitives::Asset> = supported_erc20_tokens
		.into_iter()
		.map(|(asset, address)| (address, asset.into()))
		.collect();

	let eth_source = EvmSource::new(eth_client.clone()).strictly_monotonic().shared(scope);

	eth_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
		.chain_tracking(state_chain_client.clone(), eth_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let vaults = epoch_source.vaults::<Ethereum>().await;

	// ===== Full witnessing stream =====

	let eth_safety_margin = state_chain_client
		.storage_value::<pallet_cf_ingress_egress::WitnessSafetyMargin<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>(state_chain_stream.cache().hash)
		.await?
		// Default to berghain in case the value is missing (e.g. during initial upgrade)
		.unwrap_or(ETHEREUM_SAFETY_MARGIN);

	tracing::info!("Safety margin for Ethereum is set to {eth_safety_margin} blocks.",);

	let eth_safe_vault_source = eth_source
		.lag_safety(eth_safety_margin)
		.logging("safe block produced")
		.chunk_by_vault(vaults, scope);

	let eth_safe_vault_source_deposit_addresses = eth_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await;

	eth_safe_vault_source
		.clone()
		.key_manager_witnessing(process_call.clone(), eth_client.clone(), key_manager_address)
		.continuous("KeyManager".to_string(), db.clone())
		.logging("KeyManager")
		.spawn(scope);

	eth_safe_vault_source
		.clone()
		.state_chain_gateway_witnessing(
			process_call.clone(),
			eth_client.clone(),
			state_chain_gateway_address,
		)
		.continuous("StateChainGateway".to_string(), db.clone())
		.logging("StateChainGateway")
		.spawn(scope);

	eth_safe_vault_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdcEvents>(
			process_call.clone(),
			eth_client.clone(),
			EthAsset::Usdc,
			usdc_contract_address,
		)
		.await?
		.continuous("USDCDeposits".to_string(), db.clone())
		.logging("USDCDeposits")
		.spawn(scope);

	eth_safe_vault_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, FlipEvents>(
			process_call.clone(),
			eth_client.clone(),
			EthAsset::Flip,
			flip_contract_address,
		)
		.await?
		.continuous("FlipDeposits".to_string(), db.clone())
		.logging("FlipDeposits")
		.spawn(scope);

	eth_safe_vault_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdtEvents>(
			process_call.clone(),
			eth_client.clone(),
			EthAsset::Usdt,
			usdt_contract_address,
		)
		.await?
		.continuous("USDTDeposits".to_string(), db.clone())
		.logging("USDTDeposits")
		.spawn(scope);

	eth_safe_vault_source_deposit_addresses
		.clone()
		.ethereum_deposits(
			process_call.clone(),
			eth_client.clone(),
			EthAsset::Eth,
			address_checker_address,
			vault_address,
		)
		.await
		.continuous("EthereumDeposits".to_string(), db.clone())
		.logging("EthereumDeposits")
		.spawn(scope);

	eth_safe_vault_source
		.vault_witnessing::<EthCallBuilder, _, _, _>(
			process_call,
			eth_client.clone(),
			vault_address,
			cf_primitives::Asset::Eth,
			cf_primitives::ForeignChain::Ethereum,
			supported_erc20_tokens,
		)
		.continuous("Vault".to_string(), db)
		.logging("Vault")
		.spawn(scope);

	Ok(())
}

use cf_chains::{address::EncodedAddress, CcmDepositMetadata};
use cf_primitives::{Asset, AssetAmount, TransactionHash};

pub struct EthCallBuilder {}

impl super::evm::vault::IngressCallBuilder for EthCallBuilder {
	type Chain = Ethereum;

	fn vault_swap_request(
		source_asset: Asset,
		deposit_amount: AssetAmount,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		deposit_metadata: Option<CcmDepositMetadata>,
		tx_hash: TransactionHash,
	) -> state_chain_runtime::RuntimeCall {
		state_chain_runtime::RuntimeCall::EthereumIngressEgress(
			pallet_cf_ingress_egress::Call::vault_swap_request {
				input_asset: source_asset.try_into().expect("invalid asset for chain"),
				output_asset: destination_asset,
				deposit_amount,
				destination_address,
				deposit_metadata,
				tx_hash,
				deposit_details: Box::new(DepositDetails { tx_hashes: Some(vec![tx_hash.into()]) }),
				broker_fees: Default::default(),
				// TODO: use real parameters when we can decode them
				boost_fee: 0,
				dca_params: None,
				refund_params: None,
			},
		)
	}

	fn vault_transfer_failed(
		asset: <Self::Chain as cf_chains::Chain>::ChainAsset,
		amount: <Self::Chain as cf_chains::Chain>::ChainAmount,
		destination_address: <Self::Chain as cf_chains::Chain>::ChainAccount,
	) -> state_chain_runtime::RuntimeCall {
		state_chain_runtime::RuntimeCall::EthereumIngressEgress(
			pallet_cf_ingress_egress::Call::vault_transfer_failed {
				asset,
				amount,
				destination_address,
			},
		)
	}
}
