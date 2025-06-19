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
	evm::{
		api::{EvmAddress, EvmCall},
		tokenizable::Tokenizable,
	},
};
use cf_primitives::{Asset, AssetAmount};
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_core::U256;
use sp_std::{vec, vec::Vec};

/// Represents all the arguments required to build the call to Vault's 'XCallToken'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct XCallToken {
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
	/// Address of the source token to swap.
	src_token: EvmAddress,
	/// Amount of source tokens to swap.
	amount: U256,
	/// Additional parameters to be passed to the Chainflip Protocol.
	cf_parameters: Vec<u8>,
}

impl XCallToken {
	pub fn new(
		destination_address: EncodedAddress,
		destination_asset: Asset,
		message: Vec<u8>,
		gas_budget: AssetAmount,
		source_token: EvmAddress,
		source_amount: AssetAmount,
		cf_parameters: Vec<u8>,
	) -> Self {
		Self {
			dst_chain: destination_address.chain() as u32,
			dst_address: destination_address.inner_bytes().to_vec(),
			dst_token: Into::<Asset>::into(destination_asset) as u32,
			message,
			gas_budget: gas_budget.into(),
			src_token: source_token,
			amount: source_amount.into(),
			cf_parameters,
		}
	}
}

impl EvmCall for XCallToken {
	const FUNCTION_NAME: &'static str = "xCallToken";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![
			("dstChain", u32::param_type()),
			("dstAddress", <Vec<u8>>::param_type()),
			("dstToken", u32::param_type()),
			("message", <Vec<u8>>::param_type()),
			("gasAmount", U256::param_type()),
			("srcToken", ethabi::Address::param_type()),
			("amount", U256::param_type()),
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
			self.src_token.tokenize(),
			self.amount.tokenize(),
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

		let eth_token: EvmAddress = [0xEE; 20].into();
		let amount = 1_234_567_890u128;

		let eth_vault = load_abi("IVault");
		let function_reference = eth_vault.function("xCallToken").unwrap();

		let function_runtime = EvmTransactionBuilder::new_unsigned(
			Default::default(),
			super::XCallToken::new(
				dest_address,
				dest_asset,
				ccm.message.to_vec(),
				ccm.gas_budget,
				eth_token,
				amount,
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
					dest_address_bytes.to_vec().tokenize(),
					(dest_asset as u32).tokenize(),
					ccm.message.to_vec().tokenize(),
					U256::from(ccm.gas_budget).tokenize(),
					eth_token.tokenize(),
					U256::from(amount).tokenize(),
					dummy_cf_parameter(true).tokenize(),
				])
				.unwrap()
		);

		let expected = hex!("bbddc2fb000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000014000000000000000000000000000000000000000000000000000000000000f4240000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee00000000000000000000000000000000000000000000000000000000499602d200000000000000000000000000000000000000000000000000000000000001800000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00000000000000000000000000000000000000000000000000000000000000070001020304050600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006a010001000000f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0000000000000000000000000000000000000000000000000000000000000000000010a0000000500000064f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2010004010a00000000000000000000000000000000000000000000");

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
