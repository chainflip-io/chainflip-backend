use core::marker::PhantomData;

use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::DispatchError, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_std::vec;

use crate::{
	sol::{SolAddress, SolAmount, SolHash, SolanaCrypto},
	AllBatch, AllBatchError, ApiCall, Chain, ChainCrypto, ChainEnvironment, ConsolidateCall,
	ConsolidationError, ExecutexSwapAndCall, FetchAssetParams, ForeignChainAddress,
	SetAggKeyWithAggKey, Solana, TransferAssetParams, TransferFallback,
};

mod batch_fetch;

/// Super trait combining all Environment lookups required for the Solana chain.
pub trait SolanaEnvironment:
	ChainEnvironment<SolanaEnvAccountLookupKey, SolAddress>
	+ ChainEnvironment<(), SolAmount>
	+ ChainEnvironment<(), SolHash>
{
	fn compute_price() -> Option<SolAmount> {
		<Self as ChainEnvironment<(), SolAmount>>::lookup(())
	}

	fn durable_nonce() -> Option<SolHash> {
		<Self as ChainEnvironment<(), SolHash>>::lookup(())
	}

	fn lookup_account(key: SolanaEnvAccountLookupKey) -> Option<SolAddress> {
		<Self as ChainEnvironment<SolanaEnvAccountLookupKey, SolAddress>>::lookup(key)
	}
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
/// This is the Key type used in Solana Environment to look up Account type (Pubkey).
pub enum SolanaEnvAccountLookupKey {
	AggKey,
	AvailableNonceAccount,
	VaultProgramDataAccount,
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Env))]
pub enum SolanaApi<Env: 'static> {
	BatchFetch(batch_fetch::BatchFetches<Env>),
	Transfer,
	SetAggKeyWithAggKey {
		maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::AggKey>,
		new_key: <SolanaCrypto as ChainCrypto>::AggKey,
	},
	ExecutexSwapAndCall {
		transfer_param: TransferAssetParams<Solana>,
		source_chain: cf_primitives::ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: <Solana as Chain>::ChainAmount,
		message: vec::Vec<u8>,
	},
	#[doc(hidden)]
	#[codec(skip)]
	_PhantomData(PhantomData<Env>),
}

impl<Env: 'static> ApiCall<SolanaCrypto> for SolanaApi<Env> {
	fn threshold_signature_payload(&self) -> <SolanaCrypto as ChainCrypto>::Payload {
		todo!()
	}

	fn signed(
		self,
		_threshold_signature: &<SolanaCrypto as ChainCrypto>::ThresholdSignature,
	) -> Self {
		todo!()
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
	) -> Result<Option<Self>, crate::SetAggKeyWithAggKeyError> {
		Ok(Some(Self::SetAggKeyWithAggKey { maybe_old_key, new_key }))
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
		_fetch_params: vec::Vec<FetchAssetParams<Solana>>,
		_transfer_params: vec::Vec<TransferAssetParams<Solana>>,
	) -> Result<Self, AllBatchError> {
		Err(AllBatchError::NotRequired)
	}
}

impl<Env: 'static> TransferFallback<Solana> for SolanaApi<Env> {
	fn new_unsigned(_transfer_param: TransferAssetParams<Solana>) -> Result<Self, DispatchError> {
		Err(DispatchError::Other("Solana does not support TransferFallback."))
	}
}
