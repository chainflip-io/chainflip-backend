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
use crate::eth::deposit_address::get_salt;
use cf_primitives::{AssetAmount, ChannelId};
use ethabi::{Address, ParamType, Token, Uint};

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableFetchAssetParams {
	pub contract_address: Address,
	pub asset: Address,
}

impl Tokenizable for EncodableFetchAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![Token::Address(self.contract_address), Token::Address(self.asset)])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![ParamType::Address, ParamType::Address])
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub(crate) struct EncodableFetchDeployAssetParams {
	pub channel_id: ChannelId,
	pub asset: Address,
}

impl Tokenizable for EncodableFetchDeployAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::FixedBytes(get_salt(self.channel_id).to_vec()),
			Token::Address(self.asset),
		])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![ParamType::FixedBytes(32), ParamType::Address])
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct EncodableTransferAssetParams {
	/// For EVM, the asset is encoded as a contract address.
	pub asset: Address,
	pub to: Address,
	pub amount: AssetAmount,
}

impl Tokenizable for EncodableTransferAssetParams {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::Address(self.asset),
			Token::Address(self.to),
			Token::Uint(Uint::from(self.amount)),
		])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![ParamType::Address, ParamType::Address, ParamType::Uint(256)])
	}
}
