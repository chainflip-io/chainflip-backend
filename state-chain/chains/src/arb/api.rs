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

use self::evm::{api::transfer_fallback, Address, EvmCrypto};

/// Chainflip api calls available on Arbitrum.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum ArbitrumApi<Environment: 'static> {
	SetAggKeyWithAggKey(EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
	AllBatch(EvmTransactionBuilder<all_batch::AllBatch>),
	ExecutexSwapAndCall(EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
	TransferFallback(EvmTransactionBuilder<transfer_fallback::TransferFallback>),
	RejectCall(EvmTransactionBuilder<all_batch::AllBatch>),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

impl<E> SetAggKeyWithAggKey<EvmCrypto> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum> + ReplayProtectionProvider<Arbitrum>,
{
	fn new_unsigned_impl(
		_maybe_old_key: Option<<EvmCrypto as ChainCrypto>::AggKey>,
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
	fn new_unsigned_impl(
		fetch_params: Vec<FetchAssetParams<Arbitrum>>,
		transfer_params: Vec<(TransferAssetParams<Arbitrum>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError> {
		let (transfer_params, egress_ids) = transfer_params.iter().cloned().unzip();
		Ok(vec![(
			Self::AllBatch(evm_all_batch_builder(
				fetch_params,
				transfer_params,
				E::token_address,
				E::replay_protection(E::vault_address()),
			)?),
			egress_ids,
		)])
	}
}

impl<E> ExecutexSwapAndCall<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum> + ReplayProtectionProvider<Arbitrum>,
{
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<Arbitrum>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: GasAmount,
		message: Vec<u8>,
		_ccm_additional_data: DecodedCcmAdditionalData,
	) -> Result<Self, ExecutexSwapAndCallError> {
		let transfer_param = EncodableTransferAssetParams {
			asset: E::token_address(transfer_param.asset)
				.ok_or(ExecutexSwapAndCallError::DispatchError(DispatchError::CannotLookup))?,
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
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<Arbitrum>,
	) -> Result<Self, TransferFallbackError> {
		let transfer_param = EncodableTransferAssetParams {
			asset: E::token_address(transfer_param.asset)
				.ok_or(TransferFallbackError::CannotLookupTokenAddress)?,
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

impl<E> RejectCall<Arbitrum> for ArbitrumApi<E>
where
	E: EvmEnvironmentProvider<Arbitrum> + ReplayProtectionProvider<Arbitrum>,
{
	fn new_unsigned(
		_deposit_details: <Arbitrum as Chain>::DepositDetails,
		refund_address: <Arbitrum as Chain>::ChainAccount,
		refund_amount: Option<<Arbitrum as Chain>::ChainAmount>,
		asset: <Arbitrum as Chain>::ChainAsset,
		deposit_fetch_id: Option<<Arbitrum as Chain>::DepositFetchId>,
	) -> Result<Self, RejectError> {
		match evm_all_batch_builder::<Arbitrum, _>(
			deposit_fetch_id
				.map(|id| vec![FetchAssetParams { deposit_fetch_id: id, asset }])
				.unwrap_or_default(),
			refund_amount
				.map(|amount| TransferAssetParams { asset, amount, to: refund_address })
				.into_iter()
				.collect(),
			E::token_address,
			E::replay_protection(E::vault_address()),
		) {
			Ok(builder) => Ok(Self::RejectCall(builder)),
			Err(AllBatchError::NotRequired) => Err(RejectError::from(AllBatchError::NotRequired)),
			Err(err) => Err(RejectError::from(err)),
		}
	}
}

macro_rules! map_over_api_variants {
	( $self:expr, $var:pat_param, $var_method:expr $(,)* ) => {
		match $self {
			ArbitrumApi::SetAggKeyWithAggKey($var) => $var_method,
			ArbitrumApi::AllBatch($var) => $var_method,
			ArbitrumApi::ExecutexSwapAndCall($var) => $var_method,
			ArbitrumApi::TransferFallback($var) => $var_method,
			ArbitrumApi::RejectCall($var) => $var_method,
			ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	};
}

impl<E> ArbitrumApi<E> {
	pub fn replay_protection(&self) -> EvmReplayProtection {
		map_over_api_variants!(self, call, call.replay_protection())
	}
}

impl<E: ReplayProtectionProvider<Arbitrum> + EvmEnvironmentProvider<Arbitrum>> ApiCall<EvmCrypto>
	for ArbitrumApi<E>
{
	fn threshold_signature_payload(&self) -> <EvmCrypto as ChainCrypto>::Payload {
		map_over_api_variants!(self, call, call.threshold_signature_payload())
	}

	fn signed(
		self,
		threshold_signature: &<EvmCrypto as ChainCrypto>::ThresholdSignature,
		signer: <EvmCrypto as ChainCrypto>::AggKey,
	) -> Self {
		map_over_api_variants!(self, call, call.signed(threshold_signature, signer).into())
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
		let EvmReplayProtection { contract_address, .. } =
			map_over_api_variants!(self, call, call.replay_protection());
		let new_replay_protection = E::replay_protection(contract_address);
		map_over_api_variants!(self, call, call.refresh_replay_protection(new_replay_protection))
	}

	fn signer(&self) -> Option<<EvmCrypto as ChainCrypto>::AggKey> {
		map_over_api_variants!(self, call, call.signer_and_sig_data).map(|(signer, _)| signer)
	}
}

impl<E> ArbitrumApi<E> {
	pub fn ccm_transfer_data(&self) -> Option<(GasAmount, usize, Address)> {
		map_over_api_variants!(self, call, call.ccm_transfer_data())
	}
}
