use std::sync::Arc;

use cf_chains::Ethereum;
use ethers::types::Bloom;
use sp_core::{H160, H256};

use crate::{
	eth::retry_rpc::EthersRetryRpcApi,
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcClient, RawRpcApi},
		extrinsic_api::signed::SignedExtrinsicApi,
		StateChainClient,
	},
};

use super::{
	super::common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	contract_common::{events_at_block, Event},
};

use anyhow::{anyhow, Result};
use cf_chains::{
	address::EncodedAddress, eth::Address as EthereumAddress, CcmChannelMetadata,
	CcmDepositMetadata,
};
use cf_primitives::{Asset, ForeignChain};
use ethers::prelude::*;

abigen!(Vault, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IVault.json");

#[async_trait::async_trait]
pub trait EthAssetApi {
	async fn asset(&self, token_address: EthereumAddress) -> Result<Option<Asset>>;
}

#[async_trait::async_trait]
impl<RawRpcClient: RawRpcApi + Send + Sync + 'static, SignedExtrinsicClient: Send + Sync>
	EthAssetApi for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	async fn asset(&self, token_address: EthereumAddress) -> Result<Option<Asset>> {
		self.base_rpc_client
			.raw_rpc_client
			.cf_eth_asset(None, token_address)
			.await
			.map_err(Into::into)
	}
}

pub enum CallFromEventError {
	Network(anyhow::Error),
	Decode(String),
}

pub async fn call_from_event<StateChainClient>(
	event: Event<VaultEvents>,
	state_chain_client: Arc<StateChainClient>,
) -> Result<Option<pallet_cf_swapping::Call<state_chain_runtime::Runtime>>, CallFromEventError>
where
	StateChainClient: EthAssetApi,
{
	fn try_into_encoded_address(
		chain: ForeignChain,
		bytes: Vec<u8>,
	) -> Result<EncodedAddress, CallFromEventError> {
		EncodedAddress::from_chain_bytes(chain, bytes).map_err(|e| {
			CallFromEventError::Decode(format!("Failed to convert into EncodedAddress: {e}"))
		})
	}

	fn try_into_primitive<Primitive: std::fmt::Debug + TryInto<CfType> + Copy, CfType>(
		from: Primitive,
	) -> Result<CfType, CallFromEventError>
	where
		<Primitive as TryInto<CfType>>::Error: std::fmt::Display,
	{
		from.try_into().map_err(|err| {
			CallFromEventError::Decode(format!(
				"Failed to convert into {:?}: {err}",
				std::any::type_name::<CfType>(),
			))
		})
	}

	Ok(match event.event_parameters {
		VaultEvents::SwapNativeFilter(SwapNativeFilter {
			dst_chain,
			dst_address,
			dst_token,
			amount,
			sender: _,
			cf_parameters: _,
		}) => Some(
			pallet_cf_swapping::Call::<state_chain_runtime::Runtime>::schedule_swap_from_contract {
				from: Asset::Eth,
				to: try_into_primitive(dst_token)?,
				deposit_amount: try_into_primitive(amount)?,
				destination_address: try_into_encoded_address(
					try_into_primitive(dst_chain)?,
					dst_address.to_vec(),
				)?,
				tx_hash: event.tx_hash.into(),
			},
		),
		VaultEvents::SwapTokenFilter(SwapTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender: _,
			cf_parameters: _,
		}) => Some(pallet_cf_swapping::Call::schedule_swap_from_contract {
			from: state_chain_client
				.asset(src_token)
				.await
				.map_err(|e| {
					CallFromEventError::Network(anyhow!(
						"Failed to retrieve from token for SwapToken call: {e}"
					))
				})?
				.ok_or(CallFromEventError::Decode(format!("Source token {src_token} not found")))?,
			to: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			tx_hash: event.tx_hash.into(),
		}),
		VaultEvents::XcallNativeFilter(XcallNativeFilter {
			dst_chain,
			dst_address,
			dst_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		}) => Some(pallet_cf_swapping::Call::ccm_deposit {
			source_asset: Asset::Eth,
			destination_asset: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			deposit_metadata: CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(sender.into()),
				channel_metadata: CcmChannelMetadata {
					message: message.to_vec(),
					gas_budget: try_into_primitive(gas_amount)?,
					cf_parameters: cf_parameters.0.to_vec(),
				},
			},
			tx_hash: event.tx_hash.into(),
		}),
		VaultEvents::XcallTokenFilter(XcallTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		}) => Some(pallet_cf_swapping::Call::ccm_deposit {
			source_asset: state_chain_client
				.asset(src_token)
				.await
				.map_err(|e| {
					CallFromEventError::Network(anyhow!(
						"Failed to retrieve From token for XCallToken call: {e}"
					))
				})?
				.ok_or(CallFromEventError::Decode(format!("Source token {src_token} not found")))?,
			destination_asset: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			deposit_metadata: CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(sender.into()),
				channel_metadata: CcmChannelMetadata {
					message: message.to_vec(),
					gas_budget: try_into_primitive(gas_amount)?,
					cf_parameters: cf_parameters.0.to_vec(),
				},
			},
			tx_hash: event.tx_hash.into(),
		}),
		_ => None,
	})
}

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn vault_witnessing<
		StateChainClient,
		EthRpcClient: EthersRetryRpcApi + ChainClient + Clone,
	>(
		self,
		state_chain_client: Arc<StateChainClient>,
		eth_rpc: EthRpcClient,
		contract_address: H160,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = Bloom, Chain = Ethereum>,
		StateChainClient: SignedExtrinsicApi + EthAssetApi + Send + Sync + 'static,
	{
		self.then::<Result<Bloom>, _, _>(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			let eth_rpc = eth_rpc.clone();
			async move {
				for event in
					events_at_block::<VaultEvents, _>(header, contract_address, &eth_rpc).await?
				{
					match call_from_event(event, state_chain_client.clone()).await {
						Ok(option_call) =>
							if let Some(call) = option_call {
								state_chain_client
									.submit_signed_extrinsic(
										pallet_cf_witnesser::Call::witness_at_epoch {
											call: Box::new(call.into()),
											epoch_index: epoch.index,
										},
									)
									.await;
							},
						Err(CallFromEventError::Decode(message)) => {
							tracing::error!("Ignoring vault contract event: {message}");
						},
						Err(CallFromEventError::Network(err)) => return Err(err),
					}
				}

				Result::Ok(header.data)
			}
		})
	}
}
