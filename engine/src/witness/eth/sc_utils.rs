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
use ethers::prelude::abigen;

abigen!(ScUtils, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IScUtils.json");

#[cfg(test)]
mod tests {
	use cf_primitives::FlipBalance;
	use codec::Encode;
	use frame_support::sp_runtime::AccountId32;
	use state_chain_runtime::chainflip::ethereum_sc_calls::{
		DelegationAmount, DelegationApi, EthereumSCApi,
	};

	#[test]
	fn test_sc_call_encode() {
		let sc_call_delegate = EthereumSCApi::<FlipBalance>::Delegation {
			call: DelegationApi::Delegate {
				operator: AccountId32::new([0xF4; 32]),
				increase: DelegationAmount::<FlipBalance>::Max,
			},
		}
		.encode();
		assert_eq!(
			sc_call_delegate,
			hex::decode("0000f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f400")
				.unwrap()
		);
	}
}
