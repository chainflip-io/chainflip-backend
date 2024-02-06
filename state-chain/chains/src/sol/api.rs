use core::marker::PhantomData;

use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::DispatchError, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_std::vec;

use crate::{
	AllBatch, AllBatchError, ApiCall, Chain, ChainCrypto, ConsolidateCall, ConsolidationError,
	ExecutexSwapAndCall, FetchAssetParams, ForeignChainAddress, SetAggKeyWithAggKey,
	TransferAssetParams, TransferFallback,
};

use super::{Solana, SolanaCrypto};

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Env))]
pub struct SolanaApi<Env>(PhantomData<Env>);

impl<Env: 'static> ApiCall<SolanaCrypto> for SolanaApi<Env> {
	fn threshold_signature_payload(&self) -> <SolanaCrypto as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<SolanaCrypto as ChainCrypto>::ThresholdSignature,
	) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> vec::Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}

	fn transaction_out_id(&self) -> <SolanaCrypto as ChainCrypto>::TransactionOutId {
		unimplemented!()
	}
}

impl<Env: 'static> ConsolidateCall<Solana> for SolanaApi<Env> {
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		unimplemented!()
	}
}

impl<Env: 'static> SetAggKeyWithAggKey<SolanaCrypto> for SolanaApi<Env> {
	fn new_unsigned(
		_maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::AggKey>,
		_new_key: <SolanaCrypto as ChainCrypto>::AggKey,
	) -> Result<Self, crate::SetAggKeyWithAggKeyError> {
		unimplemented!()
	}
}

impl<Env: 'static> ExecutexSwapAndCall<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		_transfer_param: TransferAssetParams<Solana>,
		_source_chain: cf_primitives::ForeignChain,
		_source_address: Option<ForeignChainAddress>,
		_gas_budget: <Solana as Chain>::ChainAmount,
		_message: vec::Vec<u8>,
	) -> Result<Self, DispatchError> {
		unimplemented!()
	}
}

impl<Env: 'static> AllBatch<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		_fetch_params: vec::Vec<FetchAssetParams<Solana>>,
		_transfer_params: vec::Vec<TransferAssetParams<Solana>>,
	) -> Result<Self, AllBatchError> {
		unimplemented!()
	}
}

impl<Env: 'static> TransferFallback<Solana> for SolanaApi<Env> {
	fn new_unsigned(_transfer_param: TransferAssetParams<Solana>) -> Result<Self, DispatchError> {
		unimplemented!()
	}
}
