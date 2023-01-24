use ethabi::Address;
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::marker::PhantomData;

use crate::*;

use super::Ethereum;

pub mod all_batch;
pub mod register_claim;
pub mod set_agg_key_with_agg_key;
pub mod set_comm_key_with_agg_key;
pub mod set_gov_key_with_agg_key;
pub mod update_flip_supply;

/// Chainflip api calls available on Ethereum.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum EthereumApi<Environment: 'static> {
	SetAggKeyWithAggKey(set_agg_key_with_agg_key::SetAggKeyWithAggKey),
	RegisterClaim(register_claim::RegisterClaim),
	UpdateFlipSupply(update_flip_supply::UpdateFlipSupply),
	SetGovKeyWithAggKey(set_gov_key_with_agg_key::SetGovKeyWithAggKey),
	SetCommKeyWithAggKey(set_comm_key_with_agg_key::SetCommKeyWithAggKey),
	AllBatch(all_batch::AllBatch),
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
	fn new_unsigned(_maybe_old_key: Option<Vec<u8>>, new_gov_key: Vec<u8>) -> Result<Self, ()> {
		let slice: [u8; 20] = new_gov_key.try_into().expect("to have a valid length");
		Ok(Self::SetGovKeyWithAggKey(set_gov_key_with_agg_key::SetGovKeyWithAggKey::new_unsigned(
			E::replay_protection(),
			slice.into(),
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

impl<E: ReplayProtectionProvider<Ethereum>> RegisterClaim<Ethereum> for EthereumApi<E> {
	fn new_unsigned(node_id: &[u8; 32], amount: u128, address: &[u8; 20], expiry: u64) -> Self {
		Self::RegisterClaim(register_claim::RegisterClaim::new_unsigned(
			E::replay_protection(),
			node_id,
			amount,
			address,
			expiry,
		))
	}

	fn amount(&self) -> u128 {
		match self {
			EthereumApi::RegisterClaim(call) => call.amount.unique_saturated_into(),
			_ => unreachable!(),
		}
	}
}

impl<E: ReplayProtectionProvider<Ethereum>> UpdateFlipSupply<Ethereum> for EthereumApi<E> {
	fn new_unsigned(
		new_total_supply: u128,
		block_number: u64,
		stake_manager_address: &[u8; 20],
	) -> Self {
		Self::UpdateFlipSupply(update_flip_supply::UpdateFlipSupply::new_unsigned(
			E::replay_protection(),
			new_total_supply,
			block_number,
			stake_manager_address,
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
		Ok(Self::AllBatch(all_batch::AllBatch::new_unsigned(
			E::replay_protection(),
			fetch_params
				.into_iter()
				.map(|FetchAssetParams { intent_id, asset }| {
					E::lookup(asset)
						.map(|address| all_batch::EncodableFetchAssetParams {
							intent_id,
							asset: address,
						})
						.ok_or(())
				})
				.collect::<Result<Vec<_>, ()>>()?,
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

impl<E> From<set_agg_key_with_agg_key::SetAggKeyWithAggKey> for EthereumApi<E> {
	fn from(tx: set_agg_key_with_agg_key::SetAggKeyWithAggKey) -> Self {
		Self::SetAggKeyWithAggKey(tx)
	}
}

impl<E> From<register_claim::RegisterClaim> for EthereumApi<E> {
	fn from(tx: register_claim::RegisterClaim) -> Self {
		Self::RegisterClaim(tx)
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

impl<E> ApiCall<Ethereum> for EthereumApi<E> {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		match self {
			EthereumApi::SetAggKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::RegisterClaim(tx) => tx.threshold_signature_payload(),
			EthereumApi::UpdateFlipSupply(tx) => tx.threshold_signature_payload(),
			EthereumApi::SetGovKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::SetCommKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::AllBatch(tx) => tx.threshold_signature_payload(),
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(self, threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.signed(threshold_signature).into(),
			EthereumApi::RegisterClaim(call) => call.signed(threshold_signature).into(),
			EthereumApi::UpdateFlipSupply(call) => call.signed(threshold_signature).into(),
			EthereumApi::SetGovKeyWithAggKey(call) => call.signed(threshold_signature).into(),
			EthereumApi::SetCommKeyWithAggKey(call) => call.signed(threshold_signature).into(),
			EthereumApi::AllBatch(call) => call.signed(threshold_signature).into(),
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::RegisterClaim(call) => call.chain_encoded(),
			EthereumApi::UpdateFlipSupply(call) => call.chain_encoded(),
			EthereumApi::SetGovKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::SetCommKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::AllBatch(call) => call.chain_encoded(),
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.is_signed(),
			EthereumApi::RegisterClaim(call) => call.is_signed(),
			EthereumApi::UpdateFlipSupply(call) => call.is_signed(),
			EthereumApi::SetGovKeyWithAggKey(call) => call.is_signed(),
			EthereumApi::SetCommKeyWithAggKey(call) => call.is_signed(),
			EthereumApi::AllBatch(call) => call.is_signed(),
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
