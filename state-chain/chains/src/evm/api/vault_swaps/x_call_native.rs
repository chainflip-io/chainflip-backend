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

use crate::{
	address::EncodedAddress,
	evm::{api::EvmCall, tokenizable::Tokenizable},
};
use cf_primitives::{Asset, AssetAmount};
use codec::{Decode, DecodeWithMemTracking, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_core::U256;
use sp_std::{vec, vec::Vec};

/// Represents all the arguments required to build the call to Vault's 'XCallNative'
/// function.
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct XCallNative {
	/// The destination chain according to Chainflip Protocol's nomenclature.
	dst_chain: u32,
	/// Bytes containing the destination address on the destination chain.
	dst_address: Vec<u8>,
	/// Destination token to be swapped to.
	dst_token: u32,
	/// Arbitrary bytes passed in by the user as part of the CCM message.
	message: Vec<u8>,
	/// Amount gas the user allows to execute the CCM on the target chain.
	gas_budget: U256,
	/// Additional parameters to be passed to the Chainflip Protocol.
	cf_parameters: Vec<u8>,
}

impl XCallNative {
	pub fn new(
		destination_address: EncodedAddress,
		destination_asset: Asset,
		message: Vec<u8>,
		gas_budget: AssetAmount,
		cf_parameters: Vec<u8>,
	) -> Self {
		Self {
			dst_chain: destination_address.chain() as u32,
			dst_address: destination_address.inner_bytes().to_vec(),
			dst_token: destination_asset as u32,
			message,
			gas_budget: gas_budget.into(),
			cf_parameters,
		}
	}
}

impl EvmCall for XCallNative {
	const FUNCTION_NAME: &'static str = "xCallNative";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![
			("dstChain", u32::param_type()),
			("dstAddress", <Vec<u8>>::param_type()),
			("dstToken", u32::param_type()),
			("message", <Vec<u8>>::param_type()),
			("gasAmount", U256::param_type()),
			("cfParameters", <Vec<u8>>::param_type()),
		]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![
			self.dst_chain.tokenize(),
			self.dst_address.clone().tokenize(),
			self.dst_token.tokenize(),
			self.message.clone().tokenize(),
			self.gas_budget.tokenize(),
			self.cf_parameters.clone().tokenize(),
		]
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		eth::api::abi::load_abi,
		evm::api::{vault_swaps::test_utils::*, EvmTransactionBuilder},
	};
	use cf_primitives::ForeignChain;
	use hex_literal::hex;

	#[test]
	fn test_payload() {
		let dest_address = EncodedAddress::Dot([0xff; 32]);
		let dest_address_bytes = dest_address.inner_bytes().to_vec().clone();
		let dest_chain = ForeignChain::Polkadot as u32;
		let dest_asset = Asset::Dot;
		let ccm = channel_metadata();

		let eth_vault = load_abi("IVault");
		let function_reference = eth_vault.function("xCallNative").unwrap();

		// Create the EVM call without replay protection and signer info.
		// It is expected for vault swap calls to be unsigned.
		let function_runtime = EvmTransactionBuilder::new_unsigned(
			Default::default(),
			super::XCallNative::new(
				dest_address,
				dest_asset,
				ccm.message.to_vec().clone(),
				ccm.gas_budget,
				dummy_cf_parameter(true),
			),
		);

		let runtime_payload = function_runtime.chain_encoded_payload();
		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			function_reference
				.encode_input(&[
					dest_chain.tokenize(),
					dest_address_bytes.tokenize(),
					(dest_asset as u32).tokenize(),
					ccm.message.to_vec().tokenize(),
					U256::from(ccm.gas_budget).tokenize(),
					dummy_cf_parameter(true).tokenize(),
				])
				.unwrap()
		);

		let expected = hex!("07933dd2000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000f424000000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00000000000000000000000000000000000000000000000000000000000000070001020304050600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006b010001000000f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f000000000000000000000000000000000000000000000000000000000000000000000010a0000000500000064f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2010004010a000000000000000000000000000000000000000000");

		// Check against hardcoded tested payload
		assert_eq!(
			runtime_payload.clone(),
			expected,
			"Encoded payload mismatch. Expected: \n{:?} \nActual: \n{:?}",
			hex::encode(expected),
			hex::encode(runtime_payload),
		);
	}
}
