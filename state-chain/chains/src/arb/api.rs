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

use self::evm::EvmCrypto;

/// Chainflip api calls available on Ethereum.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum ArbitrumApi<Environment: 'static> {
	SetAggKeyWithAggKey(EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
	AllBatch(EvmTransactionBuilder<all_batch::AllBatch>),
	ExecutexSwapAndCall(EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
	#[doc(hidden)]
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
	) -> Result<Self, SetAggKeyWithAggKeyError> {
		Ok(Self::SetAggKeyWithAggKey(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::key_manager_address()),
			set_agg_key_with_agg_key::SetAggKeyWithAggKey::new(new_key),
		)))
	}
}

impl<E> AllBatch<EvmCrypto> for ArbitrumApi<E>
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

impl<E> ExecutexSwapAndCall<EvmCrypto> for ArbitrumApi<E>
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

impl<E> ArbitrumApi<E> {
	pub fn replay_protection(&self) -> EvmReplayProtection {
		match self {
			ArbitrumApi::SetAggKeyWithAggKey(tx) => tx.replay_protection(),
			ArbitrumApi::AllBatch(tx) => tx.replay_protection(),
			ArbitrumApi::ExecutexSwapAndCall(tx) => tx.replay_protection(),
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	}
}

impl<E> ApiCall<EvmCrypto> for ArbitrumApi<E> {
	fn threshold_signature_payload(&self) -> <EvmCrypto as ChainCrypto>::Payload {
		match self {
			ArbitrumApi::SetAggKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			ArbitrumApi::AllBatch(tx) => tx.threshold_signature_payload(),
			ArbitrumApi::ExecutexSwapAndCall(tx) => tx.threshold_signature_payload(),
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(self, threshold_signature: &<EvmCrypto as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			ArbitrumApi::SetAggKeyWithAggKey(call) => call.signed(threshold_signature).into(),
			ArbitrumApi::AllBatch(call) => call.signed(threshold_signature).into(),
			ArbitrumApi::ExecutexSwapAndCall(call) => call.signed(threshold_signature).into(),
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			ArbitrumApi::SetAggKeyWithAggKey(call) => call.chain_encoded(),
			ArbitrumApi::AllBatch(call) => call.chain_encoded(),
			ArbitrumApi::ExecutexSwapAndCall(call) => call.chain_encoded(),
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			ArbitrumApi::SetAggKeyWithAggKey(call) => call.is_signed(),
			ArbitrumApi::AllBatch(call) => call.is_signed(),
			ArbitrumApi::ExecutexSwapAndCall(call) => call.is_signed(),
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn transaction_out_id(&self) -> <EvmCrypto as ChainCrypto>::TransactionOutId {
		match self {
			ArbitrumApi::SetAggKeyWithAggKey(call) => call.transaction_out_id(),
			ArbitrumApi::AllBatch(call) => call.transaction_out_id(),
			ArbitrumApi::ExecutexSwapAndCall(call) => call.transaction_out_id(),
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	}
}
