use core::marker::PhantomData;

use cf_chains::{
	eth::assets, AllBatch, ApiCall, Chain, ChainAbi, ChainCrypto, ChainEnvironment, Ethereum,
	FetchAssetParams, ReplayProtectionProvider, TransferAssetParams,
};
use cf_primitives::{EthereumAddress, ETHEREUM_ETH_ADDRESS};
use codec::{Decode, Encode};
use frame_support::{CloneNoBound, DebugNoBound, PartialEqNoBound};
use scale_info::TypeInfo;

use super::eth_replay_protection_provider::MockEthReplayProtectionProvider;

pub const ETHEREUM_FLIP_ADDRESS: EthereumAddress = [0x00; 20];
#[derive(Encode, Decode, TypeInfo, Eq, PartialEq)]
pub struct MockEthEnvironment;

impl ChainEnvironment<<Ethereum as Chain>::ChainAsset, <Ethereum as Chain>::ChainAccount>
	for MockEthEnvironment
{
	fn lookup(asset: <Ethereum as Chain>::ChainAsset) -> Option<<Ethereum as Chain>::ChainAccount> {
		match asset {
			assets::eth::Asset::Eth => Some(ETHEREUM_ETH_ADDRESS.into()),
			assets::eth::Asset::Flip => Some(ETHEREUM_FLIP_ADDRESS.into()),
			_ => None,
		}
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Default, Eq, Encode, Decode, TypeInfo)]
pub struct MockAllBatch<MockEthEnvironment> {
	nonce: <Ethereum as ChainAbi>::ReplayProtection,
	fetch_params: Vec<FetchAssetParams<Ethereum>>,
	transfer_params: Vec<TransferAssetParams<Ethereum>>,
	_phantom: PhantomData<MockEthEnvironment>,
}

impl AllBatch<Ethereum> for MockAllBatch<MockEthEnvironment> {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Result<Self, ()> {
		if fetch_params
			.iter()
			.any(|FetchAssetParams { asset, .. }| MockEthEnvironment::lookup(*asset).is_none()) ||
			transfer_params.iter().any(|TransferAssetParams { asset, .. }| {
				MockEthEnvironment::lookup(*asset).is_none()
			}) {
			Err(())
		} else {
			Ok(Self {
				nonce: MockEthReplayProtectionProvider::replay_protection(),
				fetch_params,
				transfer_params,
				_phantom: PhantomData,
			})
		}
	}
}

impl ApiCall<Ethereum> for MockAllBatch<MockEthEnvironment> {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(self, _threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}
