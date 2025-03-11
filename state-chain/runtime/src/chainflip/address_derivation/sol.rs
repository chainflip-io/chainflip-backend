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
	address::{AddressDerivationApi, AddressDerivationError},
	sol::{
		api::SolanaEnvironment,
		sol_tx_core::{address_derivation::derive_deposit_address, PdaAndBump},
	},
	Solana,
};

use super::AddressDerivation;
use crate::SolEnvironment;

impl AddressDerivationApi<Solana> for AddressDerivation {
	fn generate_address(
		source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		channel_id: cf_primitives::ChannelId,
	) -> Result<<Solana as cf_chains::Chain>::ChainAccount, AddressDerivationError> {
		<Self as AddressDerivationApi<Solana>>::generate_address_and_state(source_asset, channel_id)
			.map(|(address, _state)| address)
	}

	fn generate_address_and_state(
		_source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		channel_id: cf_primitives::ChannelId,
	) -> Result<
		(
			<Solana as cf_chains::Chain>::ChainAccount,
			<Solana as cf_chains::Chain>::DepositChannelState,
		),
		AddressDerivationError,
	> {
		let api_env = SolEnvironment::api_environment()
			.map_err(|_| AddressDerivationError::MissingSolanaApiEnvironment)?;

		derive_deposit_address(channel_id, api_env.vault_program)
			.map(|PdaAndBump { address, bump }| (address, bump))
			.map_err(AddressDerivationError::SolanaDerivationError)
	}
}
