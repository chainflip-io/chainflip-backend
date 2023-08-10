use std::{collections::HashMap, sync::Arc};

use ethers::types::Bloom;
use sp_core::{H160, H256};

use crate::{
	eth::retry_rpc::EthersRetryRpcApi,
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
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

pub fn call_from_event(
	event: Event<VaultEvents>,
	// can be different for different EVM chains
	native_asset: Asset,
	source_chain: ForeignChain,
	supported_assets: &HashMap<EthereumAddress, Asset>,
) -> Result<Option<pallet_cf_swapping::Call<state_chain_runtime::Runtime>>> {
	fn try_into_encoded_address(chain: ForeignChain, bytes: Vec<u8>) -> Result<EncodedAddress> {
		EncodedAddress::from_chain_bytes(chain, bytes)
			.map_err(|e| anyhow!("Failed to convert into EncodedAddress: {e}"))
	}

	fn try_into_primitive<Primitive: std::fmt::Debug + TryInto<CfType> + Copy, CfType>(
		from: Primitive,
	) -> Result<CfType>
	where
		<Primitive as TryInto<CfType>>::Error: std::fmt::Display,
	{
		from.try_into().map_err(|err| {
			anyhow!("Failed to convert into {:?}: {err}", std::any::type_name::<CfType>(),)
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
				from: native_asset,
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
			from: *(supported_assets
				.get(&src_token)
				.ok_or(anyhow!("Source token {src_token} not found"))?),
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
			source_asset: native_asset,
			destination_asset: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			deposit_metadata: CcmDepositMetadata {
				source_chain,
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
			source_asset: *(supported_assets
				.get(&src_token)
				.ok_or(anyhow!("Source token {src_token} not found"))?),
			destination_asset: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			deposit_metadata: CcmDepositMetadata {
				source_chain,
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
		contract_address: EthereumAddress,
		native_asset: Asset,
		source_chain: ForeignChain,
		supported_assets: HashMap<EthereumAddress, Asset>,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner::Chain:
			cf_chains::Chain<ChainAmount = u128, DepositDetails = (), ChainAccount = H160>,
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = Bloom>,
		StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
	{
		self.then::<Result<Bloom>, _, _>(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			let eth_rpc = eth_rpc.clone();
			let supported_assets = supported_assets.clone();
			async move {
				for event in
					events_at_block::<VaultEvents, _>(header, contract_address, &eth_rpc).await?
				{
					match call_from_event(event, native_asset, source_chain, &supported_assets) {
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
						Err(message) => {
							tracing::error!("Ignoring vault contract event: {message}");
						},
					}
				}

				Result::Ok(header.data)
			}
		})
	}
}
