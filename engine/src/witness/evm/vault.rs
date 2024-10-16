use codec::Decode;
use ethers::types::Bloom;
use sp_core::{ConstU32, H256};
use std::collections::HashMap;

use crate::{
	evm::retry_rpc::EvmRetryRpcApi,
	witness::eth::{ChannellessCfParameters, SharedCfParametersContract},
};

use super::{
	super::common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	contract_common::{events_at_block, Event},
};
use cf_primitives::{AssetAmount, EpochIndex};
use futures_core::Future;

use anyhow::{anyhow, Result};
use cf_chains::{
	address::{EncodedAddress, IntoForeignChainAddress},
	eth::Address as EthereumAddress,
	evm::DepositDetails,
	CcmCfParameters, CcmChannelMetadata, CcmDepositMetadata, Chain, ChannelRefundParameters,
};
use cf_primitives::{Asset, BasisPoints, DcaParameters, ForeignChain};
use ethers::prelude::*;
use frame_support::sp_runtime::BoundedVec;
use state_chain_runtime::{EthereumInstance, Runtime, RuntimeCall};

abigen!(Vault, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IVault.json");

pub const MAX_CF_PARAM_LENGTH: u32 = 1_000;
pub type CfParameters = BoundedVec<u8, ConstU32<MAX_CF_PARAM_LENGTH>>;

pub fn call_from_event<
	C: cf_chains::Chain<ChainAccount = EthereumAddress>,
	CallBuilder: IngressCallBuilder<Chain = C>,
>(
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

	fn decode_shared_cf_parameters(
		cf_parameters_vec: CfParameters,
	) -> Result<(Option<ChannelRefundParameters>, Option<DcaParameters>, Option<BasisPoints>)> {
		println!("DEBUGDEBUG cf_parameters_vec {:?}", cf_parameters_vec);

		if cf_parameters_vec.is_empty() {
			println!("DEBUGDEBUG Emtpy cf_parameters_vec");

			Ok((None, None, None))
		} else {
			let shared_cf_parameters: SharedCfParametersContract =
				SharedCfParametersContract::decode(&mut &cf_parameters_vec[..])
					.map_err(|_| anyhow!("Failed to decode to `SharedCfParametersContract`"))?;

			println!("DEBUGDEBUG shared_cf_parameters {:?}", shared_cf_parameters);

			Ok((
				shared_cf_parameters.refund_params,
				shared_cf_parameters.dca_params,
				shared_cf_parameters.boost_fee,
			))
		}
	}

	fn decode_channelless_cf_parameters(
		cf_parameters_vec: CfParameters,
	) -> Result<(
		CcmCfParameters,
		(Option<ChannelRefundParameters>, Option<DcaParameters>, Option<BasisPoints>),
	)> {
		println!("DEBUGDEBUG cf_parameters_vec {:?}", cf_parameters_vec);

		if cf_parameters_vec.is_empty() {
			println!("DEBUGDEBUG Emtpy cf_parameters_vec");

			Ok((cf_parameters_vec, (None, None, None)))
		} else {
			let channelless_cf_parameters: ChannellessCfParameters =
				ChannellessCfParameters::decode(&mut &cf_parameters_vec[..])
					.map_err(|_| anyhow!("Failed to decode to `ChannellessCfParameters`"))?;

			println!("DEBUGDEBUG channelless_cf_parameters {:?}", channelless_cf_parameters);

			let (refund_params, dca_params, boost_fee) = if let Some(shared_cf_parameters) =
				channelless_cf_parameters.shared_cf_parameters
			{
				(
					shared_cf_parameters.refund_params,
					shared_cf_parameters.dca_params,
					shared_cf_parameters.boost_fee,
				)
			} else {
				(None, None, None)
			};

			Ok((
				// Default to empty CcmCfParameters if not present
				channelless_cf_parameters.ccm_cf_parameters.unwrap_or_default(),
				(refund_params, dca_params, boost_fee),
			))
		}
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
			println!("DEBUGDEBUG cf_parameters {:?}", cf_parameters);
			let cf_parameters_vec: CfParameters = cf_parameters
				.to_vec()
				.try_into()
				.map_err(|_| anyhow!("Failed to decode `cf_parameters` too long."))?;
			let (refund_params, dca_params, boost_fee) =
				decode_shared_cf_parameters(cf_parameters_vec)?;

			Some(CallBuilder::contract_swap_request(
				native_asset,
				try_into_primitive(amount)?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				None,
				event.tx_hash.into(),
				refund_params,
				dca_params,
				boost_fee,
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
			println!("DEBUGDEBUG cf_parameters {:?}", cf_parameters);

			let cf_parameters_vec: CfParameters = cf_parameters
				.to_vec()
				.try_into()
				.map_err(|_| anyhow!("Failed to decode `cf_parameters` too long."))?;
			let (refund_params, dca_params, boost_fee) =
				decode_shared_cf_parameters(cf_parameters_vec)?;

			Some(CallBuilder::contract_swap_request(
				*(supported_assets
					.get(&src_token)
					.ok_or(anyhow!("Source token {src_token:?} not found"))?),
				try_into_primitive(amount)?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				None,
				event.tx_hash.into(),
				refund_params,
				dca_params,
				boost_fee,
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
			println!("DEBUGDEBUG cf_parameters {:?}", cf_parameters);

			let cf_parameters_vec: CfParameters = cf_parameters
				.to_vec()
				.try_into()
				.map_err(|_| anyhow!("Failed to decode `cf_parameters` too long."))?;
			let (ccm_cf_parameters, (refund_params, dca_params, boost_fee)) =
				decode_channelless_cf_parameters(cf_parameters_vec)?;

			Some(CallBuilder::contract_swap_request(
				native_asset,
				try_into_primitive(amount)?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				Some(CcmDepositMetadata {
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
						ccm_cf_parameters,
					},
				}),
				event.tx_hash.into(),
				refund_params,
				dca_params,
				boost_fee,
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
			println!("DEBUGDEBUG cf_parameters {:?}", cf_parameters);

			let cf_parameters_vec: CfParameters = cf_parameters
				.to_vec()
				.try_into()
				.map_err(|_| anyhow!("Failed to decode `cf_parameters` too long."))?;
			let (ccm_cf_parameters, (refund_params, dca_params, boost_fee)) =
				decode_channelless_cf_parameters(cf_parameters_vec)?;

			Some(CallBuilder::contract_swap_request(
				*(supported_assets
					.get(&src_token)
					.ok_or(anyhow!("Source token {src_token:?} not found"))?),
				try_into_primitive(amount)?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				Some(CcmDepositMetadata {
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
						ccm_cf_parameters,
					},
				}),
				event.tx_hash.into(),
				refund_params,
				dca_params,
				boost_fee,
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

pub trait IngressCallBuilder {
	type Chain: cf_chains::Chain<ChainAccount = EthereumAddress>;

	fn contract_swap_request(
		source_asset: Asset,
		deposit_amount: cf_primitives::AssetAmount,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		deposit_metadata: Option<CcmDepositMetadata>,
		tx_hash: cf_primitives::TransactionHash,
		refund_params: Option<ChannelRefundParameters>,
		dca_params: Option<DcaParameters>,
		boost_fee: Option<BasisPoints>,
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
