use crate::evm::retry_rpc::EvmRetryRpcApi;
use anyhow::{anyhow, Result};
use codec::Decode;
use ethers::types::Bloom;
use futures_core::Future;
use sp_core::H256;
use std::collections::HashMap;

use super::{
	super::common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	contract_common::{events_at_block, Event},
};

use cf_chains::{
	address::{EncodedAddress, IntoForeignChainAddress},
	cf_parameters::{CfParameters, VaultSwapParameters, VersionedCfParameters},
	eth::Address as EthereumAddress,
	evm::DepositDetails,
	CcmChannelMetadata, CcmDepositMetadata, Chain,
};
use cf_primitives::{Asset, AssetAmount, EpochIndex, ForeignChain};
use ethers::prelude::*;
use state_chain_runtime::{EthereumInstance, Runtime, RuntimeCall};

abigen!(Vault, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IVault.json");

fn decode_cf_parameters<CcmData>(
	cf_parameters: &[u8],
	block_height: u64,
) -> (Option<VaultSwapParameters>, CcmData)
where
	CcmData: Default + Decode,
{
	if let Ok(decoded) = VersionedCfParameters::<CcmData>::decode(&mut &cf_parameters[..]) {
		match decoded {
			VersionedCfParameters::V0(CfParameters {
				ccm_additional_data,
				vault_swap_parameters,
			}) => (Some(vault_swap_parameters), ccm_additional_data),
		}
	} else {
		tracing::warn!("Failed to decode cf_parameters: {cf_parameters:?} at block {block_height}");
		(None, Default::default())
	}
}

pub fn call_from_event<
	C: cf_chains::Chain<ChainAccount = EthereumAddress, ChainBlockNumber = u64>,
	CallBuilder: IngressCallBuilder<Chain = C>,
>(
	block_height: u64,
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
			cf_parameters,
		}) => {
			let vault_swap_parameters =
				decode_cf_parameters::<()>(&cf_parameters[..], block_height).0;

			Some(CallBuilder::vault_swap_request(
				block_height,
				native_asset,
				try_into_primitive(amount)?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				None,
				event.tx_hash,
				vault_swap_parameters,
			))
		},
		VaultEvents::SwapTokenFilter(SwapTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender: _,
			cf_parameters,
		}) => {
			let vault_swap_parameters =
				decode_cf_parameters::<()>(&cf_parameters[..], block_height).0;

			Some(CallBuilder::vault_swap_request(
				block_height,
				*(supported_assets
					.get(&src_token)
					.ok_or(anyhow!("Source token {src_token:?} not found"))?),
				try_into_primitive(amount)?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				None,
				event.tx_hash,
				vault_swap_parameters,
			))
		},
		VaultEvents::XcallNativeFilter(XcallNativeFilter {
			dst_chain,
			dst_address,
			dst_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		}) => {
			let (vault_swap_parameters, ccm_additional_data) =
				decode_cf_parameters(&cf_parameters[..], block_height);

			Some(CallBuilder::vault_swap_request(
				block_height,
				native_asset,
				try_into_primitive(amount)?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				Some(CcmDepositMetadata {
					source_chain,
					source_address: Some(sender.into_foreign_chain_address()),
					channel_metadata: CcmChannelMetadata {
						message: message
							.to_vec()
							.try_into()
							.map_err(|_| anyhow!("Failed to deposit CCM: `message` too long."))?,
						gas_budget: try_into_primitive(gas_amount)?,
						ccm_additional_data,
					},
				}),
				event.tx_hash,
				vault_swap_parameters,
			))
		},
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
		}) => {
			let (vault_swap_parameters, ccm_additional_data) =
				decode_cf_parameters(&cf_parameters[..], block_height);

			Some(CallBuilder::vault_swap_request(
				block_height,
				*(supported_assets
					.get(&src_token)
					.ok_or(anyhow!("Source token {src_token:?} not found"))?),
				try_into_primitive(amount)?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				Some(CcmDepositMetadata {
					source_chain,
					source_address: Some(sender.into_foreign_chain_address()),
					channel_metadata: CcmChannelMetadata {
						message: message
							.to_vec()
							.try_into()
							.map_err(|_| anyhow!("Failed to deposit CCM. Message too long."))?,
						gas_budget: try_into_primitive(gas_amount)?,
						ccm_additional_data,
					},
				}),
				event.tx_hash,
				vault_swap_parameters,
			))
		},
		VaultEvents::TransferNativeFailedFilter(TransferNativeFailedFilter {
			recipient,
			amount,
		}) => Some(CallBuilder::vault_transfer_failed(
			native_asset
				.try_into()
				.unwrap_or_else(|_| panic!("Native asset must be supported by the chain.")),
			try_into_primitive::<_, AssetAmount>(amount)?
				.try_into()
				.unwrap_or_else(|_| panic!("Amount must be supported by the chain.")),
			recipient,
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

macro_rules! vault_deposit_witness {
	($source_asset: expr, $deposit_amount: expr, $dest_asset: expr, $dest_address: expr, $metadata: expr, $tx_id: expr, $params: expr) => {
		if let Some(params) = $params {
			VaultDepositWitness {
				input_asset: $source_asset.try_into().expect("invalid asset for chain"),
				output_asset: $dest_asset,
				deposit_amount: $deposit_amount,
				destination_address: $dest_address,
				deposit_metadata: $metadata,
				tx_id: $tx_id,
				deposit_details: DepositDetails { tx_hashes: Some(vec![$tx_id]) },
				broker_fee: params.broker_fee.into(),
				affiliate_fees: params.affiliate_fees
					.into_iter()
					.map(Into::into)
					.collect_vec()
					.try_into()
					.expect("runtime supports at least as many affiliates as we allow in cf_parameters encoding"),
				boost_fee: params.boost_fee.into(),
				dca_params: params.dca_params,
				refund_params: Some(params.refund_params),
				channel_id: None,
				deposit_address: None,
			}
		} else {
			VaultDepositWitness {
				input_asset: $source_asset.try_into().expect("invalid asset for chain"),
				output_asset: $dest_asset,
				deposit_amount: $deposit_amount,
				destination_address: $dest_address,
				deposit_metadata: $metadata,
				tx_id: $tx_id,
				deposit_details: DepositDetails { tx_hashes: Some(vec![$tx_id]) },
				broker_fee: None,
				affiliate_fees: Default::default(),
				boost_fee: 0,
				dca_params: None,
				refund_params: None,
				channel_id: None,
				deposit_address: None,
			}
		}
	}
}

pub(crate) use vault_deposit_witness;

pub trait IngressCallBuilder {
	type Chain: cf_chains::Chain<ChainAccount = EthereumAddress>;

	fn vault_swap_request(
		block_height: <Self::Chain as cf_chains::Chain>::ChainBlockNumber,
		source_asset: Asset,
		deposit_amount: cf_primitives::AssetAmount,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		deposit_metadata: Option<CcmDepositMetadata>,
		tx_hash: H256,
		vault_swap_parameters: Option<VaultSwapParameters>,
	) -> state_chain_runtime::RuntimeCall;

	fn vault_transfer_failed(
		asset: <Self::Chain as cf_chains::Chain>::ChainAsset,
		amount: <Self::Chain as cf_chains::Chain>::ChainAmount,
		destination_address: <Self::Chain as cf_chains::Chain>::ChainAccount,
	) -> state_chain_runtime::RuntimeCall;
}

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn vault_witnessing<
		CallBuilder: IngressCallBuilder<Chain = Inner::Chain>,
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
			DepositDetails = DepositDetails,
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
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));

			let process_call = process_call.clone();
			let eth_rpc = eth_rpc.clone();
			let supported_assets = supported_assets.clone();
			async move {
				for event in events_at_block::<Inner::Chain, VaultEvents, _>(
					header,
					contract_address,
					&eth_rpc,
				)
				.await?
				{
					match call_from_event::<Inner::Chain, CallBuilder>(
						header.index,
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
							tracing::warn!("Ignoring vault contract event: {message}");
						},
					}
				}

				Result::Ok(header.data)
			}
		})
	}
}
