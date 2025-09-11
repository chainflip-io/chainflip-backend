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

use super::decl_api::{self, *};
use sp_api::impl_runtime_apis;

use crate::{AccountId, BitcoinElections, Block, GenericElections, Runtime, SolanaElections};

impl_runtime_apis! {
	impl decl_api::ElectoralRuntimeApi<Block> for Runtime {
		fn cf_solana_electoral_data(account_id: AccountId) -> Vec<u8> {
			SolanaElections::electoral_data(&account_id).encode()
		}

		fn cf_solana_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			SolanaElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_bitcoin_electoral_data(account_id: AccountId) -> Vec<u8> {
			BitcoinElections::electoral_data(&account_id).encode()
		}

		fn cf_bitcoin_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			BitcoinElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_generic_electoral_data(account_id: AccountId) -> Vec<u8> {
			GenericElections::electoral_data(&account_id).encode()
		}

		fn cf_generic_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			GenericElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}
	}
}
