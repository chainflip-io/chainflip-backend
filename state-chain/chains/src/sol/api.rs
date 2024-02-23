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
pub enum SolanaApi<Env> {
	AllBatch {
		fetch_params: vec::Vec<FetchAssetParams<Solana>>,
		transfer_params: vec::Vec<TransferAssetParams<Solana>>,
	},
	SetAggKeyWithAggKey {
		maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::AggKey>,
		new_key: <SolanaCrypto as ChainCrypto>::AggKey,
	},
	TransferFallback {
		transfer_param: TransferAssetParams<Solana>,
	},
	ExecutexSwapAndCall {
		transfer_param: TransferAssetParams<Solana>,
		source_chain: cf_primitives::ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: <Solana as Chain>::ChainAmount,
		message: vec::Vec<u8>,
	},
	ApiCall {
		threshold_signature: <SolanaCrypto as ChainCrypto>::ThresholdSignature,
	},

	_PhantomData(PhantomData<Env>),
}

impl<Env: 'static> ApiCall<SolanaCrypto> for SolanaApi<Env> {
	fn threshold_signature_payload(&self) -> <SolanaCrypto as ChainCrypto>::Payload {
		[]
	}

	fn signed(
		self,
		threshold_signature: &<SolanaCrypto as ChainCrypto>::ThresholdSignature,
	) -> Self {
		Self::ApiCall { threshold_signature: *threshold_signature }
	}

	fn chain_encoded(&self) -> vec::Vec<u8> {
		vec![]
	}

	fn is_signed(&self) -> bool {
		todo!()
	}

	fn transaction_out_id(&self) -> <SolanaCrypto as ChainCrypto>::TransactionOutId {
		todo!()
	}
}

impl<Env: 'static> ConsolidateCall<Solana> for SolanaApi<Env> {
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		Err(ConsolidationError::NotRequired)
	}
}

impl<Env: 'static> SetAggKeyWithAggKey<SolanaCrypto> for SolanaApi<Env> {
	fn new_unsigned(
		maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::AggKey>,
		new_key: <SolanaCrypto as ChainCrypto>::AggKey,
	) -> Result<Self, crate::SetAggKeyWithAggKeyError> {
		Ok(Self::SetAggKeyWithAggKey { maybe_old_key, new_key })
	}
}

impl<Env: 'static> ExecutexSwapAndCall<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		transfer_param: TransferAssetParams<Solana>,
		source_chain: cf_primitives::ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: <Solana as Chain>::ChainAmount,
		message: vec::Vec<u8>,
	) -> Result<Self, DispatchError> {
		Ok(Self::ExecutexSwapAndCall {
			transfer_param,
			source_chain,
			source_address,
			gas_budget,
			message,
		})
	}
}

impl<Env: 'static> AllBatch<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		fetch_params: vec::Vec<FetchAssetParams<Solana>>,
		transfer_params: vec::Vec<TransferAssetParams<Solana>>,
	) -> Result<Self, AllBatchError> {
		Ok(Self::AllBatch { fetch_params, transfer_params })
	}
}

impl<Env: 'static> TransferFallback<Solana> for SolanaApi<Env> {
	fn new_unsigned(transfer_param: TransferAssetParams<Solana>) -> Result<Self, DispatchError> {
		Ok(Self::TransferFallback { transfer_param })
	}
}
