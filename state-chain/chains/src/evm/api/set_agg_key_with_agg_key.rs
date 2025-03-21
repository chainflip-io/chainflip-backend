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

use crate::evm::AggKey;

use super::*;

use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::vec;

/// Represents all the arguments required to build the call to StateChainGateway's
/// 'requestRedemption' function.
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SetAggKeyWithAggKey {
	/// The new public key.
	pub new_key: AggKey,
}

impl SetAggKeyWithAggKey {
	pub fn new<Key: Into<AggKey> + Clone>(new_key: Key) -> Self {
		Self { new_key: new_key.into() }
	}
}

impl EvmCall for SetAggKeyWithAggKey {
	const FUNCTION_NAME: &'static str = "setAggKeyWithAggKey";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![("newKey", AggKey::param_type())]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![self.new_key.tokenize()]
	}
}

#[cfg(test)]
mod test_set_agg_key_with_agg_key {
	use crate::{
		eth::api::abi::load_abi,
		evm::{
			api::{EvmReplayProtection, EvmTransactionBuilder},
			SchnorrVerificationComponents,
		},
	};

	use super::*;

	#[test]
	fn test_known_payload() {
		let key_manager_address =
			hex_literal::hex!("A44B9f3F5Bb8C278c1ee85D8F32517c6EFa64B0D").into();
		let expected_payload =
			hex_literal::hex!("d45a181d77a4e10810b033734a611885d85848663b98f372f5d279309b3da0b5")
				.into();
		let call = EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection {
				nonce: 0,
				chain_id: 31337,
				key_manager_address,
				contract_address: key_manager_address,
			},
			super::SetAggKeyWithAggKey::new(AggKey::from_pubkey_compressed(hex_literal::hex!(
				"01 1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d"
			))),
		);
		assert_eq!(call.threshold_signature_payload(), expected_payload);
	}

	#[test]
	fn test_set_agg_key_with_agg_key_payload() {
		use crate::evm::{tests::asymmetrise, ParityBit};
		use ethabi::Token;
		const CHAIN_ID: u64 = 1;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const NONCE: u64 = 6;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const FAKE_NEW_KEY_X: [u8; 32] = asymmetrise([0xcf; 32]);
		const FAKE_NEW_KEY_Y: ParityBit = ParityBit::Odd;

		let key_manager = load_abi("IKeyManager");

		let set_agg_key_reference = key_manager.function("setAggKeyWithAggKey").unwrap();

		let set_agg_key_runtime = EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_KEYMAN_ADDR.into(),
			},
			super::SetAggKeyWithAggKey::new(AggKey {
				pub_key_x: FAKE_NEW_KEY_X,
				pub_key_y_parity: FAKE_NEW_KEY_Y,
			}),
		);

		let expected_msg_hash = set_agg_key_runtime.threshold_signature_payload();
		let runtime_payload = set_agg_key_runtime
			.clone()
			.signed(
				&SchnorrVerificationComponents {
					s: FAKE_SIG,
					k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
				},
				Default::default(),
			)
			.chain_encoded();
		// Ensure signing payload isn't modified by signature.
		assert_eq!(set_agg_key_runtime.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			set_agg_key_reference
				.encode_input(&[
					// sigData: SigData(uint, uint, address)
					Token::Tuple(vec![
						Token::Uint(FAKE_SIG.into()),
						Token::Uint(NONCE.into()),
						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
					]),
					// nodeId: bytes32
					Token::Tuple(vec![
						Token::Uint(FAKE_NEW_KEY_X.into()),
						Token::Uint(FAKE_NEW_KEY_Y.into()),
					]),
				])
				.unwrap()
		);
	}
}
