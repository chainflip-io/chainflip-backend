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
use frame_support::{
	dispatch::DispatchErrorWithPostInfo, traits::UnfilteredDispatchable, weights::Weight,
};

pub const ETHEREUM_SIGN_MESSAGE_PREFIX: &str = "\x19Ethereum Signed Message:\n";
pub const SOLANA_OFFCHAIN_PREFIX: &[u8] = b"\xffsolana offchain";
pub const UNSIGNED_BATCH_VERSION: &str = "0";
pub const BATCHED_CALL_LIMITS: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TransactionMetadata {
	pub nonce: u32,
	pub expiry_block: BlockNumber,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum EthSigType {
	Domain, // personal_sign
	Eip712,
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SolSigType {
	Domain, /* Using `b"\xffsolana offchain" as per Anza specifications,
	         * even if we are not using the proposal. Phantom might use
	         * a different standard though..
	         * References
	         * https://docs.anza.xyz/proposals/off-chain-message-signing
	         * And/or phantom off-chain signing:
	         * https://github.com/phantom/sign-in-with-solana */
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum UserSignatureData {
	Solana { signature: SolSignature, signer: SolAddress, sig_type: SolSigType },
	Ethereum { signature: EthereumSignature, signer: EvmAddress, sig_type: EthSigType },
}

impl UserSignatureData {
	/// Extract the signer account ID as T::AccountId from the signature data
	pub fn signer_account<AccountId: codec::Decode>(&self) -> Result<AccountId, codec::Error> {
		use cf_chains::evm::ToAccountId32;

		let account_id_32 = match self {
			UserSignatureData::Solana { signer, .. } => AccountId32::new((*signer).into()),
			UserSignatureData::Ethereum { signer, .. } => signer.into_account_id_32(),
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
	calls: Vec<<T as Config>::RuntimeCall>,
	weight_fn: fn(u32) -> Weight,
) -> DispatchResultWithPostInfo {
	let mut weight = Weight::zero();
	let calls_len = calls.len();
	if calls_len > BATCHED_CALL_LIMITS {
		return Err(Error::<T>::TooManyCalls.into());
	}

	for (index, call) in calls.into_iter().enumerate() {
		let info = call.get_dispatch_info();

		let origin = frame_system::RawOrigin::Signed(signer_account.clone()).into();

		// Don't allow nested calls.
		if let Some(Call::non_native_signed_call { .. }) | Some(Call::batch { .. }) =
			call.is_sub_type()
		{
			let base_weight = weight_fn(index.saturating_add(1) as u32);
			let err = DispatchErrorWithPostInfo {
				post_info: Some(base_weight.saturating_add(weight)).into(),
				error: DispatchError::Other("Nested runtime call batches not allowed"),
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

/// `signer is not technically necessary but is added as part of the metadata so
/// we add it so is displayed separately to the user in the wallet.
/// To be implemented in PRO-2535.
pub(crate) fn build_eip_712_payload<T: Config>(
	_call: <T as Config>::RuntimeCall,
	_chain_name: &str,
	_version: &str,
	_transaction_metadata: TransactionMetadata,
	_signer: EvmAddress,
) -> Vec<u8> {
	todo!("implement eip-712");
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

pub(crate) fn validate_unsigned<T: Config>(
	_source: TransactionSource,
	call: &Call<T>,
) -> TransactionValidity {
	if let Call::non_native_signed_call { call, transaction_metadata, user_signature_data } = call {
		// Check if payload hasn't expired
		if frame_system::Pallet::<T>::block_number() >= transaction_metadata.expiry_block.into() {
			return InvalidTransaction::Stale.into();
		}

		// Extract signer account ID
		let signer_account: T::AccountId = match user_signature_data.signer_account() {
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
		let serialized_calls: Vec<u8> = call.encode();

		let build_domain_data = || -> Vec<u8> {
			[
				serialized_calls.clone(),
				chanflip_network_name.as_str().encode(),
				UNSIGNED_BATCH_VERSION.encode(),
				transaction_metadata.encode(),
			]
			.concat()
		};

		let valid_signature = match user_signature_data {
			UserSignatureData::Solana { signature, signer, sig_type } => {
				let signed_payload = match sig_type {
					SolSigType::Domain => {
						let domain_data = build_domain_data();
						[SOLANA_OFFCHAIN_PREFIX, domain_data.as_slice()].concat()
					},
				};
				verify_sol_signature(signer, &signed_payload, signature)
			},
			UserSignatureData::Ethereum { signature, signer, sig_type } => {
				let signed_payload = match sig_type {
					EthSigType::Domain => {
						let domain_data = build_domain_data();
						let prefix = scale_info::prelude::format!(
							"{}{}",
							ETHEREUM_SIGN_MESSAGE_PREFIX,
							domain_data.len()
						);
						let prefix_bytes = prefix.as_bytes();
						[prefix_bytes, &domain_data].concat()
					},
					EthSigType::Eip712 => build_eip_712_payload::<T>(
						*call.clone(),
						chanflip_network_name.as_str(),
						UNSIGNED_BATCH_VERSION,
						transaction_metadata.clone(),
						*signer,
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
	} else {
		InvalidTransaction::Call.into()
	}
}
