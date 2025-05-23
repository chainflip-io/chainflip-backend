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
use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{Address, Token};
use frame_support::pallet_prelude::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SetGovKeyWithAggKey {
	/// The new gov key.
	pub new_gov_key: Address,
}

impl SetGovKeyWithAggKey {
	pub fn new(new_gov_key: Address) -> Self {
		Self { new_gov_key }
	}
}

impl EvmCall for SetGovKeyWithAggKey {
	const FUNCTION_NAME: &'static str = "setGovKeyWithAggKey";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![("newGovKey", Address::param_type())]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![self.new_gov_key.tokenize()]
	}
}

#[cfg(test)]
mod test_set_gov_key_with_agg_key {
	use super::*;
	use crate::{
		eth::api::abi::load_abi,
		evm::{api::EvmTransactionBuilder, tests::asymmetrise, SchnorrVerificationComponents},
	};

	#[test]
	fn test_known_payload() {
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 6;
		const TEST_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);

		let key_manager = load_abi("IKeyManager");

		let tx_builder = EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_KEYMAN_ADDR.into(),
			},
			super::SetGovKeyWithAggKey::new(Address::from(TEST_ADDR)),
		);

		assert_eq!(
			// Our encoding:
			tx_builder
				.signed(
					&SchnorrVerificationComponents {
						s: FAKE_SIG,
						k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
					},
					Default::default()
				)
				.chain_encoded(),
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			key_manager
				.function("setGovKeyWithAggKey")
				.unwrap()
				.encode_input(&[
					// sigData: SigData(uint, uint, address)
					Token::Tuple(vec![
						Token::Uint(FAKE_SIG.into()),
						Token::Uint(NONCE.into()),
						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
					]),
					Token::Address(TEST_ADDR.into()),
				])
				.unwrap()
		);
	}
}
