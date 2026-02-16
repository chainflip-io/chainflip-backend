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
	eth::deposit_address::get_create_2_address,
	Chain, Tron,
};
use cf_primitives::{chains::assets::tron, ChannelId};

use crate::{chainflip::EvmEnvironment, Environment};
use cf_chains::evm::api::EvmEnvironmentProvider;

use super::AddressDerivation;

impl AddressDerivationApi<Tron> for AddressDerivation {
	fn generate_address(
		source_asset: tron::Asset,
		channel_id: ChannelId,
	) -> Result<<Tron as Chain>::ChainAccount, AddressDerivationError> {
		// TODO: We need to implement the address derivation for Tron
		Ok(get_create_2_address(
			Environment::tron_vault_address(),
			<EvmEnvironment as EvmEnvironmentProvider<Tron>>::token_address(source_asset),
			channel_id,
		))
	}

	fn generate_address_and_state(
		source_asset: <Tron as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Tron as Chain>::ChainAccount, <Tron as Chain>::DepositChannelState),
		AddressDerivationError,
	> {
		Ok((
			<Self as AddressDerivationApi<Tron>>::generate_address(source_asset, channel_id)?,
			Default::default(),
		))
	}
}
