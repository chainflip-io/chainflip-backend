use std::sync::Arc;

use async_trait::async_trait;
use cf_primitives::{Asset, EpochIndex, EvmAddress};
use tracing::info;
use web3::types::H160;

use crate::{
	eth::ethers_vault::{call_from_event, CallFromEventError, VaultEvents},
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcClient, RawRpcApi},
		extrinsic_api::signed::SignedExtrinsicApi,
		StateChainClient,
	},
};

use super::{event::Event, rpc::EthRpcApi, BlockWithItems, EthContractWitnesser};

use anyhow::Result;

pub struct Vault {
	pub deployed_address: H160,
}

#[async_trait]
pub trait EthAssetApi {
	async fn asset(&self, token_address: EvmAddress) -> Result<Option<Asset>>;
}

#[async_trait]
impl<RawRpcClient: RawRpcApi + Send + Sync + 'static, SignedExtrinsicClient: Send + Sync>
	EthAssetApi for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	async fn asset(&self, token_address: EvmAddress) -> Result<Option<Asset>> {
		self.base_rpc_client
			.raw_rpc_client
			.cf_eth_asset(None, token_address)
			.await
			.map_err(Into::into)
	}
}

#[async_trait]
impl EthContractWitnesser for Vault {
	type EventParameters = VaultEvents;

	fn contract_name(&self) -> String {
		"Vault".to_string()
	}

	async fn handle_block_events<StateChainClient, EthRpcClient>(
		&mut self,
		epoch: EpochIndex,
		_block_number: u64,
		block: BlockWithItems<Event<Self::EventParameters>>,
		state_chain_client: Arc<StateChainClient>,
		_eth_rpc: &EthRpcClient,
	) -> Result<()>
	where
		EthRpcClient: EthRpcApi + Sync + Send,
		StateChainClient: SignedExtrinsicApi + EthAssetApi + Send + Sync,
	{
		for event in block.block_items {
			info!("Handling event: {event}");

			match call_from_event(event, state_chain_client.clone()).await {
				Ok(call) => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(call.into()),
							epoch_index: epoch,
						})
						.await;
				},
				Err(CallFromEventError::Network(err)) => return Err(err),
				Err(CallFromEventError::Decode(message)) => {
					tracing::warn!("Ignoring event: {message}");
					continue
				},
			}
		}
		Ok(())
	}

	fn contract_address(&self) -> H160 {
		self.deployed_address
	}
}

impl Vault {
	pub fn new(deployed_address: H160) -> Self {
		Self { deployed_address }
	}
}
