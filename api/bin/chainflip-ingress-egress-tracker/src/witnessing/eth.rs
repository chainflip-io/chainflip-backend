use std::sync::Arc;

use cf_primitives::chains::assets::eth::Asset;
use utilities::task_scope;

use chainflip_engine::{
	eth::{retry_rpc::EthersRetryRpcClient, rpc::EthRpcSigningClient},
	settings::NodeContainer,
	state_chain_observer::client::{StateChainClient, StateChainStreamApi},
	witness::{
		common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
		eth::{
			erc20_deposits::{flip::FlipEvents, usdc::UsdcEvents},
			EthSource,
		},
	},
};

use crate::DepositTrackerSettings;

use super::EnvironmentParameters;

pub(super) async fn start<ProcessCall, ProcessingFut>(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	state_chain_client: Arc<StateChainClient<()>>,
	state_chain_stream: impl StateChainStreamApi<false> + Clone,
	settings: DepositTrackerSettings,
	env_params: EnvironmentParameters,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient<()>, (), ()>,
	witness_call: ProcessCall,
) -> anyhow::Result<()>
where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, cf_primitives::EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: futures::Future<Output = ()> + Send + 'static,
{
	let eth_client = {
		let nodes = NodeContainer { primary: settings.eth_node.clone(), backup: None };

		EthersRetryRpcClient::<EthRpcSigningClient>::new(
			scope,
			settings.eth_key_path,
			nodes,
			env_params.eth_chain_id.into(),
		)?
	};

	let vaults = epoch_source.vaults().await;
	let eth_source = EthSource::new(eth_client.clone())
		.strictly_monotonic()
		.chunk_by_vault(vaults, scope);

	let eth_source_deposit_addresses = eth_source
		.clone()
		.deposit_addresses(scope, state_chain_stream, state_chain_client.clone())
		.await;

	eth_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdcEvents>(
			witness_call.clone(),
			eth_client.clone(),
			Asset::Usdc,
			env_params.usdc_contract_address,
		)
		.await?
		.logging("witnessing USDCDeposits")
		.spawn(scope);

	eth_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, FlipEvents>(
			witness_call.clone(),
			eth_client.clone(),
			Asset::Flip,
			env_params.flip_contract_address,
		)
		.await?
		.logging("witnessing FlipDeposits")
		.spawn(scope);

	eth_source_deposit_addresses
		.clone()
		.ethereum_deposits(
			witness_call.clone(),
			eth_client.clone(),
			Asset::Eth,
			env_params.eth_address_checker_address,
			env_params.eth_vault_address,
		)
		.await
		.logging("witnessing EthereumDeposits")
		.spawn(scope);

	eth_source
		.clone()
		.vault_witnessing(
			witness_call.clone(),
			eth_client.clone(),
			env_params.eth_vault_address,
			cf_primitives::Asset::Eth,
			cf_primitives::ForeignChain::Ethereum,
			env_params.supported_erc20_tokens.clone(),
		)
		.logging("witnessing Vault")
		.spawn(scope);

	Ok(())
}
