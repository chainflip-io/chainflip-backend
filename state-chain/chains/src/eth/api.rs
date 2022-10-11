use sp_runtime::traits::UniqueSaturatedInto;

use crate::*;

pub mod all_batch;
pub mod register_claim;
pub mod set_agg_key_with_agg_key;
pub mod set_comm_key_with_agg_key;
pub mod set_gov_key_with_agg_key;
pub mod update_flip_supply;

/// Chainflip api calls available on Ethereum.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum EthereumApi {
	SetAggKeyWithAggKey(set_agg_key_with_agg_key::SetAggKeyWithAggKey),
	RegisterClaim(register_claim::RegisterClaim),
	UpdateFlipSupply(update_flip_supply::UpdateFlipSupply),
	SetGovKeyWithAggKey(set_gov_key_with_agg_key::SetGovKeyWithAggKey),
	SetCommKeyWithAggKey(set_comm_key_with_agg_key::SetCommKeyWithAggKey),
	AllBatch(all_batch::AllBatch),
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
	) -> Result<Self::TransactionHash, Self::ValidationError> {
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

impl SetGovKeyWithAggKey<Ethereum> for EthereumApi {
	fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		new_gov_key: eth::Address,
	) -> Self {
		Self::SetGovKeyWithAggKey(set_gov_key_with_agg_key::SetGovKeyWithAggKey::new_unsigned(
			replay_protection,
			new_gov_key,
		))
	}
}

impl SetCommKeyWithAggKey<Ethereum> for EthereumApi {
	fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		new_comm_key: eth::Address,
	) -> Self {
		Self::SetCommKeyWithAggKey(set_comm_key_with_agg_key::SetCommKeyWithAggKey::new_unsigned(
			replay_protection,
			new_comm_key,
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
			EthereumApi::SetAggKeyWithAggKey(_) => unreachable!(),
			EthereumApi::RegisterClaim(call) => call.amount.unique_saturated_into(),
			EthereumApi::UpdateFlipSupply(_) => unreachable!(),
			EthereumApi::SetGovKeyWithAggKey(_) => unreachable!(),
			EthereumApi::SetCommKeyWithAggKey(_) => unreachable!(),
			EthereumApi::AllBatch(_) => unreachable!(),
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

impl AllBatch<Ethereum> for EthereumApi {
	fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Self {
		Self::AllBatch(all_batch::AllBatch::new_unsigned(
			replay_protection,
			fetch_params,
			transfer_params,
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

impl From<set_gov_key_with_agg_key::SetGovKeyWithAggKey> for EthereumApi {
	fn from(tx: set_gov_key_with_agg_key::SetGovKeyWithAggKey) -> Self {
		Self::SetGovKeyWithAggKey(tx)
	}
}

impl From<set_comm_key_with_agg_key::SetCommKeyWithAggKey> for EthereumApi {
	fn from(tx: set_comm_key_with_agg_key::SetCommKeyWithAggKey) -> Self {
		Self::SetCommKeyWithAggKey(tx)
	}
}

impl From<all_batch::AllBatch> for EthereumApi {
	fn from(tx: all_batch::AllBatch) -> Self {
		Self::AllBatch(tx)
	}
}

impl ApiCall<Ethereum> for EthereumApi {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		match self {
			EthereumApi::SetAggKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::RegisterClaim(tx) => tx.threshold_signature_payload(),
			EthereumApi::UpdateFlipSupply(tx) => tx.threshold_signature_payload(),
			EthereumApi::SetGovKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::SetCommKeyWithAggKey(tx) => tx.threshold_signature_payload(),
			EthereumApi::AllBatch(tx) => tx.threshold_signature_payload(),
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
		}
	}

	fn chain_encoded(&self) -> <Ethereum as ChainAbi>::SignedTransaction {
		match self {
			EthereumApi::SetAggKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::RegisterClaim(call) => call.chain_encoded(),
			EthereumApi::UpdateFlipSupply(call) => call.chain_encoded(),
			EthereumApi::SetGovKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::SetCommKeyWithAggKey(call) => call.chain_encoded(),
			EthereumApi::AllBatch(call) => call.chain_encoded(),
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
