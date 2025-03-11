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

use cf_chains::{
	evm::{api::EvmEnvironmentProvider, Address},
	Chain,
};

/// A mock that just returns defaults for the KeyManager and Chain ID.
pub struct MockEvmEnvironment;

impl<C: Chain> EvmEnvironmentProvider<C> for MockEvmEnvironment {
	fn key_manager_address() -> Address {
		Default::default()
	}

	fn vault_address() -> Address {
		Default::default()
	}

	fn next_nonce() -> u64 {
		Default::default()
	}

	fn token_address(_asset: <C as Chain>::ChainAsset) -> Option<Address> {
		Some(Default::default())
	}

	fn chain_id() -> cf_chains::evm::api::EvmChainId {
		Default::default()
	}
}
