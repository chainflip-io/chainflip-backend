use crate::{
	eth::{
		api::{
			all_batch,
			common::{
				EncodableFetchAssetParams, EncodableFetchDeployAssetParams,
				EncodableTransferAssetParams,
			},
			EthereumTransactionBuilder,
		},
		Address as EvmAddress, EthereumFetchId,
	},
	*,
};

use super::EthereumChainId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
pub struct EvmReplayProtection {
	pub nonce: u64,
	pub chain_id: EthereumChainId,
	pub key_manager_address: EvmAddress,
	pub contract_address: EvmAddress,
}

pub trait SCGatewayProvider {
	fn state_chain_gateway_address() -> EvmAddress;
}

pub fn evm_all_batch_builder<
	C: Chain<DepositFetchId = EthereumFetchId, ChainAccount = EvmAddress, ChainAmount = u128>,
	F: Fn(<C as Chain>::ChainAsset) -> Option<EvmAddress>,
>(
	fetch_params: Vec<FetchAssetParams<C>>,
	transfer_params: Vec<TransferAssetParams<C>>,
	token_address_fn: F,
	replay_protection: EvmReplayProtection,
) -> Result<EthereumTransactionBuilder<all_batch::AllBatch>, AllBatchError> {
	let mut fetch_only_params = vec![];
	let mut fetch_deploy_params = vec![];
	for FetchAssetParams { deposit_fetch_id, asset } in fetch_params {
		if let Some(token_address) = token_address_fn(asset) {
			match deposit_fetch_id {
				EthereumFetchId::Fetch(contract_address) => fetch_only_params
					.push(EncodableFetchAssetParams { contract_address, asset: token_address }),
				EthereumFetchId::DeployAndFetch(channel_id) => fetch_deploy_params
					.push(EncodableFetchDeployAssetParams { channel_id, asset: token_address }),
				EthereumFetchId::NotRequired => (),
			};
		} else {
			return Err(AllBatchError::Other)
		}
	}
	Ok(EthereumTransactionBuilder::new_unsigned(
		replay_protection,
		all_batch::AllBatch::new(
			fetch_deploy_params,
			fetch_only_params,
			transfer_params
				.into_iter()
				.map(|TransferAssetParams { asset, to, amount }| {
					token_address_fn(asset)
						.map(|address| EncodableTransferAssetParams { to, amount, asset: address })
						.ok_or(AllBatchError::Other)
				})
				.collect::<Result<Vec<_>, _>>()?,
		),
	))
}
