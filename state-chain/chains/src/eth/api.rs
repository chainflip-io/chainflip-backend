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

use super::Ethereum;
use crate::{
	evm::{
		api::{
			all_batch, evm_all_batch_builder, execute_x_swap_and_call, set_agg_key_with_agg_key,
			set_comm_key_with_agg_key, set_gov_key_with_agg_key, transfer_fallback, EvmCall,
			EvmEnvironmentProvider, EvmReplayProtection, EvmTransactionBuilder,
		},
		EvmCrypto,
	},
	RejectCall, *,
};
use ethabi::{Address, Uint};
use evm::api::common::*;
use frame_support::{
	sp_runtime::DispatchError, CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound,
};
use sp_std::marker::PhantomData;

use evm::tokenizable::Tokenizable;

#[cfg(feature = "std")]
pub mod abi {
	#[macro_export]
	macro_rules! include_abi_bytes {
		($name:ident) => {
			&include_bytes!(concat!(
				env!("CF_ETH_CONTRACT_ABI_ROOT"),
				"/",
				env!("CF_ETH_CONTRACT_ABI_TAG"),
				"/",
				stringify!($name),
				".json"
			))[..]
		};
	}

	#[cfg(test)]
	pub fn load_abi(name: &'static str) -> ethabi::Contract {
		fn abi_file(name: &'static str) -> std::path::PathBuf {
			let mut path = std::path::PathBuf::from(env!("CF_ETH_CONTRACT_ABI_ROOT"));
			path.push(env!("CF_ETH_CONTRACT_ABI_TAG"));
			path.push(name);
			path.set_extension("json");
			path.canonicalize()
				.unwrap_or_else(|e| panic!("Failed to canonicalize abi file {path:?}: {e}"))
		}

		fn load_abi_bytes(name: &'static str) -> impl std::io::Read {
			std::fs::File::open(abi_file(name))
				.unwrap_or_else(|e| panic!("Failed to open abi file {:?}: {e}", abi_file(name)))
		}

		ethabi::Contract::load(load_abi_bytes(name)).expect("Failed to load abi from bytes.")
	}
}

pub mod register_redemption;
pub mod update_flip_supply;

/// Chainflip api calls available on Ethereum.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum EthereumApi<Environment: 'static> {
	SetAggKeyWithAggKey(EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
	RegisterRedemption(EvmTransactionBuilder<register_redemption::RegisterRedemption>),
	UpdateFlipSupply(EvmTransactionBuilder<update_flip_supply::UpdateFlipSupply>),
	SetGovKeyWithAggKey(EvmTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>),
	SetCommKeyWithAggKey(EvmTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>),
	AllBatch(EvmTransactionBuilder<all_batch::AllBatch>),
	ExecutexSwapAndCall(EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
	TransferFallback(EvmTransactionBuilder<transfer_fallback::TransferFallback>),
	RejectCall(EvmTransactionBuilder<all_batch::AllBatch>),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

impl<E> SetAggKeyWithAggKey<EvmCrypto> for EthereumApi<E>
where
	E: EvmEnvironmentProvider<Ethereum> + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned_impl(
		_old_key: Option<<EvmCrypto as ChainCrypto>::AggKey>,
		new_key: <EvmCrypto as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, SetAggKeyWithAggKeyError> {
		Ok(Some(Self::SetAggKeyWithAggKey(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::key_manager_address()),
			set_agg_key_with_agg_key::SetAggKeyWithAggKey::new(new_key),
		))))
	}
}

impl<E> SetGovKeyWithAggKey<EvmCrypto> for EthereumApi<E>
where
	E: EvmEnvironmentProvider<Ethereum> + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned_impl(
		_maybe_old_key: Option<<EvmCrypto as ChainCrypto>::GovKey>,
		new_gov_key: <EvmCrypto as ChainCrypto>::GovKey,
	) -> Result<Self, SetGovKeyWithAggKeyError> {
		Ok(Self::SetGovKeyWithAggKey(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::key_manager_address()),
			set_gov_key_with_agg_key::SetGovKeyWithAggKey::new(new_gov_key),
		)))
	}
}

impl<E> SetCommKeyWithAggKey<EvmCrypto> for EthereumApi<E>
where
	E: EvmEnvironmentProvider<Ethereum> + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(new_comm_key: <EvmCrypto as ChainCrypto>::GovKey) -> Self {
		Self::SetCommKeyWithAggKey(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::key_manager_address()),
			set_comm_key_with_agg_key::SetCommKeyWithAggKey::new(new_comm_key),
		))
	}
}

impl<E> RegisterRedemption for EthereumApi<E>
where
	E: StateChainGatewayAddressProvider
		+ EvmEnvironmentProvider<Ethereum>
		+ ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		node_id: &[u8; 32],
		amount: u128,
		address: &[u8; 20],
		expiry: u64,
		executor: Option<Address>,
	) -> Self {
		Self::RegisterRedemption(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::state_chain_gateway_address()),
			register_redemption::RegisterRedemption::new(
				node_id, amount, address, expiry, executor,
			),
		))
	}
}

impl<E> UpdateFlipSupply<EvmCrypto> for EthereumApi<E>
where
	E: StateChainGatewayAddressProvider
		+ EvmEnvironmentProvider<Ethereum>
		+ ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(new_total_supply: u128, block_number: u64) -> Self {
		Self::UpdateFlipSupply(EvmTransactionBuilder::new_unsigned(
			E::replay_protection(E::state_chain_gateway_address()),
			update_flip_supply::UpdateFlipSupply::new(new_total_supply, block_number),
		))
	}
}

impl<E> ConsolidateCall<Ethereum> for EthereumApi<E>
where
	E: EvmEnvironmentProvider<Ethereum> + ReplayProtectionProvider<Ethereum>,
{
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		Err(ConsolidationError::NotRequired)
	}
}

impl<E> AllBatch<Ethereum> for EthereumApi<E>
where
	E: EvmEnvironmentProvider<Ethereum> + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned_impl(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<(TransferAssetParams<Ethereum>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError> {
		let (transfer_params, egress_ids) = transfer_params.into_iter().unzip();

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

impl<E> ExecutexSwapAndCall<Ethereum> for EthereumApi<E>
where
	E: EvmEnvironmentProvider<Ethereum> + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<Ethereum>,
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

impl<E> TransferFallback<Ethereum> for EthereumApi<E>
where
	E: EvmEnvironmentProvider<Ethereum> + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<Ethereum>,
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

impl<E> RejectCall<Ethereum> for EthereumApi<E>
where
	E: EvmEnvironmentProvider<Ethereum> + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		_deposit_details: <Ethereum as Chain>::DepositDetails,
		refund_address: <Ethereum as Chain>::ChainAccount,
		refund_amount: Option<<Ethereum as Chain>::ChainAmount>,
		asset: <Ethereum as Chain>::ChainAsset,
		deposit_fetch_id: Option<<Ethereum as Chain>::DepositFetchId>,
	) -> Result<Self, RejectError> {
		match evm_all_batch_builder::<Ethereum, _>(
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

impl<E> From<EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(tx: EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>) -> Self {
		Self::SetAggKeyWithAggKey(tx)
	}
}

impl<E> From<EvmTransactionBuilder<register_redemption::RegisterRedemption>> for EthereumApi<E> {
	fn from(tx: EvmTransactionBuilder<register_redemption::RegisterRedemption>) -> Self {
		Self::RegisterRedemption(tx)
	}
}

impl<E> From<EvmTransactionBuilder<update_flip_supply::UpdateFlipSupply>> for EthereumApi<E> {
	fn from(tx: EvmTransactionBuilder<update_flip_supply::UpdateFlipSupply>) -> Self {
		Self::UpdateFlipSupply(tx)
	}
}

impl<E> From<EvmTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(tx: EvmTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>) -> Self {
		Self::SetGovKeyWithAggKey(tx)
	}
}

impl<E> From<EvmTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(tx: EvmTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>) -> Self {
		Self::SetCommKeyWithAggKey(tx)
	}
}

impl<E> From<EvmTransactionBuilder<all_batch::AllBatch>> for EthereumApi<E> {
	fn from(tx: EvmTransactionBuilder<all_batch::AllBatch>) -> Self {
		Self::AllBatch(tx)
	}
}

impl<E> From<EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>>
	for EthereumApi<E>
{
	fn from(tx: EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>) -> Self {
		Self::ExecutexSwapAndCall(tx)
	}
}

impl<E> From<EvmTransactionBuilder<transfer_fallback::TransferFallback>> for EthereumApi<E> {
	fn from(tx: EvmTransactionBuilder<transfer_fallback::TransferFallback>) -> Self {
		Self::TransferFallback(tx)
	}
}

macro_rules! map_over_api_variants {
	( $self:expr, $var:pat_param, $var_method:expr $(,)* ) => {
		match $self {
			EthereumApi::SetAggKeyWithAggKey($var) => $var_method,
			EthereumApi::RegisterRedemption($var) => $var_method,
			EthereumApi::UpdateFlipSupply($var) => $var_method,
			EthereumApi::SetGovKeyWithAggKey($var) => $var_method,
			EthereumApi::SetCommKeyWithAggKey($var) => $var_method,
			EthereumApi::AllBatch($var) => $var_method,
			EthereumApi::ExecutexSwapAndCall($var) => $var_method,
			EthereumApi::TransferFallback($var) => $var_method,
			EthereumApi::RejectCall($var) => $var_method,
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	};
}

impl<E> EthereumApi<E> {
	pub fn replay_protection(&self) -> EvmReplayProtection {
		map_over_api_variants!(self, call, call.replay_protection())
	}
}

impl<E: ReplayProtectionProvider<Ethereum> + EvmEnvironmentProvider<Ethereum>> ApiCall<EvmCrypto>
	for EthereumApi<E>
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

impl<E> EthereumApi<E> {
	pub fn ccm_transfer_data(&self) -> Option<(GasAmount, usize, Address)> {
		map_over_api_variants!(self, call, call.ccm_transfer_data())
	}
}

pub trait StateChainGatewayAddressProvider {
	fn state_chain_gateway_address() -> Address;
}
