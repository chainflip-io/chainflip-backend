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

// TODO: See if we can dedup this once the vault address stuff is deduped
use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError},
	eth::deposit_address::get_create_2_address,
	Arbitrum, Chain,
};
use cf_primitives::{chains::assets::arb, ChannelId};

use crate::{chainflip::EvmEnvironment, Environment};
use cf_chains::evm::api::EvmEnvironmentProvider;

use super::AddressDerivation;

impl AddressDerivationApi<Arbitrum> for AddressDerivation {
	fn generate_address(
		source_asset: arb::Asset,
		channel_id: ChannelId,
	) -> Result<<Arbitrum as Chain>::ChainAccount, AddressDerivationError> {
		Ok(get_create_2_address(
			Environment::arb_vault_address(),
			<EvmEnvironment as EvmEnvironmentProvider<Arbitrum>>::token_address(source_asset),
			channel_id,
		))
	}

	fn generate_address_and_state(
		source_asset: <Arbitrum as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Arbitrum as Chain>::ChainAccount, <Arbitrum as Chain>::DepositChannelState),
		AddressDerivationError,
	> {
		Ok((
			<Self as AddressDerivationApi<Arbitrum>>::generate_address(source_asset, channel_id)?,
			Default::default(),
		))
	}
}
