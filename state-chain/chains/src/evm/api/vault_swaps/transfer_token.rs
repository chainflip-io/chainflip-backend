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

use crate::evm::{
	api::{EvmAddress, EvmCall},
	tokenizable::Tokenizable,
};
use cf_primitives::AssetAmount;
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_core::U256;
use sp_std::{vec, vec::Vec};

/// Represents all the arguments required to build the call to ERC20's 'transfer'
/// function for ERC20 asset transfers.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct TransferToken {
	/// The destination address to receive the tokens.
	to: EvmAddress,
	/// Amount of tokens to transfer.
	value: U256,
}

impl TransferToken {
	pub fn new(to: EvmAddress, amount: AssetAmount) -> Self {
		Self { to, value: amount.into() }
	}
}

impl EvmCall for TransferToken {
	const FUNCTION_NAME: &'static str = "transfer";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![("to", ethabi::Address::param_type()), ("value", U256::param_type())]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![self.to.tokenize(), self.value.tokenize()]
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::evm::api::EvmTransactionBuilder;

	// Loading manually because the ERC20 contract is not part of the tagged contracts.
	fn load_erc20_abi() -> ethabi::Contract {
		let mut path = std::path::PathBuf::from(env!("CF_ETH_CONTRACT_ABI_ROOT"));
		path.push("IERC20.json");
		let file = std::fs::File::open(path.canonicalize().unwrap()).unwrap();
		ethabi::Contract::load(file).unwrap()
	}

	#[test]
	fn test_payload() {
		let to_address: EvmAddress = [0xAA; 20].into();
		let amount = 1_234_567_890u128;

		let erc20_abi = load_erc20_abi();
		let function_reference = erc20_abi.function("transfer").unwrap();

		let function_runtime = EvmTransactionBuilder::new_unsigned(
			Default::default(),
			super::TransferToken::new(to_address, amount),
		);

		let runtime_payload = function_runtime.chain_encoded_payload();

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			function_reference
				.encode_input(&[to_address.tokenize(), U256::from(amount).tokenize(),])
				.unwrap()
		);
	}
}
