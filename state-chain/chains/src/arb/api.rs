use crate::{
	eth::api::execute_x_swap_and_call,
	evm::{
		api::{
			common::EncodableTransferAssetParams, evm_all_batch_builder,
			EthereumTransactionBuilder, EvmReplayProtection,
		},
		EvmEnvironmentProvider,
	},
	*,
};
use eth::api::{all_batch, set_agg_key_with_agg_key};
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_std::{cmp::min, marker::PhantomData};

use super::ArbitrumContract;

impl ChainAbi for Arbitrum {
	type Transaction = eth::Transaction;
	type ReplayProtection = EvmReplayProtection;
}

impl FeeRefundCalculator<Arbitrum> for eth::Transaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Arbitrum as Chain>::TransactionFee,
	) -> <Arbitrum as Chain>::ChainAmount {
		min(
			self.max_fee_per_gas
				.unwrap_or_default()
				.try_into()
				.expect("In practice `max_fee_per_gas` is always less than u128::MAX"),
			fee_paid.effective_gas_price,
		)
		.saturating_mul(fee_paid.gas_used)
	}
}

/// Chainflip api calls available on Ethereum.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum ArbitrumApi<Environment: 'static> {
	SetAggKeyWithAggKey(EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
	AllBatch(EthereumTransactionBuilder<all_batch::AllBatch>),
	ExecutexSwapAndCall(EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
	#[doc(hidden)]
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

impl<E> SetAggKeyWithAggKey<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum, Contract = ArbitrumContract>,
{
	fn new_unsigned(
		_old_key: Option<<Arbitrum as ChainCrypto>::AggKey>,
		new_key: <Arbitrum as ChainCrypto>::AggKey,
	) -> Result<Self, SetAggKeyWithAggKeyError> {
		Ok(Self::SetAggKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(ArbitrumContract::KeyManager),
			set_agg_key_with_agg_key::SetAggKeyWithAggKey::new(new_key),
		)))
	}
}

impl<E> AllBatch<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum, Contract = ArbitrumContract>,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Arbitrum>>,
		transfer_params: Vec<TransferAssetParams<Arbitrum>>,
	) -> Result<Self, AllBatchError> {
		Ok(Self::AllBatch(evm_all_batch_builder(
			fetch_params,
			transfer_params,
			E::token_address,
			E::replay_protection(ArbitrumContract::Vault),
		)?))
	}
}

impl<E> ExecutexSwapAndCall<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum, Contract = ArbitrumContract>,
{
	fn new_unsigned(
		egress_id: EgressId,
		transfer_param: TransferAssetParams<Arbitrum>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		let transfer_param = EncodableTransferAssetParams {
			asset: E::token_address(transfer_param.asset).ok_or(DispatchError::CannotLookup)?,
			to: transfer_param.to,
			amount: transfer_param.amount,
		};

		Ok(Self::ExecutexSwapAndCall(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(ArbitrumContract::Vault),
			execute_x_swap_and_call::ExecutexSwapAndCall::new(
				egress_id,
				transfer_param,
				source_chain,
				source_address,
				message,
			),
		)))
	}
}

impl<E> From<EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>>
	for ArbitrumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>) -> Self {
		Self::SetAggKeyWithAggKey(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<all_batch::AllBatch>> for ArbitrumApi<E> {
	fn from(tx: EthereumTransactionBuilder<all_batch::AllBatch>) -> Self {
		Self::AllBatch(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>>
	for ArbitrumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>) -> Self {
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

impl<E> ApiCall<Arbitrum> for ArbitrumApi<E> {
	fn threshold_signature_payload(&self) -> <Arbitrum as ChainCrypto>::Payload {
		match self {
			ArbitrumApi::SetAggKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			ArbitrumApi::AllBatch(tx) => tx.threshold_signature_payload(),
			ArbitrumApi::ExecutexSwapAndCall(tx) => tx.threshold_signature_payload(),
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(self, threshold_signature: &<Arbitrum as ChainCrypto>::ThresholdSignature) -> Self {
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

	fn transaction_out_id(&self) -> <Arbitrum as ChainCrypto>::TransactionOutId {
		match self {
			ArbitrumApi::SetAggKeyWithAggKey(call) => call.transaction_out_id(),
			ArbitrumApi::AllBatch(call) => call.transaction_out_id(),
			ArbitrumApi::ExecutexSwapAndCall(call) => call.transaction_out_id(),
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	}
}
