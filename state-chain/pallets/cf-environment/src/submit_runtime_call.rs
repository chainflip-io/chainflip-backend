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
use core::primitive::str;
use ethereum_eip712::eip712::{Eip712, Eip712Error};
use frame_support::{
	dispatch::{DispatchErrorWithPostInfo, DispatchResultWithPostInfo},
	traits::UnfilteredDispatchable,
	weights::Weight,
};
use scale_info::prelude::{
	format,
	string::{String, ToString},
};
use serde::{Deserialize, Serialize};

pub const ETHEREUM_SIGN_MESSAGE_PREFIX: &str = "\x19Ethereum Signed Message:\n";
pub const MAX_BATCHED_CALLS: u32 = 10u32;
// We don't use Anza's offchain signing proposal because it's not supported by wallets.
// The main Solana wallets support utf-8 signing only so we can't use Anza's prefix
// either. We strip the non-utf-8 characters from Anza's prefix. These transactions won't
// result in on-chain Solana transactions anyway.
pub const DOMAIN_OFFCHAIN_PREFIX: &str = "chainflip offchain";

pub type BatchedCalls<T> = BoundedVec<<T as Config>::RuntimeCall, ConstU32<MAX_BATCHED_CALLS>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize, TypeInfo)]
pub struct TransactionMetadata {
	pub nonce: u32,
	pub expiry_block: BlockNumber,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize)]
pub enum EthEncodingType {
	PersonalSign,
	Eip712,
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize)]
pub enum SolEncodingType {
	Domain,
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

#[derive(Encode, Decode, TypeInfo, Debug, Clone, PartialEq)]
pub struct ChainflipExtrinsic<C> {
	pub call: C,
	pub transaction_metadata: TransactionMetadata,
}

/// `signer is not technically necessary but is added as part of the metadata so
/// we add it so is displayed separately to the user in the wallet.
/// TODO: This is a temporary simplified implementation for basic EIP-712 support
/// in a specific format. Full logic to be implemented in PRO-2535.
pub(crate) fn build_eip_712_payload(
	call: impl Encode + TypeInfo + 'static,
	chain_name: &str,
	version: &str,
	transaction_metadata: TransactionMetadata,
) -> Result<Vec<u8>, Eip712Error> {
	let domain = ethereum_eip712::eip712::EIP712Domain {
		name: Some(chain_name.to_string()),
		version: Some(version.to_string()),
		chain_id: None,
		verifying_contract: None,
		salt: None,
	};

	let mut typed_data = ethereum_eip712::encode_eip712_using_type_info(
		ChainflipExtrinsic { call, transaction_metadata },
		domain,
	)?;

	typed_data.types.insert(
		"EIP712Domain".to_string(),
		vec![
			ethereum_eip712::eip712::Eip712DomainType {
				name: "name".to_string(),
				r#type: "string".to_string(),
			},
			ethereum_eip712::eip712::Eip712DomainType {
				name: "version".to_string(),
				r#type: "string".to_string(),
			},
		],
	);

	typed_data.encode_eip712()
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

/// Checks non-native signed call metadata against current chain state.
pub(crate) fn validate_metadata<T: Config>(
	transaction_metadata: &TransactionMetadata,
	signer_account: &T::AccountId,
) -> TransactionValidity {
	// Check if payload hasn't expired
	ensure!(
		BlockNumberFor::<T>::from(transaction_metadata.expiry_block) >
			frame_system::Pallet::<T>::block_number(),
		InvalidTransaction::Stale
	);

	// Check account nonce
	let current_nonce = frame_system::Pallet::<T>::account_nonce(signer_account);
	let tx_nonce: <T as frame_system::Config>::Nonce = transaction_metadata.nonce.into();

	ensure!(tx_nonce >= current_nonce, InvalidTransaction::Stale);

	// Build transaction validity with requires/provides
	let mut tx_builder = ValidTransaction::with_tag_prefix(<Pallet<T>>::name())
		.and_provides((signer_account, transaction_metadata.nonce));

	if tx_nonce > current_nonce {
		// This is a future tx, require the immediately previous nonce
		tx_builder = tx_builder.and_requires((signer_account, transaction_metadata.nonce - 1));
	}

	tx_builder.build()
}

pub fn build_domain_data(
	call: impl Encode,
	chainflip_network: &ChainflipNetwork,
	transaction_metadata: &TransactionMetadata,
	spec_version: u32,
) -> String {
	format!(
		"/network:{}/version:{}/call:{}/nonce:{}/expiry_block:{}",
		chainflip_network.as_str(),
		spec_version,
		hex::encode(call.encode()),
		transaction_metadata.nonce,
		transaction_metadata.expiry_block
	)
}

/// Validates the signature, given some call and metadata.
///
/// This call should be kept idempotent: it should not access storage.
pub(crate) fn is_valid_signature(
	call: impl Encode + TypeInfo + 'static,
	chainflip_network: &ChainflipNetwork,
	transaction_metadata: &TransactionMetadata,
	signature_data: &SignatureData,
	spec_version: u32,
) -> Result<bool, Eip712Error> {
	let raw_payload =
		|| build_domain_data(&call, chainflip_network, transaction_metadata, spec_version);

	match signature_data {
		SignatureData::Solana { signature, signer, sig_type } => {
			let signed_payload = match sig_type {
				SolEncodingType::Domain =>
					format!("{}{}", DOMAIN_OFFCHAIN_PREFIX, raw_payload()).into_bytes(),
			};
			Ok(verify_sol_signature(signer, &signed_payload, signature))
		},
		SignatureData::Ethereum { signature, signer, sig_type } => {
			let signed_payload = match sig_type {
				EthEncodingType::PersonalSign => {
					let payload = raw_payload();
					format!("{}{}{}", ETHEREUM_SIGN_MESSAGE_PREFIX, payload.len(), payload)
						.into_bytes()
				},
				EthEncodingType::Eip712 => build_eip_712_payload(
					call,
					chainflip_network.as_str(),
					&format!("{}", spec_version),
					*transaction_metadata,
				)?,
			};
			Ok(verify_evm_signature(signer, &signed_payload, signature))
		},
	}
}
