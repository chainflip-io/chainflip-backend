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

use super::*;
use cf_chains::evm::{encode, Token, U256};
use frame_support::{
	dispatch::{DispatchErrorWithPostInfo, DispatchResultWithPostInfo},
	sp_runtime::traits::{Hash, Keccak256},
	traits::UnfilteredDispatchable,
	weights::Weight,
};
use serde::{Deserialize, Serialize};
pub const ETHEREUM_SIGN_MESSAGE_PREFIX: &str = "\x19Ethereum Signed Message:\n";
pub const SOLANA_OFFCHAIN_PREFIX: &[u8] = b"\xffsolana offchain";
pub const MAX_BATCHED_CALLS: u32 = 10u32;
// Using a str for consistency between EIP-712 and other encodings
pub const UNSIGNED_CALL_VERSION: &str = "0";

pub type BatchedCalls<T> = BoundedVec<<T as Config>::RuntimeCall, ConstU32<MAX_BATCHED_CALLS>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize, TypeInfo)]
pub struct TransactionMetadata {
	pub nonce: u32,
	pub expiry_block: BlockNumber,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum EthEncodingType {
	PersonalSign,
	Eip712,
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SolEncodingType {
	Domain, /* Using `b"\xffsolana offchain" as per Anza specifications,
	         * even if we are not using the proposal. Phantom might use
	         * a different standard though..
	         * References
	         * https://docs.anza.xyz/proposals/off-chain-message-signing
	         * And/or phantom off-chain signing:
	         * https://github.com/phantom/sign-in-with-solana */
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SignatureData {
	Solana { signature: SolSignature, signer: SolAddress, sig_type: SolEncodingType },
	Ethereum { signature: EthereumSignature, signer: EvmAddress, sig_type: EthEncodingType },
}

impl SignatureData {
	/// Extract the signer account ID as T::AccountId from the signature data
	pub fn signer_account<AccountId: codec::Decode>(&self) -> Result<AccountId, codec::Error> {
		use cf_chains::evm::ToAccountId32;

		let account_id_32 = match self {
			SignatureData::Solana { signer, .. } => AccountId32::new((*signer).into()),
			SignatureData::Ethereum { signer, .. } => signer.into_account_id_32(),
		};

		AccountId::decode(&mut account_id_32.encode().as_slice())
	}
}

/// Executes a batch of calls and returns the total weight or an error with an associated call
/// index. It will error early as soon as a call fails.
/// Inspiration from pallet_utility
/// https://paritytech.github.io/polkadot-sdk/master/pallet_utility/pallet/struct.Pallet.html
pub(crate) fn batch_all<T: Config>(
	signer_account: T::AccountId,
	calls: BatchedCalls<T>,
	weight_fn: fn(u32) -> Weight,
) -> DispatchResultWithPostInfo {
	let mut weight = Weight::zero();
	let calls_len = calls.len();

	for (index, call) in calls.into_iter().enumerate() {
		let info = call.get_dispatch_info();

		let origin = frame_system::RawOrigin::Signed(signer_account.clone()).into();

		// Don't allow nested calls.
		if let Some(Call::batch { .. }) = call.is_sub_type() {
			let base_weight = weight_fn(index.saturating_add(1) as u32);
			let err = DispatchErrorWithPostInfo {
				post_info: Some(base_weight.saturating_add(weight)).into(),
				error: Error::<T>::InvalidNestedBatch.into(),
			};
			return Err(err);
		}

		let result = call.dispatch_bypass_filter(origin);

		weight =
			weight.saturating_add(frame_support::dispatch::extract_actual_weight(&result, &info));

		result.map_err(|mut err| {
			// Take the weight of this function itself into account.
			let base_weight = weight_fn(index.saturating_add(1) as u32);
			// Return the actual used weight + base_weight of this call.
			err.post_info = Some(base_weight.saturating_add(weight)).into();
			err
		})?;
	}

	let base_weight = weight_fn(calls_len as u32);
	Ok(Some(base_weight.saturating_add(weight)).into())
}

const EIP712_DOMAIN_TYPE_STR: &str = "EIP712Domain(string name,string version)";
const EIP712_DOMAIN_PREFIX: [u8; 2] = [0x19, 0x01];
const EIP712_METADATA_TYPE_STR: &str = "Metadata(uint32 nonce,uint32 expiryBlock)";
const EIP712_RUNTIMECALL_TYPE_STR: &str = "RuntimeCall(bytes value)";
const EIP712_TRANSACTION_TYPE_STR: &str = "Transaction(RuntimeCall call,Metadata metadata)";

/// `signer is not technically necessary but is added as part of the metadata so
/// we add it so is displayed separately to the user in the wallet.
/// TODO: This is a temporary simplified implementation for basic EIP-712 support
/// in a specific format. Full logic to be implemented in PRO-2535.
pub(crate) fn build_eip_712_payload<T: Config>(
	call: &<T as Config>::RuntimeCall,
	chain_name: &str,
	version: &str,
	transaction_metadata: TransactionMetadata,
) -> Vec<u8> {
	// -----------------
	// Domain separator
	// -----------------
	// Not using chain_id as this is not an EVM network and the domain name
	// will act as the replay protection between different Chainflip networks.
	let type_hash = Keccak256::hash(EIP712_DOMAIN_TYPE_STR.as_bytes());
	let name_hash = Keccak256::hash(chain_name.as_bytes());
	let version_hash = Keccak256::hash(version.as_bytes());

	let tokens = vec![
		Token::FixedBytes(type_hash.as_bytes().to_vec()),
		Token::FixedBytes(name_hash.as_bytes().to_vec()),
		Token::FixedBytes(version_hash.as_bytes().to_vec()),
	];

	// ABI encode
	let encoded = encode(&tokens);
	let domain_separator = Keccak256::hash(&encoded);

	// -----------------
	// Metadata struct
	// -----------------
	let metadata_type_str = EIP712_METADATA_TYPE_STR;
	let metadata_type_hash = Keccak256::hash(metadata_type_str.as_bytes());
	let metadata_tokens = vec![
		Token::FixedBytes(metadata_type_hash.as_bytes().to_vec()),
		Token::Uint(U256::from(transaction_metadata.nonce)),
		Token::Uint(U256::from(transaction_metadata.expiry_block)),
	];
	let encoded_metadata = encode(&metadata_tokens);
	let metadata_hash = Keccak256::hash(&encoded_metadata);

	// -----------------
	// RuntimeCall struct
	// -----------------
	let runtime_call_type_str = EIP712_RUNTIMECALL_TYPE_STR;
	let runtime_call_type_hash = Keccak256::hash(runtime_call_type_str.as_bytes());

	let runtime_call_tokens = vec![
		Token::FixedBytes(runtime_call_type_hash.as_bytes().to_vec()),
		Token::FixedBytes(Keccak256::hash(&call.encode()).0.to_vec()),
	];
	let encoded_runtime_call = encode(&runtime_call_tokens);
	let runtime_call_hash = Keccak256::hash(&encoded_runtime_call);

	// -----------------
	// Message struct
	// -----------------
	let transaction_type_str = scale_info::prelude::format!(
		"{}{}{}",
		EIP712_TRANSACTION_TYPE_STR,
		metadata_type_str,
		runtime_call_type_str,
	);
	let transaction_type_hash = Keccak256::hash(transaction_type_str.as_bytes());
	let tokens = vec![
		Token::FixedBytes(transaction_type_hash.as_bytes().to_vec()),
		Token::FixedBytes(runtime_call_hash.as_bytes().to_vec()),
		Token::FixedBytes(metadata_hash.as_bytes().to_vec()),
	];

	let encoded_message = encode(&tokens);
	let message_hash = Keccak256::hash(&encoded_message);

	// -----------------
	// EIP712 digest
	// -----------------
	let mut encoded_final = EIP712_DOMAIN_PREFIX.to_vec();
	encoded_final.extend_from_slice(domain_separator.0.as_slice());
	encoded_final.extend_from_slice(message_hash.0.as_slice());
	encoded_final
}

/// Get the accumulated `weight` and the dispatch class for the given `calls`.
pub(crate) fn weight_and_dispatch_class<T: Config>(
	calls: &[<T as Config>::RuntimeCall],
) -> (Weight, DispatchClass) {
	let dispatch_infos = calls.iter().map(|call| call.get_dispatch_info());
	let (dispatch_weight, dispatch_class) = dispatch_infos.fold(
		(Weight::zero(), DispatchClass::Operational),
		|(total_weight, dispatch_class): (Weight, DispatchClass), di| {
			(
				total_weight.saturating_add(di.weight),
				// If not all are `Operational`, we want to use `DispatchClass::Normal`.
				if di.class == DispatchClass::Normal { di.class } else { dispatch_class },
			)
		},
	);

	(dispatch_weight, dispatch_class)
}

pub fn build_domain_data(
	encoded_call: Vec<u8>,
	chanflip_network_name: ChainflipNetwork,
	transaction_metadata: TransactionMetadata,
) -> Vec<u8> {
	[
		encoded_call,
		chanflip_network_name.as_str().encode(),
		UNSIGNED_CALL_VERSION.encode(),
		transaction_metadata.encode(),
	]
	.concat()
}

pub(crate) fn validate_non_native_signed_call<T: Config>(
	call: &<T as Config>::RuntimeCall,
	transaction_metadata: TransactionMetadata,
	signature_data: &SignatureData,
) -> TransactionValidity {
	// Check if payload hasn't expired
	if frame_system::Pallet::<T>::block_number() >= transaction_metadata.expiry_block.into() {
		return InvalidTransaction::Stale.into();
	}

	// Extract signer account ID
	let signer_account: T::AccountId = match signature_data.signer_account() {
		Ok(account_id) => account_id,
		Err(_) => return InvalidTransaction::BadSigner.into(),
	};

	// Check account nonce
	let current_nonce = frame_system::Pallet::<T>::account_nonce(&signer_account);
	let tx_nonce: <T as frame_system::Config>::Nonce = transaction_metadata.nonce.into();

	if tx_nonce < current_nonce {
		return InvalidTransaction::Stale.into();
	}

	// Signature check
	let chanflip_network_name = ChainflipNetworkName::<T>::get();

	let valid_signature = match signature_data {
		SignatureData::Solana { signature, signer, sig_type } => {
			let signed_payload = match sig_type {
				SolEncodingType::Domain => {
					let domain_data = build_domain_data(
						call.encode(),
						chanflip_network_name,
						transaction_metadata,
					);
					[SOLANA_OFFCHAIN_PREFIX, domain_data.as_slice()].concat()
				},
			};
			verify_sol_signature(signer, &signed_payload, signature)
		},
		SignatureData::Ethereum { signature, signer, sig_type } => {
			let signed_payload = match sig_type {
				EthEncodingType::PersonalSign => {
					let domain_data = build_domain_data(
						call.encode(),
						chanflip_network_name,
						transaction_metadata,
					);
					let prefix = scale_info::prelude::format!(
						"{}{}",
						ETHEREUM_SIGN_MESSAGE_PREFIX,
						domain_data.len()
					);
					let prefix_bytes = prefix.as_bytes();
					[prefix_bytes, &domain_data].concat()
				},
				EthEncodingType::Eip712 => build_eip_712_payload::<T>(
					call,
					chanflip_network_name.as_str(),
					UNSIGNED_CALL_VERSION,
					transaction_metadata,
				),
			};
			verify_evm_signature(signer, &signed_payload, signature)
		},
	};

	if !valid_signature {
		return InvalidTransaction::BadProof.into();
	}

	// Build transaction validity with requires/provides
	let unique_id = (signer_account.clone(), transaction_metadata.nonce);

	let mut tx = ValidTransaction::with_tag_prefix(<Pallet<T>>::name()).and_provides(unique_id);

	if tx_nonce > current_nonce {
		// This is a future tx, require the immediately previous nonce
		tx = tx.and_requires((signer_account, transaction_metadata.nonce - 1));
	}

	tx.build()
}
