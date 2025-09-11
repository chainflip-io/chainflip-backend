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

use frame_support::sp_runtime::AccountId32;
use sp_api::decl_runtime_apis;
use sp_std::vec::Vec;

decl_runtime_apis!(
	/// Versioning of runtime apis is explained here:
	/// https://docs.rs/sp-api/latest/sp_api/macro.decl_runtime_apis.html
	/// Of course it doesn't explain everything, e.g. there's a very useful
	/// `#[renamed($OLD_NAME, $VERSION)]` attribute which will handle renaming
	/// of apis automatically.
	#[api_version(2)]
	pub trait ElectoralRuntimeApi {
		/// Returns SCALE encoded `Option<ElectoralDataFor<state_chain_runtime::Runtime,
		/// Instance>>`
		#[renamed("cf_electoral_data", 2)]
		fn cf_solana_electoral_data(account_id: AccountId32) -> Vec<u8>;

		/// Returns SCALE encoded `BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as
		/// pallet_cf_elections::Config<Instance>>::ElectoralSystem>>`
		#[renamed("cf_filter_votes", 2)]
		fn cf_solana_filter_votes(account_id: AccountId32, proposed_votes: Vec<u8>) -> Vec<u8>;

		fn cf_bitcoin_electoral_data(account_id: AccountId32) -> Vec<u8>;

		fn cf_bitcoin_filter_votes(account_id: AccountId32, proposed_votes: Vec<u8>) -> Vec<u8>;

		fn cf_generic_electoral_data(account_id: AccountId32) -> Vec<u8>;

		fn cf_generic_filter_votes(account_id: AccountId32, proposed_votes: Vec<u8>) -> Vec<u8>;
	}
);
