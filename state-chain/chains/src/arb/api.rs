use crate::{
	evm::api::{
		common::EncodableTransferAssetParams, evm_all_batch_builder, execute_x_swap_and_call,
		EvmEnvironmentProvider, EvmReplayProtection, EvmTransactionBuilder,
	},
	*,
};
use evm::api::{all_batch, set_agg_key_with_agg_key};
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_std::marker::PhantomData;

use self::evm::{api::transfer_fallback, EvmCrypto};

/// Chainflip api calls available on Arbitrum.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum ArbitrumApi<Environment: 'static> {
	SetAggKeyWithAggKey(EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
	AllBatch(EvmTransactionBuilder<all_batch::AllBatch>),
	ExecutexSwapAndCall(EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
	TransferFallback(EvmTransactionBuilder<transfer_fallback::TransferFallback>),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

impl<E> SetAggKeyWithAggKey<EvmCrypto> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum> + ReplayProtectionProvider<Arbitrum>,
{
	fn new_unsigned(
		_old_key: Option<<EvmCrypto as ChainCrypto>::AggKey>,
		new_key: <EvmCrypto as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, SetAggKeyWithAggKeyError> {
		Ok(Some(Self::SetAggKeyWithAggKey(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::key_manager_address()),
			set_agg_key_with_agg_key::SetAggKeyWithAggKey::new(new_key),
		))))
	}
}

impl<E> AllBatch<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum> + ReplayProtectionProvider<Arbitrum>,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Arbitrum>>,
		transfer_params: Vec<TransferAssetParams<Arbitrum>>,
	) -> Result<Self, AllBatchError> {
		Ok(Self::AllBatch(evm_all_batch_builder(
			fetch_params,
			transfer_params,
			E::token_address,
			E::replay_protection(E::vault_address()),
		)?))
	}
}

impl<E> ExecutexSwapAndCall<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum> + ReplayProtectionProvider<Arbitrum>,
{
	fn new_unsigned(
		transfer_param: TransferAssetParams<Arbitrum>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: <Arbitrum as Chain>::ChainAmount,
		message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		let transfer_param = EncodableTransferAssetParams {
			asset: E::token_address(transfer_param.asset).ok_or(DispatchError::CannotLookup)?,
			to: transfer_param.to,
			amount: transfer_param.amount,
		};

		Ok(Self::ExecutexSwapAndCall(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::vault_address()),
			execute_x_swap_and_call::ExecutexSwapAndCall::new(
				transfer_param,
				source_chain,
				source_address,
				gas_budget,
				message,
			),
		)))
	}
}

impl<E> TransferFallback<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum> + ReplayProtectionProvider<Arbitrum>,
{
	fn new_unsigned(transfer_param: TransferAssetParams<Arbitrum>) -> Result<Self, DispatchError> {
		let transfer_param = EncodableTransferAssetParams {
			asset: E::token_address(transfer_param.asset).ok_or(DispatchError::CannotLookup)?,
			to: transfer_param.to,
			amount: transfer_param.amount,
		};

		Ok(Self::TransferFallback(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::vault_address()),
			transfer_fallback::TransferFallback::new(transfer_param),
		)))
	}
}

impl<E> ConsolidateCall<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum> + ReplayProtectionProvider<Arbitrum>,
{
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		Err(ConsolidationError::NotRequired)
	}
}

impl<E> From<EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>>
	for ArbitrumApi<E>
{
	fn from(tx: EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>) -> Self {
		Self::SetAggKeyWithAggKey(tx)
	}
}

impl<E> From<EvmTransactionBuilder<all_batch::AllBatch>> for ArbitrumApi<E> {
	fn from(tx: EvmTransactionBuilder<all_batch::AllBatch>) -> Self {
		Self::AllBatch(tx)
	}
}

impl<E> From<EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>>
	for ArbitrumApi<E>
{
	fn from(tx: EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>) -> Self {
		Self::ExecutexSwapAndCall(tx)
	}
}

impl<E> From<EvmTransactionBuilder<transfer_fallback::TransferFallback>> for ArbitrumApi<E> {
	fn from(tx: EvmTransactionBuilder<transfer_fallback::TransferFallback>) -> Self {
		Self::TransferFallback(tx)
	}
}

macro_rules! map_over_api_variants {
	( $self:expr, $var:pat_param, $var_method:expr $(,)* ) => {
		match $self {
			ArbitrumApi::SetAggKeyWithAggKey($var) => $var_method,
			ArbitrumApi::AllBatch($var) => $var_method,
			ArbitrumApi::ExecutexSwapAndCall($var) => $var_method,
			ArbitrumApi::TransferFallback($var) => $var_method,
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	};
}

impl<E> ArbitrumApi<E> {
	pub fn replay_protection(&self) -> EvmReplayProtection {
		map_over_api_variants!(self, call, call.replay_protection())
	}
}

impl<E> ApiCall<EvmCrypto> for ArbitrumApi<E> {
	fn threshold_signature_payload(&self) -> <EvmCrypto as ChainCrypto>::Payload {
		map_over_api_variants!(self, call, call.threshold_signature_payload())
	}

	fn signed(self, threshold_signature: &<EvmCrypto as ChainCrypto>::ThresholdSignature) -> Self {
		map_over_api_variants!(self, call, call.signed(threshold_signature).into())
	}

	fn chain_encoded(&self) -> Vec<u8> {
		map_over_api_variants!(self, call, call.chain_encoded())
	}

	fn is_signed(&self) -> bool {
		map_over_api_variants!(self, call, call.is_signed())
	}

	fn transaction_out_id(&self) -> <EvmCrypto as ChainCrypto>::TransactionOutId {
		map_over_api_variants!(self, call, call.transaction_out_id())
	}

	fn refresh_replay_protection(&mut self) {
		map_over_api_variants!(self, call, call.refresh_replay_protection())
	}
}

impl<E> ArbitrumApi<E> {
	pub fn gas_budget(&self) -> Option<<Arbitrum as Chain>::ChainAmount> {
		map_over_api_variants!(self, call, call.gas_budget())
	}
}
