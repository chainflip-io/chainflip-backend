use core::marker::PhantomData;

use cf_chains::{
	btc::BitcoinCrypto, evm::EvmCrypto, AllBatch, AllBatchError, ApiCall, Bitcoin, Chain,
	ChainCrypto, ChainEnvironment, ConsolidationError, Ethereum, ExecutexSwapAndCall,
	FetchAssetParams, ForeignChainAddress, TransferAssetParams, TransferFallback,
};
use cf_primitives::{chains::assets, ForeignChain};
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
	AllBatch(MockEthAllBatch<MockEthEnvironment>),
	ExecutexSwapAndCall(MockEthExecutexSwapAndCall<MockEthEnvironment>),
	TransferFallback(MockEthTransferFallback<MockEthEnvironment>),
}

impl ApiCall<EvmCrypto> for MockEthereumApiCall<MockEthEnvironment> {
	fn threshold_signature_payload(&self) -> <EvmCrypto as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(self, _threshold_signature: &<EvmCrypto as ChainCrypto>::ThresholdSignature) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}

	fn transaction_out_id(&self) -> <EvmCrypto as ChainCrypto>::TransactionOutId {
		unimplemented!()
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Default, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthAllBatch<MockEthEnvironment> {
	pub nonce: <Ethereum as Chain>::ReplayProtection,
	pub fetch_params: Vec<FetchAssetParams<Ethereum>>,
	pub transfer_params: Vec<TransferAssetParams<Ethereum>>,
	_phantom: PhantomData<MockEthEnvironment>,
}

impl MockEthAllBatch<MockEthEnvironment> {
	pub fn set_success(success: bool) {
		ALL_BATCH_SUCCESS.with(|cell| *cell.borrow_mut() = success);
	}
}

thread_local! {
	static ALL_BATCH_SUCCESS: std::cell::RefCell<bool> = const { std::cell::RefCell::new(true) };
	pub static SHOULD_CONSOLIDATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl AllBatch<Ethereum> for MockEthereumApiCall<MockEthEnvironment> {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Result<Self, AllBatchError> {
		if ALL_BATCH_SUCCESS.with(|cell| *cell.borrow()) {
			Ok(Self::AllBatch(MockEthAllBatch {
				nonce: Default::default(),
				fetch_params,
				transfer_params,
				_phantom: PhantomData,
			}))
		} else {
			Err(AllBatchError::UnsupportedToken)
		}
	}
}

impl cf_chains::ConsolidateCall<Ethereum> for MockEthereumApiCall<MockEthEnvironment> {
	fn consolidate_utxos() -> Result<Self, cf_chains::ConsolidationError> {
		// Consolidation isn't necessary for Ethereum, but this implementation
		// helps in testing some generic behaviour

		if SHOULD_CONSOLIDATE.with(|cell| cell.get()) {
			Ok(Self::AllBatch(MockEthAllBatch {
				nonce: Default::default(),
				fetch_params: Default::default(),
				transfer_params: Default::default(),
				_phantom: PhantomData,
			}))
		} else {
			Err(ConsolidationError::NotRequired)
		}
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthExecutexSwapAndCall<MockEthEnvironment> {
	nonce: <Ethereum as Chain>::ReplayProtection,
	transfer_param: TransferAssetParams<Ethereum>,
	source_chain: ForeignChain,
	source_address: Option<ForeignChainAddress>,
	gas_budget: <Ethereum as Chain>::ChainAmount,
	message: Vec<u8>,
	_phantom: PhantomData<MockEthEnvironment>,
}

impl ExecutexSwapAndCall<Ethereum> for MockEthereumApiCall<MockEthEnvironment> {
	fn new_unsigned(
		transfer_param: TransferAssetParams<Ethereum>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: <Ethereum as Chain>::ChainAmount,
		message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		if MockEthEnvironment::lookup(transfer_param.asset).is_none() {
			Err(DispatchError::CannotLookup)
		} else {
			Ok(Self::ExecutexSwapAndCall(MockEthExecutexSwapAndCall {
				nonce: Default::default(),
				transfer_param,
				source_chain,
				source_address,
				gas_budget,
				message,
				_phantom: PhantomData,
			}))
		}
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthTransferFallback<MockEthEnvironment> {
	nonce: <Ethereum as Chain>::ReplayProtection,
	transfer_param: TransferAssetParams<Ethereum>,
	_phantom: PhantomData<MockEthEnvironment>,
}

impl TransferFallback<Ethereum> for MockEthereumApiCall<MockEthEnvironment> {
	fn new_unsigned(transfer_param: TransferAssetParams<Ethereum>) -> Result<Self, DispatchError> {
		if MockEthEnvironment::lookup(transfer_param.asset).is_none() {
			Err(DispatchError::CannotLookup)
		} else {
			Ok(Self::TransferFallback(MockEthTransferFallback {
				nonce: Default::default(),
				transfer_param,
				_phantom: PhantomData,
			}))
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Eq, PartialEq)]
pub struct MockBtcEnvironment;

impl ChainEnvironment<<Bitcoin as Chain>::ChainAsset, <Bitcoin as Chain>::ChainAccount>
	for MockBtcEnvironment
{
	fn lookup(_asset: <Bitcoin as Chain>::ChainAsset) -> Option<<Bitcoin as Chain>::ChainAccount> {
		Some(cf_chains::btc::ScriptPubkey::Taproot([16u8; 32]))
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub enum MockBitcoinApiCall<MockBtcEnvironment> {
	AllBatch(MockBtcAllBatch<MockBtcEnvironment>),
	ExecutexSwapAndCall(MockBtcExecutexSwapAndCall<MockBtcEnvironment>),
	TransferFallback(MockBtcTransferFallback<MockBtcEnvironment>),
}

impl ApiCall<BitcoinCrypto> for MockBitcoinApiCall<MockBtcEnvironment> {
	fn threshold_signature_payload(&self) -> <BitcoinCrypto as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<BitcoinCrypto as ChainCrypto>::ThresholdSignature,
	) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}

	fn transaction_out_id(&self) -> <BitcoinCrypto as ChainCrypto>::TransactionOutId {
		unimplemented!()
	}
}

impl cf_chains::ConsolidateCall<Bitcoin> for MockBitcoinApiCall<MockBtcEnvironment> {
	fn consolidate_utxos() -> Result<Self, cf_chains::ConsolidationError> {
		if SHOULD_CONSOLIDATE.with(|cell| cell.get()) {
			Ok(Self::AllBatch(MockBtcAllBatch {
				fetch_params: Default::default(),
				transfer_params: Default::default(),
				_phantom: PhantomData,
			}))
		} else {
			Err(ConsolidationError::NotRequired)
		}
	}
}
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub struct MockBtcTransferFallback<MockBtcEnvironment> {
	_phantom: PhantomData<MockBtcEnvironment>,
}

impl TransferFallback<Bitcoin> for MockBitcoinApiCall<MockBtcEnvironment> {
	fn new_unsigned(_transfer_param: TransferAssetParams<Bitcoin>) -> Result<Self, DispatchError> {
		Err(DispatchError::CannotLookup)
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub struct MockBtcExecutexSwapAndCall<MockBtcEnvironment> {
	transfer_param: TransferAssetParams<Bitcoin>,
	source_chain: ForeignChain,
	source_address: Option<ForeignChainAddress>,
	gas_budget: <Bitcoin as Chain>::ChainAmount,
	message: Vec<u8>,
	_phantom: PhantomData<MockBtcEnvironment>,
}

impl ExecutexSwapAndCall<Bitcoin> for MockBitcoinApiCall<MockBtcEnvironment> {
	fn new_unsigned(
		transfer_param: TransferAssetParams<Bitcoin>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: <Bitcoin as Chain>::ChainAmount,
		message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		if MockBtcEnvironment::lookup(transfer_param.asset).is_none() {
			Err(DispatchError::CannotLookup)
		} else {
			Ok(Self::ExecutexSwapAndCall(MockBtcExecutexSwapAndCall {
				transfer_param,
				source_chain,
				source_address,
				gas_budget,
				message,
				_phantom: PhantomData,
			}))
		}
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Default, Eq, Encode, Decode, TypeInfo)]
pub struct MockBtcAllBatch<MockBtcEnvironment> {
	pub fetch_params: Vec<FetchAssetParams<Bitcoin>>,
	pub transfer_params: Vec<TransferAssetParams<Bitcoin>>,
	_phantom: PhantomData<MockBtcEnvironment>,
}

impl MockBtcAllBatch<MockBtcEnvironment> {
	pub fn set_success(success: bool) {
		ALL_BATCH_SUCCESS.with(|cell| *cell.borrow_mut() = success);
	}
}

impl AllBatch<Bitcoin> for MockBitcoinApiCall<MockBtcEnvironment> {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Bitcoin>>,
		transfer_params: Vec<TransferAssetParams<Bitcoin>>,
	) -> Result<Self, AllBatchError> {
		if ALL_BATCH_SUCCESS.with(|cell| *cell.borrow()) {
			Ok(Self::AllBatch(MockBtcAllBatch {
				fetch_params,
				transfer_params,
				_phantom: PhantomData,
			}))
		} else {
			Err(AllBatchError::UnsupportedToken)
		}
	}
}
