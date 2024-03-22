use ethers::types::Bloom;
use sp_core::H256;
use std::collections::HashMap;

use crate::evm::retry_rpc::EvmRetryRpcApi;

use super::{
	super::common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	contract_common::{events_at_block, Event},
};
use cf_primitives::EpochIndex;
use futures_core::Future;

use anyhow::{anyhow, Result};
use cf_chains::{
	address::{EncodedAddress, IntoForeignChainAddress},
	eth::Address as EthereumAddress,
	CcmChannelMetadata, CcmDepositMetadata,
};
use cf_primitives::{chains::assets::eth::Asset as EthereumAsset, Asset, ForeignChain};
use ethers::prelude::*;
use state_chain_runtime::{EthereumInstance, Runtime, RuntimeCall};

abigen!(Vault, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IVault.json");

pub fn call_from_event<C: cf_chains::Chain<ChainAccount = EthereumAddress>>(
	event: Event<VaultEvents>,
	// can be different for different EVM chains
	native_asset: Asset,
	source_chain: ForeignChain,
	supported_assets: &HashMap<EthereumAddress, Asset>,
) -> Result<Option<RuntimeCall>>
where
	EthereumAddress: IntoForeignChainAddress<C>,
{
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
		}) => Some(RuntimeCall::Swapping(pallet_cf_swapping::Call::schedule_swap_from_contract {
			from: native_asset,
			to: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			tx_hash: event.tx_hash.into(),
		})),
		VaultEvents::SwapTokenFilter(SwapTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender: _,
			cf_parameters: _,
		}) => Some(RuntimeCall::Swapping(pallet_cf_swapping::Call::schedule_swap_from_contract {
			from: *(supported_assets
				.get(&src_token)
				.ok_or(anyhow!("Source token {src_token:?} not found"))?),
			to: try_into_primitive(dst_token)?,
			deposit_amount: try_into_primitive(amount)?,
			destination_address: try_into_encoded_address(
				try_into_primitive(dst_chain)?,
				dst_address.to_vec(),
			)?,
			tx_hash: event.tx_hash.into(),
		})),
		VaultEvents::XcallNativeFilter(XcallNativeFilter {
			dst_chain,
			dst_address,
			dst_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		}) =>
			Some(RuntimeCall::Swapping(pallet_cf_swapping::Call::ccm_deposit {
				source_asset: native_asset,
				destination_asset: try_into_primitive(dst_token)?,
				deposit_amount: try_into_primitive(amount)?,
				destination_address: try_into_encoded_address(
					try_into_primitive(dst_chain)?,
					dst_address.to_vec(),
				)?,
				deposit_metadata: CcmDepositMetadata {
					source_chain,
					source_address: Some(
						<EthereumAddress as IntoForeignChainAddress<C>>::into_foreign_chain_address(
							sender,
						),
					),
					channel_metadata: CcmChannelMetadata {
						message: message
							.to_vec()
							.try_into()
							.map_err(|_| anyhow!("Failed to deposit CCM: `message` too long."))?,
						gas_budget: try_into_primitive(gas_amount)?,
						cf_parameters: cf_parameters.0.to_vec().try_into().map_err(|_| {
							anyhow!("Failed to deposit CCM: `cf_parameters` too long.")
						})?,
					},
				},
				tx_hash: event.tx_hash.into(),
			})),
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
		}) =>
			Some(RuntimeCall::Swapping(pallet_cf_swapping::Call::ccm_deposit {
				source_asset: *(supported_assets
					.get(&src_token)
					.ok_or(anyhow!("Source token {src_token:?} not found"))?),
				destination_asset: try_into_primitive(dst_token)?,
				deposit_amount: try_into_primitive(amount)?,
				destination_address: try_into_encoded_address(
					try_into_primitive(dst_chain)?,
					dst_address.to_vec(),
				)?,
				deposit_metadata: CcmDepositMetadata {
					source_chain,
					source_address: Some(
						<EthereumAddress as IntoForeignChainAddress<C>>::into_foreign_chain_address(
							sender,
						),
					),
					channel_metadata: CcmChannelMetadata {
						message: message
							.to_vec()
							.try_into()
							.map_err(|_| anyhow!("Failed to deposit CCM. Message too long."))?,
						gas_budget: try_into_primitive(gas_amount)?,
						cf_parameters: cf_parameters.0.to_vec().try_into().map_err(|_| {
							anyhow!("Failed to deposit CCM. cf_parameter too long.")
						})?,
					},
				},
				tx_hash: event.tx_hash.into(),
			})),
		VaultEvents::TransferNativeFailedFilter(TransferNativeFailedFilter {
			recipient,
			amount,
		}) => Some(RuntimeCall::EthereumIngressEgress(
			pallet_cf_ingress_egress::Call::vault_transfer_failed {
				asset: EthereumAsset::Eth,
				amount: try_into_primitive(amount)?,
				destination_address: recipient,
			},
		)),
		VaultEvents::TransferTokenFailedFilter(TransferTokenFailedFilter {
			recipient,
			amount,
			token,
			reason: _,
		}) => Some(RuntimeCall::EthereumIngressEgress(pallet_cf_ingress_egress::Call::<
			Runtime,
			EthereumInstance,
		>::vault_transfer_failed {
			asset: (*(supported_assets.get(&token).ok_or(anyhow!("Asset {token:?} not found"))?))
				.try_into()
				.expect("Asset translated from EthereumAddress must be supported by the chain."),
			amount: try_into_primitive(amount)?,
			destination_address: recipient,
		})),
		_ => None,
	})
}

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn vault_witnessing<
		EvmRpcClient: EvmRetryRpcApi + ChainClient + Clone,
		ProcessCall,
		ProcessingFut,
	>(
		self,
		process_call: ProcessCall,
		eth_rpc: EvmRpcClient,
		contract_address: EthereumAddress,
		native_asset: Asset,
		source_chain: ForeignChain,
		supported_assets: HashMap<EthereumAddress, Asset>,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner::Chain: cf_chains::Chain<
			ChainAmount = u128,
			DepositDetails = (),
			ChainAccount = EthereumAddress,
		>,
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = Bloom>,
		EthereumAddress: IntoForeignChainAddress<Inner::Chain>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
	{
		self.then::<Result<Bloom>, _, _>(move |epoch, header| {
			let process_call = process_call.clone();
			let eth_rpc = eth_rpc.clone();
			let supported_assets = supported_assets.clone();
			async move {
				for event in
					events_at_block::<VaultEvents, _>(header, contract_address, &eth_rpc).await?
				{
					match call_from_event::<Inner::Chain>(
						event,
						native_asset,
						source_chain,
						&supported_assets,
					) {
						Ok(option_call) =>
							if let Some(call) = option_call {
								process_call(call, epoch.index).await;
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
