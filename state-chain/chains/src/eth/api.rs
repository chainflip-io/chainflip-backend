use sp_runtime::traits::UniqueSaturatedInto;

use crate::*;

pub mod register_claim;
pub mod set_agg_key_with_agg_key;
pub mod update_flip_supply;

/// Chainflip api calls available on Ethereum.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum EthereumApi {
	SetAggKeyWithAggKey(set_agg_key_with_agg_key::SetAggKeyWithAggKey),
	RegisterClaim(register_claim::RegisterClaim),
	UpdateFlipSupply(update_flip_supply::UpdateFlipSupply),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
pub struct EthereumReplayProtection {
	pub key_manager_address: [u8; 20],
	pub chain_id: u64,
	pub nonce: u64,
}

impl ChainAbi for Ethereum {
	type UnsignedTransaction = eth::UnsignedTransaction;
	type SignedTransaction = eth::RawSignedTransaction;
	type SignerCredential = eth::Address;
	type ReplayProtection = EthereumReplayProtection;
	type ValidationError = eth::TransactionVerificationError;

	fn verify_signed_transaction(
		unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
		signer_credential: &Self::SignerCredential,
	) -> Result<(), Self::ValidationError> {
		eth::verify_transaction(unsigned_tx, signed_tx, signer_credential)
	}
}

impl SetAggKeyWithAggKey<Ethereum> for EthereumApi {
	fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		new_key: <Ethereum as ChainCrypto>::AggKey,
	) -> Self {
		Self::SetAggKeyWithAggKey(set_agg_key_with_agg_key::SetAggKeyWithAggKey::new_unsigned(
			replay_protection,
			new_key,
		))
	}
}

impl RegisterClaim<Ethereum> for EthereumApi {
	fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		node_id: &[u8; 32],
		amount: u128,
		address: &[u8; 20],
		expiry: u64,
	) -> Self {
		Self::RegisterClaim(register_claim::RegisterClaim::new_unsigned(
			replay_protection,
			node_id,
			amount,
			address,
			expiry,
		))
	}

	fn amount(&self) -> u128 {
		match self {
			EthereumApi::SetAggKeyWithAggKey(_) => 0,
			EthereumApi::RegisterClaim(call) => call.amount.unique_saturated_into(),
			EthereumApi::UpdateFlipSupply(_) => 0,
		}
	}
}

impl UpdateFlipSupply<Ethereum> for EthereumApi {
	fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		new_total_supply: u128,
		block_number: u64,
		stake_manager_address: &[u8; 20],
	) -> Self {
		Self::UpdateFlipSupply(update_flip_supply::UpdateFlipSupply::new_unsigned(
			replay_protection,
			new_total_supply,
			block_number,
			stake_manager_address,
		))
	}
}

impl From<set_agg_key_with_agg_key::SetAggKeyWithAggKey> for EthereumApi {
	fn from(tx: set_agg_key_with_agg_key::SetAggKeyWithAggKey) -> Self {
		Self::SetAggKeyWithAggKey(tx)
	}
}

impl From<register_claim::RegisterClaim> for EthereumApi {
	fn from(tx: register_claim::RegisterClaim) -> Self {
		Self::RegisterClaim(tx)
	}
}

impl From<update_flip_supply::UpdateFlipSupply> for EthereumApi {
	fn from(tx: update_flip_supply::UpdateFlipSupply) -> Self {
		Self::UpdateFlipSupply(tx)
	}
}

impl ApiCall<Ethereum> for EthereumApi {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		match self {
			EthereumApi::SetAggKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::RegisterClaim(tx) => tx.threshold_signature_payload(),
			EthereumApi::UpdateFlipSupply(tx) => tx.threshold_signature_payload(),
		}
	}

	fn signed(self, threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.signed(threshold_signature).into(),
			EthereumApi::RegisterClaim(call) => call.signed(threshold_signature).into(),
			EthereumApi::UpdateFlipSupply(call) => call.signed(threshold_signature).into(),
		}
	}

	fn abi_encoded(&self) -> <Ethereum as ChainAbi>::SignedTransaction {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.abi_encoded(),
			EthereumApi::RegisterClaim(call) => call.abi_encoded(),
			EthereumApi::UpdateFlipSupply(call) => call.abi_encoded(),
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
