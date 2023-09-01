use core::marker::PhantomData;

use cf_chains::{
	AllBatch, AllBatchError, ApiCall, Chain, ChainAbi, ChainCrypto, ChainEnvironment, Ethereum,
	ExecutexSwapAndCall, FetchAssetParams, ForeignChainAddress, TransferAssetParams,
};
use cf_primitives::{chains::assets, EgressId, ForeignChain, GasUnit};
use codec::{Decode, Encode};
use frame_support::{CloneNoBound, DebugNoBound, PartialEqNoBound};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

pub const ETHEREUM_ETH_ADDRESS: [u8; 20] = [0xee; 20];
pub const ETHEREUM_FLIP_ADDRESS: [u8; 20] = [0xcf; 20];
pub const ETHEREUM_USDC_ADDRESS: [u8; 20] = [0x45; 20];
#[derive(Encode, Decode, TypeInfo, Eq, PartialEq)]
pub struct MockEthEnvironment;

impl ChainEnvironment<<Ethereum as Chain>::ChainAsset, <Ethereum as Chain>::ChainAccount>
	for MockEthEnvironment
{
	fn lookup(asset: <Ethereum as Chain>::ChainAsset) -> Option<<Ethereum as Chain>::ChainAccount> {
		match asset {
			assets::eth::Asset::Eth => Some(ETHEREUM_ETH_ADDRESS.into()),
			assets::eth::Asset::Flip => Some(ETHEREUM_FLIP_ADDRESS.into()),
			assets::eth::Asset::Usdc => Some(ETHEREUM_USDC_ADDRESS.into()),
		}
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub enum MockEthereumApiCall<MockEthEnvironment> {
	AllBatch(MockAllBatch<MockEthEnvironment>),
	ExecutexSwapAndCall(MockExecutexSwapAndCall<MockEthEnvironment>),
}

impl ApiCall<Ethereum> for MockEthereumApiCall<MockEthEnvironment> {
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

	fn transaction_out_id(&self) -> <Ethereum as ChainCrypto>::TransactionOutId {
		unimplemented!()
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Default, Eq, Encode, Decode, TypeInfo)]
pub struct MockAllBatch<MockEthEnvironment> {
	pub nonce: <Ethereum as ChainAbi>::ReplayProtection,
	pub fetch_params: Vec<FetchAssetParams<Ethereum>>,
	pub transfer_params: Vec<TransferAssetParams<Ethereum>>,
	_phantom: PhantomData<MockEthEnvironment>,
}

impl MockAllBatch<MockEthEnvironment> {
	pub fn set_success(success: bool) {
		ALL_BATCH_SUCCESS.with(|cell| *cell.borrow_mut() = success);
	}
}

thread_local! {
	static ALL_BATCH_SUCCESS: std::cell::RefCell<bool> = std::cell::RefCell::new(true);
}

impl AllBatch<Ethereum> for MockEthereumApiCall<MockEthEnvironment> {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Result<Self, AllBatchError> {
		if ALL_BATCH_SUCCESS.with(|cell| *cell.borrow()) {
			Ok(Self::AllBatch(MockAllBatch {
				nonce: Default::default(),
				fetch_params,
				transfer_params,
				_phantom: PhantomData,
			}))
		} else {
			Err(AllBatchError::Other)
		}
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub struct MockExecutexSwapAndCall<MockEthEnvironment> {
	nonce: <Ethereum as ChainAbi>::ReplayProtection,
	egress_id: EgressId,
	transfer_param: TransferAssetParams<Ethereum>,
	source_chain: ForeignChain,
	source_address: Option<ForeignChainAddress>,
	gas_limit: GasUnit,
	message: Vec<u8>,
	_phantom: PhantomData<MockEthEnvironment>,
}

impl ExecutexSwapAndCall<Ethereum> for MockEthereumApiCall<MockEthEnvironment> {
	fn new_unsigned(
		egress_id: EgressId,
		transfer_param: TransferAssetParams<Ethereum>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_limit: GasUnit,
		message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		if MockEthEnvironment::lookup(transfer_param.asset).is_none() {
			Err(DispatchError::CannotLookup)
		} else {
			Ok(Self::ExecutexSwapAndCall(MockExecutexSwapAndCall {
				nonce: Default::default(),
				egress_id,
				transfer_param,
				source_chain,
				source_address,
				gas_limit,
				message,
				_phantom: PhantomData,
			}))
		}
	}
}
