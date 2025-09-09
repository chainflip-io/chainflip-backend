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

use crate::evm::{api::EvmCall, tokenizable::Tokenizable};
use cf_primitives::FlipBalance;
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use sp_core::U256;
use sp_std::{vec, vec::Vec};

/// Represents all the arguments required to build the call to Vault's 'depositToScGateway'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct DepositToSCGatewayAndCall {
	amount: U256,
	sc_call: Vec<u8>,
}

impl DepositToSCGatewayAndCall {
	pub fn new(amount: FlipBalance, sc_call: Vec<u8>) -> Self {
		Self { amount: amount.into(), sc_call }
	}
}

impl EvmCall for DepositToSCGatewayAndCall {
	const FUNCTION_NAME: &'static str = "depositToScGateway";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![("amount", U256::param_type()), ("scCall", <Vec<u8>>::param_type())]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![self.amount.tokenize(), self.sc_call.clone().tokenize()]
	}
}
