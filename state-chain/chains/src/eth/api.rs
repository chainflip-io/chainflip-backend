use ethabi::Address;
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_runtime::{traits::UniqueSaturatedInto, DispatchError};
use sp_std::marker::PhantomData;

use crate::*;

use self::all_batch::{
	EncodableFetchAssetParams, EncodableFetchDeployAssetParams, EncodableTransferAssetParams,
};

use super::{Ethereum, EthereumIngressId};

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
	SetAggKeyWithAggKey(set_agg_key_with_agg_key::SetAggKeyWithAggKey),
	RegisterRedemption(register_redemption::RegisterRedemption),
	UpdateFlipSupply(update_flip_supply::UpdateFlipSupply),
	SetGovKeyWithAggKey(set_gov_key_with_agg_key::SetGovKeyWithAggKey),
	SetCommKeyWithAggKey(set_comm_key_with_agg_key::SetCommKeyWithAggKey),
	AllBatch(all_batch::AllBatch),
	ExecutexSwapAndCall(execute_x_swap_and_call::ExecutexSwapAndCall),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
pub struct EthereumReplayProtection {
	pub key_manager_address: [u8; 20],
	pub chain_id: u64,
	pub nonce: u64,
}

impl ChainAbi for Ethereum {
	type Transaction = eth::Transaction;
	type ReplayProtection = EthereumReplayProtection;
}

impl<E: ReplayProtectionProvider<Ethereum>> SetAggKeyWithAggKey<Ethereum> for EthereumApi<E> {
	fn new_unsigned(
		_old_key: Option<<Ethereum as ChainCrypto>::AggKey>,
		new_key: <Ethereum as ChainCrypto>::AggKey,
	) -> Result<Self, ()> {
		Ok(Self::SetAggKeyWithAggKey(set_agg_key_with_agg_key::SetAggKeyWithAggKey::new_unsigned(
			E::replay_protection(),
			new_key,
		)))
	}
}

impl<E: ReplayProtectionProvider<Ethereum>> SetGovKeyWithAggKey<Ethereum> for EthereumApi<E> {
	fn new_unsigned(
		_maybe_old_key: Option<<Ethereum as ChainCrypto>::GovKey>,
		new_gov_key: <Ethereum as ChainCrypto>::GovKey,
	) -> Result<Self, ()> {
		Ok(Self::SetGovKeyWithAggKey(set_gov_key_with_agg_key::SetGovKeyWithAggKey::new_unsigned(
			E::replay_protection(),
			new_gov_key,
		)))
	}
}

impl<E: ReplayProtectionProvider<Ethereum>> SetCommKeyWithAggKey<Ethereum> for EthereumApi<E> {
	fn new_unsigned(new_comm_key: eth::Address) -> Self {
		Self::SetCommKeyWithAggKey(set_comm_key_with_agg_key::SetCommKeyWithAggKey::new_unsigned(
			E::replay_protection(),
			new_comm_key,
		))
	}
}

impl<E: ReplayProtectionProvider<Ethereum>> RegisterRedemption<Ethereum> for EthereumApi<E> {
	fn new_unsigned(node_id: &[u8; 32], amount: u128, address: &[u8; 20], expiry: u64) -> Self {
		Self::RegisterRedemption(register_redemption::RegisterRedemption::new_unsigned(
			E::replay_protection(),
			node_id,
			amount,
			address,
			expiry,
		))
	}

	fn amount(&self) -> u128 {
		match self {
			EthereumApi::RegisterRedemption(call) => call.amount.unique_saturated_into(),
			_ => unreachable!(),
		}
	}
}

impl<E: ReplayProtectionProvider<Ethereum>> UpdateFlipSupply<Ethereum> for EthereumApi<E> {
	fn new_unsigned(
		new_total_supply: u128,
		block_number: u64,
		state_chain_gateway_address: &[u8; 20],
	) -> Self {
		Self::UpdateFlipSupply(update_flip_supply::UpdateFlipSupply::new_unsigned(
			E::replay_protection(),
			new_total_supply,
			block_number,
			state_chain_gateway_address,
		))
	}
}

impl<E> AllBatch<Ethereum> for EthereumApi<E>
where
	E: ChainEnvironment<assets::eth::Asset, Address>,
	E: ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Result<Self, ()> {
		let mut fetch_only_params = vec![];
		let mut fetch_deploy_params = vec![];
		for FetchAssetParams { ingress_fetch_id, asset } in fetch_params {
			if let Some(token_address) = E::lookup(asset) {
				match ingress_fetch_id {
					EthereumIngressId::Deployed(contract_address) => fetch_only_params
						.push(EncodableFetchAssetParams { contract_address, asset: token_address }),
					EthereumIngressId::UnDeployed(intent_id) => fetch_deploy_params
						.push(EncodableFetchDeployAssetParams { intent_id, asset: token_address }),
				};
			} else {
				return Err(())
			}
		}
		Ok(Self::AllBatch(all_batch::AllBatch::new_unsigned(
			E::replay_protection(),
			fetch_deploy_params,
			fetch_only_params,
			transfer_params
				.into_iter()
				.map(|TransferAssetParams { asset, to, amount }| {
					E::lookup(asset)
						.map(|address| all_batch::EncodableTransferAssetParams {
							to,
							amount,
							asset: address,
						})
						.ok_or(())
				})
				.collect::<Result<Vec<_>, ()>>()?,
		)))
	}
}

impl<E> ExecutexSwapAndCall<Ethereum> for EthereumApi<E>
where
	E: ChainEnvironment<assets::eth::Asset, Address>,
	E: ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		egress_id: EgressId,
		transfer_param: TransferAssetParams<Ethereum>,
		source_address: ForeignChainAddress,
		message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		let transfer_param = EncodableTransferAssetParams {
			asset: E::lookup(transfer_param.asset).ok_or(DispatchError::CannotLookup)?,
			to: transfer_param.to,
			amount: transfer_param.amount,
		};

		Ok(Self::ExecutexSwapAndCall(execute_x_swap_and_call::ExecutexSwapAndCall::new_unsigned(
			E::replay_protection(),
			egress_id,
			transfer_param,
			source_address,
			message,
		)))
	}
}

impl<E> From<set_agg_key_with_agg_key::SetAggKeyWithAggKey> for EthereumApi<E> {
	fn from(tx: set_agg_key_with_agg_key::SetAggKeyWithAggKey) -> Self {
		Self::SetAggKeyWithAggKey(tx)
	}
}

impl<E> From<register_redemption::RegisterRedemption> for EthereumApi<E> {
	fn from(tx: register_redemption::RegisterRedemption) -> Self {
		Self::RegisterRedemption(tx)
	}
}

impl<E> From<update_flip_supply::UpdateFlipSupply> for EthereumApi<E> {
	fn from(tx: update_flip_supply::UpdateFlipSupply) -> Self {
		Self::UpdateFlipSupply(tx)
	}
}

impl<E> From<set_gov_key_with_agg_key::SetGovKeyWithAggKey> for EthereumApi<E> {
	fn from(tx: set_gov_key_with_agg_key::SetGovKeyWithAggKey) -> Self {
		Self::SetGovKeyWithAggKey(tx)
	}
}

impl<E> From<set_comm_key_with_agg_key::SetCommKeyWithAggKey> for EthereumApi<E> {
	fn from(tx: set_comm_key_with_agg_key::SetCommKeyWithAggKey) -> Self {
		Self::SetCommKeyWithAggKey(tx)
	}
}

impl<E> From<all_batch::AllBatch> for EthereumApi<E> {
	fn from(tx: all_batch::AllBatch) -> Self {
		Self::AllBatch(tx)
	}
}

impl<E> From<execute_x_swap_and_call::ExecutexSwapAndCall> for EthereumApi<E> {
	fn from(tx: execute_x_swap_and_call::ExecutexSwapAndCall) -> Self {
		Self::ExecutexSwapAndCall(tx)
	}
}

impl<E> ApiCall<Ethereum> for EthereumApi<E> {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		match self {
			EthereumApi::SetAggKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::RegisterRedemption(tx) => tx.threshold_signature_payload(),
			EthereumApi::UpdateFlipSupply(tx) => tx.threshold_signature_payload(),
			EthereumApi::SetGovKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::SetCommKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::AllBatch(tx) => tx.threshold_signature_payload(),
			EthereumApi::ExecutexSwapAndCall(tx) => tx.threshold_signature_payload(),
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(self, threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.signed(threshold_signature).into(),
			EthereumApi::RegisterRedemption(call) => call.signed(threshold_signature).into(),
			EthereumApi::UpdateFlipSupply(call) => call.signed(threshold_signature).into(),
			EthereumApi::SetGovKeyWithAggKey(call) => call.signed(threshold_signature).into(),
			EthereumApi::SetCommKeyWithAggKey(call) => call.signed(threshold_signature).into(),
			EthereumApi::AllBatch(call) => call.signed(threshold_signature).into(),
			EthereumApi::ExecutexSwapAndCall(call) => call.signed(threshold_signature).into(),
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::RegisterRedemption(call) => call.chain_encoded(),
			EthereumApi::UpdateFlipSupply(call) => call.chain_encoded(),
			EthereumApi::SetGovKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::SetCommKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::AllBatch(call) => call.chain_encoded(),
			EthereumApi::ExecutexSwapAndCall(call) => call.chain_encoded(),
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.is_signed(),
			EthereumApi::RegisterRedemption(call) => call.is_signed(),
			EthereumApi::UpdateFlipSupply(call) => call.is_signed(),
			EthereumApi::SetGovKeyWithAggKey(call) => call.is_signed(),
			EthereumApi::SetCommKeyWithAggKey(call) => call.is_signed(),
			EthereumApi::AllBatch(call) => call.is_signed(),
			EthereumApi::ExecutexSwapAndCall(call) => call.is_signed(),
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	}
}

fn ethabi_function(name: &'static str, params: Vec<ethabi::Param>) -> ethabi::Function {
	#[allow(deprecated)]
	ethabi::Function {
		name: name.into(),
		inputs: params,
		outputs: vec![],
		constant: None,
		state_mutability: ethabi::StateMutability::NonPayable,
	}
}

fn ethabi_param(name: &'static str, param_type: ethabi::ParamType) -> ethabi::Param {
	ethabi::Param { name: name.into(), kind: param_type, internal_type: None }
}

#[macro_export]
macro_rules! impl_api_call_eth {
	($call:ident) => {
		impl ApiCall<Ethereum> for $call {
			fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
				self.sig_data.msg_hash
			}

			fn signed(mut self, signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
				self.sig_data.insert_signature(signature);
				self
			}

			fn chain_encoded(&self) -> Vec<u8> {
				self.abi_encoded()
			}

			fn is_signed(&self) -> bool {
				self.sig_data.is_signed()
			}
		}
	};
}
