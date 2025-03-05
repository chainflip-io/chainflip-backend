use cf_chains::{Arbitrum, Chain};
use cf_utilities::task_scope;
use chainflip_api::primitives::{
	chains::assets::arb::Asset as ArbAsset, Asset, EpochIndex, ForeignChain,
};
use std::sync::Arc;

use chainflip_engine::{
	evm::{retry_rpc::EvmRetryRpcClient, rpc::EvmRpcClient},
	settings::NodeContainer,
	state_chain_observer::client::{
		stream_api::{StreamApi, UNFINALIZED},
		StateChainClient,
	},
	witness::{
		arb::ArbCallBuilder,
		common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
		evm::{erc20_deposits::usdc::UsdcEvents, source::EvmSource},
	},
};

use crate::DepositTrackerSettings;

use super::EnvironmentParameters;

pub(super) async fn start<ProcessCall, ProcessingFut>(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	state_chain_client: Arc<StateChainClient<()>>,
	state_chain_stream: impl StreamApi<UNFINALIZED> + Clone,
	settings: DepositTrackerSettings,
	env_params: EnvironmentParameters,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient<()>, (), ()>,
	witness_call: ProcessCall,
) -> anyhow::Result<()>
where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: futures::Future<Output = ()> + Send + 'static,
{
	let arb_client = {
		let nodes = NodeContainer { primary: settings.arb.clone(), backup: None };

		EvmRetryRpcClient::<EvmRpcClient>::new(
			scope,
			nodes,
			env_params.arb_chain_id.into(),
			"arb_rpc",
			"arb_subscribe",
			"Arbitrum",
			Arbitrum::WITNESS_PERIOD,
		)?
	};

	let vaults = epoch_source.vaults::<Arbitrum>().await;
	let arb_source = EvmSource::<_, Arbitrum>::new(arb_client.clone())
		.strictly_monotonic()
		.chunk_by_vault(vaults, scope);

	let arb_source_deposit_addresses = arb_source
		.clone()
		.deposit_addresses(scope, state_chain_stream, state_chain_client.clone())
		.await;

	arb_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdcEvents>(
			witness_call.clone(),
			arb_client.clone(),
			ArbAsset::ArbUsdc,
			env_params.arb_usdc_contract_address,
		)
		.await?
		.logging("witnessing USDCDeposits")
		.spawn(scope);

	arb_source_deposit_addresses
		.clone()
		.ethereum_deposits(
			witness_call.clone(),
			arb_client.clone(),
			ArbAsset::ArbEth,
			env_params.arb_address_checker_address,
			env_params.arb_vault_address,
		)
		.await
		.logging("witnessing EthereumDeposits")
		.spawn(scope);

	arb_source
		.clone()
		.vault_witnessing::<ArbCallBuilder, _, _, _>(
			witness_call.clone(),
			arb_client.clone(),
			env_params.arb_vault_address,
			Asset::ArbEth,
			ForeignChain::Arbitrum,
			env_params.arb_supported_erc20_tokens.clone(),
		)
		.logging("witnessing Vault")
		.spawn(scope);

	arb_source
		.clone()
		.key_manager_witnessing(
			witness_call.clone(),
			arb_client.clone(),
			env_params.arb_key_manager_address,
		)
		.logging("witnessing KeyManager")
		.spawn(scope);

	Ok(())
}
