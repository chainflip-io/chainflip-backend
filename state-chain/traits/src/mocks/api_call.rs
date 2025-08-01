// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use core::marker::PhantomData;

use cf_chains::{
	btc::BitcoinCrypto, ccm_checker::DecodedCcmAdditionalData, evm::EvmCrypto, AllBatch,
	AllBatchError, ApiCall, Bitcoin, Chain, ChainCrypto, ChainEnvironment, ConsolidationError,
	Ethereum, ExecutexSwapAndCall, ExecutexSwapAndCallError, FetchAssetParams, ForeignChainAddress,
	RejectCall, RejectError, TransferAssetParams, TransferFallback, TransferFallbackError,
};
use cf_primitives::{chains::assets, EgressId, ForeignChain, GasAmount};
use codec::{Decode, Encode};
use frame_support::{sp_runtime::DispatchError, CloneNoBound, DebugNoBound, PartialEqNoBound};
use scale_info::TypeInfo;

pub const ETHEREUM_ETH_ADDRESS: [u8; 20] = [0xee; 20];
pub const ETHEREUM_FLIP_ADDRESS: [u8; 20] = [0xcf; 20];
pub const ETHEREUM_USDC_ADDRESS: [u8; 20] = [0x45; 20];
pub const ETHEREUM_USDT_ADDRESS: [u8; 20] = [0xba; 20];
#[derive(Encode, Decode, TypeInfo, Eq, PartialEq)]
pub struct MockEvmEnvironment;

impl ChainEnvironment<<Ethereum as Chain>::ChainAsset, <Ethereum as Chain>::ChainAccount>
	for MockEvmEnvironment
{
	fn lookup(asset: <Ethereum as Chain>::ChainAsset) -> Option<<Ethereum as Chain>::ChainAccount> {
		match asset {
			assets::eth::Asset::Eth => Some(ETHEREUM_ETH_ADDRESS.into()),
			assets::eth::Asset::Flip => Some(ETHEREUM_FLIP_ADDRESS.into()),
			assets::eth::Asset::Usdc => Some(ETHEREUM_USDC_ADDRESS.into()),
			assets::eth::Asset::Usdt => Some(ETHEREUM_USDT_ADDRESS.into()),
		}
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub enum MockEthereumApiCall<MockEvmEnvironment> {
	AllBatch(MockEthAllBatch<MockEvmEnvironment>),
	ExecutexSwapAndCall(MockEthExecutexSwapAndCall<MockEvmEnvironment>),
	TransferFallback(MockEthTransferFallback<MockEvmEnvironment>),
	RejectCall {
		deposit_details: <Ethereum as Chain>::DepositDetails,
		refund_address: <Ethereum as Chain>::ChainAccount,
		refund_amount: Option<<Ethereum as Chain>::ChainAmount>,
		asset: <Ethereum as Chain>::ChainAsset,
		deposit_fetch_id: Option<<Ethereum as Chain>::DepositFetchId>,
	},
}

impl ApiCall<EvmCrypto> for MockEthereumApiCall<MockEvmEnvironment> {
	fn threshold_signature_payload(&self) -> <EvmCrypto as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<EvmCrypto as ChainCrypto>::ThresholdSignature,
		_signer: <EvmCrypto as ChainCrypto>::AggKey,
	) -> Self {
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

	fn refresh_replay_protection(&mut self) {
		unimplemented!()
	}

	fn signer(&self) -> Option<<EvmCrypto as ChainCrypto>::AggKey> {
		unimplemented!()
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Default, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthAllBatch<MockEvmEnvironment> {
	pub nonce: <Ethereum as Chain>::ReplayProtection,
	pub fetch_params: Vec<FetchAssetParams<Ethereum>>,
	pub transfer_params: Vec<TransferAssetParams<Ethereum>>,
	_phantom: PhantomData<MockEvmEnvironment>,
}

impl MockEthAllBatch<MockEvmEnvironment> {
	pub fn set_success(success: bool) {
		ALL_BATCH_SUCCESS.with(|cell| *cell.borrow_mut() = success);
	}
}

thread_local! {
	static ALL_BATCH_SUCCESS: std::cell::RefCell<bool> = const { std::cell::RefCell::new(true) };
	pub static SHOULD_CONSOLIDATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl AllBatch<Ethereum> for MockEthereumApiCall<MockEvmEnvironment> {
	fn new_unsigned_impl(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<(TransferAssetParams<Ethereum>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError> {
		let (transfer_params, egress_ids) = transfer_params.into_iter().unzip();
		if ALL_BATCH_SUCCESS.with(|cell| *cell.borrow()) {
			Ok(vec![(
				Self::AllBatch(MockEthAllBatch {
					nonce: Default::default(),
					fetch_params,
					transfer_params,
					_phantom: PhantomData,
				}),
				egress_ids,
			)])
		} else {
			Err(AllBatchError::UnsupportedToken)
		}
	}
}

impl cf_chains::ConsolidateCall<Ethereum> for MockEthereumApiCall<MockEvmEnvironment> {
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
pub struct MockEthExecutexSwapAndCall<MockEvmEnvironment> {
	nonce: <Ethereum as Chain>::ReplayProtection,
	transfer_param: TransferAssetParams<Ethereum>,
	source_chain: ForeignChain,
	source_address: Option<ForeignChainAddress>,
	gas_budget: GasAmount,
	message: Vec<u8>,
	_phantom: PhantomData<MockEvmEnvironment>,
}

impl ExecutexSwapAndCall<Ethereum> for MockEthereumApiCall<MockEvmEnvironment> {
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<Ethereum>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: GasAmount,
		message: Vec<u8>,
		_ccm_additional_data: DecodedCcmAdditionalData,
	) -> Result<Self, ExecutexSwapAndCallError> {
		if MockEvmEnvironment::lookup(transfer_param.asset).is_none() {
			Err(ExecutexSwapAndCallError::DispatchError(DispatchError::CannotLookup))
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
pub struct MockEthTransferFallback<MockEvmEnvironment> {
	nonce: <Ethereum as Chain>::ReplayProtection,
	transfer_param: TransferAssetParams<Ethereum>,
	_phantom: PhantomData<MockEvmEnvironment>,
}

impl TransferFallback<Ethereum> for MockEthereumApiCall<MockEvmEnvironment> {
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<Ethereum>,
	) -> Result<Self, TransferFallbackError> {
		if MockEvmEnvironment::lookup(transfer_param.asset).is_none() {
			Err(TransferFallbackError::CannotLookupTokenAddress)
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
	RejectCall,
}

impl ApiCall<BitcoinCrypto> for MockBitcoinApiCall<MockBtcEnvironment> {
	fn threshold_signature_payload(&self) -> <BitcoinCrypto as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<BitcoinCrypto as ChainCrypto>::ThresholdSignature,
		_signer: <BitcoinCrypto as ChainCrypto>::AggKey,
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

	fn refresh_replay_protection(&mut self) {
		unimplemented!()
	}

	fn signer(&self) -> Option<<BitcoinCrypto as ChainCrypto>::AggKey> {
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
	fn new_unsigned_impl(
		_transfer_param: TransferAssetParams<Bitcoin>,
	) -> Result<Self, TransferFallbackError> {
		Err(TransferFallbackError::Unsupported)
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub struct MockBtcExecutexSwapAndCall<MockBtcEnvironment> {
	transfer_param: TransferAssetParams<Bitcoin>,
	source_chain: ForeignChain,
	source_address: Option<ForeignChainAddress>,
	gas_budget: GasAmount,
	message: Vec<u8>,
	_phantom: PhantomData<MockBtcEnvironment>,
}

impl ExecutexSwapAndCall<Bitcoin> for MockBitcoinApiCall<MockBtcEnvironment> {
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<Bitcoin>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: GasAmount,
		message: Vec<u8>,
		_ccm_additional_data: DecodedCcmAdditionalData,
	) -> Result<Self, ExecutexSwapAndCallError> {
		if MockBtcEnvironment::lookup(transfer_param.asset).is_none() {
			Err(ExecutexSwapAndCallError::DispatchError(DispatchError::CannotLookup))
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
	fn new_unsigned_impl(
		fetch_params: Vec<FetchAssetParams<Bitcoin>>,
		transfer_params: Vec<(TransferAssetParams<Bitcoin>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError> {
		let (transfer_params, egress_ids) = transfer_params.into_iter().unzip();
		if ALL_BATCH_SUCCESS.with(|cell| *cell.borrow()) {
			Ok(vec![(
				Self::AllBatch(MockBtcAllBatch {
					fetch_params,
					transfer_params,
					_phantom: PhantomData,
				}),
				egress_ids,
			)])
		} else {
			Err(AllBatchError::UnsupportedToken)
		}
	}
}

impl RejectCall<Bitcoin> for MockBitcoinApiCall<MockBtcEnvironment> {
	fn new_unsigned(
		_deposit_details: <Bitcoin as Chain>::DepositDetails,
		_refund_address: <Bitcoin as Chain>::ChainAccount,
		_refund_amount: Option<<Bitcoin as Chain>::ChainAmount>,
		_asset: <Bitcoin as Chain>::ChainAsset,
		_deposit_fetch_id: Option<<Bitcoin as Chain>::DepositFetchId>,
	) -> Result<Self, RejectError> {
		Ok(Self::RejectCall)
	}
}

impl RejectCall<Ethereum> for MockEthereumApiCall<MockEvmEnvironment> {
	fn new_unsigned(
		deposit_details: <Ethereum as Chain>::DepositDetails,
		refund_address: <Ethereum as Chain>::ChainAccount,
		refund_amount: Option<<Ethereum as Chain>::ChainAmount>,
		asset: <Ethereum as Chain>::ChainAsset,
		deposit_fetch_id: Option<<Ethereum as Chain>::DepositFetchId>,
	) -> Result<Self, RejectError> {
		Ok(Self::RejectCall {
			deposit_details,
			refund_address,
			refund_amount,
			asset,
			deposit_fetch_id,
		})
	}
}
