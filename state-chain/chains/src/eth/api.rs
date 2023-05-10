use ethabi::Address;
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_runtime::{traits::UniqueSaturatedInto, DispatchError};
use sp_std::marker::PhantomData;

use crate::*;

use self::all_batch::{
	EncodableFetchAssetParams, EncodableFetchDeployAssetParams, EncodableTransferAssetParams,
};

use super::{Ethereum, EthereumChannelId, EthereumTransactionBuilder};

pub mod all_batch;
pub mod execute_x_swap_and_call;
pub mod register_redemption;
pub mod set_agg_key_with_agg_key;
pub mod set_comm_key_with_agg_key;
pub mod set_gov_key_with_agg_key;
pub mod update_flip_supply;

/// Chainflip api calls available on Ethereum.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum EthereumApi<Environment: 'static> {
	SetAggKeyWithAggKey(EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
	RegisterRedemption(EthereumTransactionBuilder<register_redemption::RegisterRedemption>),
	UpdateFlipSupply(EthereumTransactionBuilder<update_flip_supply::UpdateFlipSupply>),
	SetGovKeyWithAggKey(EthereumTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>),
	SetCommKeyWithAggKey(
		EthereumTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>,
	),
	AllBatch(EthereumTransactionBuilder<all_batch::AllBatch>),
	ExecutexSwapAndCall(EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
pub struct EthereumReplayProtection {
	pub nonce: u64,
	pub chain_id: EthereumChainId,
	pub key_manager_address: Address,
	pub contract_address: Address,
}

/// Provides the environment data for ethereum-like chains.
pub trait EthEnvironmentProvider {
	fn token_address(asset: assets::eth::Asset) -> Option<eth::Address>;
	fn contract_address(contract: EthereumContract) -> eth::Address;
	fn chain_id() -> EthereumChainId;
	fn next_nonce() -> u64;

	fn replay_protection(contract: EthereumContract) -> EthereumReplayProtection {
		EthereumReplayProtection {
			nonce: Self::next_nonce(),
			chain_id: Self::chain_id(),
			key_manager_address: Self::key_manager_address(),
			contract_address: Self::contract_address(contract),
		}
	}

	fn key_manager_address() -> eth::Address {
		Self::contract_address(EthereumContract::KeyManager)
	}

	fn state_chain_gateway_address() -> eth::Address {
		Self::contract_address(EthereumContract::StateChainGateway)
	}

	fn vault_address() -> eth::Address {
		Self::contract_address(EthereumContract::Vault)
	}
}

impl ChainAbi for Ethereum {
	type Transaction = eth::Transaction;
	type ReplayProtection = EthereumReplayProtection;
}

impl<E> SetAggKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(
		_old_key: Option<<Ethereum as ChainCrypto>::AggKey>,
		new_key: <Ethereum as ChainCrypto>::AggKey,
	) -> Result<Self, ()> {
		Ok(Self::SetAggKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::KeyManager),
			set_agg_key_with_agg_key::SetAggKeyWithAggKey::new(new_key),
		)))
	}
}

impl<E> SetGovKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(
		_maybe_old_key: Option<<Ethereum as ChainCrypto>::GovKey>,
		new_gov_key: <Ethereum as ChainCrypto>::GovKey,
	) -> Result<Self, ()> {
		Ok(Self::SetGovKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::KeyManager),
			set_gov_key_with_agg_key::SetGovKeyWithAggKey::new(new_gov_key),
		)))
	}
}

impl<E> SetCommKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(new_comm_key: <Ethereum as ChainCrypto>::GovKey) -> Self {
		Self::SetCommKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::KeyManager),
			set_comm_key_with_agg_key::SetCommKeyWithAggKey::new(new_comm_key),
		))
	}
}

impl<E> RegisterRedemption<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(node_id: &[u8; 32], amount: u128, address: &[u8; 20], expiry: u64) -> Self {
		Self::RegisterRedemption(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::StateChainGateway),
			register_redemption::RegisterRedemption::new(node_id, amount, address, expiry),
		))
	}

	fn amount(&self) -> u128 {
		match self {
			EthereumApi::RegisterRedemption(tx_builder) =>
				tx_builder.call.amount.unique_saturated_into(),
			_ => unreachable!(),
		}
	}
}

impl<E> UpdateFlipSupply<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(new_total_supply: u128, block_number: u64) -> Self {
		Self::UpdateFlipSupply(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::StateChainGateway),
			update_flip_supply::UpdateFlipSupply::new(new_total_supply, block_number),
		))
	}
}

impl<E> AllBatch<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Result<Self, ()> {
		let mut fetch_only_params = vec![];
		let mut fetch_deploy_params = vec![];
		for FetchAssetParams { deposit_fetch_id, asset } in fetch_params {
			if let Some(token_address) = E::token_address(asset) {
				match deposit_fetch_id {
					EthereumChannelId::Deployed(contract_address) => fetch_only_params
						.push(EncodableFetchAssetParams { contract_address, asset: token_address }),
					EthereumChannelId::UnDeployed(channel_id) => fetch_deploy_params
						.push(EncodableFetchDeployAssetParams { channel_id, asset: token_address }),
				};
			} else {
				return Err(())
			}
		}
		Ok(Self::AllBatch(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::Vault),
			all_batch::AllBatch::new(
				fetch_deploy_params,
				fetch_only_params,
				transfer_params
					.into_iter()
					.map(|TransferAssetParams { asset, to, amount }| {
						E::token_address(asset)
							.map(|address| all_batch::EncodableTransferAssetParams {
								to,
								amount,
								asset: address,
							})
							.ok_or(())
					})
					.collect::<Result<Vec<_>, ()>>()?,
			),
		)))
	}
}

impl<E> ExecutexSwapAndCall<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(
		egress_id: EgressId,
		transfer_param: TransferAssetParams<Ethereum>,
		source_address: ForeignChainAddress,
		message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		let transfer_param = EncodableTransferAssetParams {
			asset: E::token_address(transfer_param.asset).ok_or(DispatchError::CannotLookup)?,
			to: transfer_param.to,
			amount: transfer_param.amount,
		};

		Ok(Self::ExecutexSwapAndCall(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::Vault),
			execute_x_swap_and_call::ExecutexSwapAndCall::new(
				egress_id,
				transfer_param,
				source_address,
				message,
			),
		)))
	}
}

impl<E> From<EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>) -> Self {
		Self::SetAggKeyWithAggKey(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<register_redemption::RegisterRedemption>>
	for EthereumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<register_redemption::RegisterRedemption>) -> Self {
		Self::RegisterRedemption(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<update_flip_supply::UpdateFlipSupply>> for EthereumApi<E> {
	fn from(tx: EthereumTransactionBuilder<update_flip_supply::UpdateFlipSupply>) -> Self {
		Self::UpdateFlipSupply(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>) -> Self {
		Self::SetGovKeyWithAggKey(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(
		tx: EthereumTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>,
	) -> Self {
		Self::SetCommKeyWithAggKey(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<all_batch::AllBatch>> for EthereumApi<E> {
	fn from(tx: EthereumTransactionBuilder<all_batch::AllBatch>) -> Self {
		Self::AllBatch(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>>
	for EthereumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>) -> Self {
		Self::ExecutexSwapAndCall(tx)
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
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	};
}

impl<E> ApiCall<Ethereum> for EthereumApi<E> {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		map_over_api_variants!(self, call, call.threshold_signature_payload())
	}

	fn signed(self, threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		map_over_api_variants!(self, call, call.signed(threshold_signature).into())
	}

	fn chain_encoded(&self) -> Vec<u8> {
		map_over_api_variants!(self, call, call.chain_encoded())
	}

	fn is_signed(&self) -> bool {
		map_over_api_variants!(self, call, call.is_signed())
	}
}

pub(super) fn ethabi_function(name: &'static str, params: Vec<ethabi::Param>) -> ethabi::Function {
	#[allow(deprecated)]
	ethabi::Function {
		name: name.into(),
		inputs: params,
		outputs: vec![],
		constant: None,
		state_mutability: ethabi::StateMutability::NonPayable,
	}
}

pub(super) fn ethabi_param(name: &'static str, param_type: ethabi::ParamType) -> ethabi::Param {
	ethabi::Param { name: name.into(), kind: param_type, internal_type: None }
}

#[macro_export]
macro_rules! impl_api_call_eth {
	($call:ident) => {
		impl ApiCall<Ethereum> for $call {
			fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
				self.signature_handler.payload
			}

			fn signed(mut self, signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
				self.signature_handler.insert_signature(signature);
				self
			}

			fn chain_encoded(&self) -> Vec<u8> {
				self.abi_encoded()
			}

			fn is_signed(&self) -> bool {
				self.signature_handler.is_signed()
			}
		}
	};
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum EthereumContract {
	StateChainGateway,
	KeyManager,
	Vault,
}

pub type EthereumChainId = u64;
