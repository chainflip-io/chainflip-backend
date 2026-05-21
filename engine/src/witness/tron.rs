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

pub(crate) mod tron_deposits;
pub(crate) mod vault_swaps_witnessing;

use cf_chains::assets::tron::Asset;
use std::collections::HashMap;

use crate::{evm::event::EvmEventSource, witness::evm::erc20_deposits::Erc20Events};
use ethers::{prelude::abigen, types::H160};

abigen!(TronVault, "$CF_TRON_CONTRACT_ABI_ROOT/IVault.json");

// ----- deposit channel querying -----

#[derive(Clone)]
pub struct TronDepositChannelWitnessingConfig {
	pub vault_contract: EvmEventSource<TronVaultEvents>,
	pub supported_assets: HashMap<Asset, EvmEventSource<Erc20Events>>,
}

// ----- vault deposit witnessing -----
#[derive(Clone)]
pub struct VaultDepositWitnessingConfig {
	pub vault: H160,
	pub vault_events: EvmEventSource<TronVaultEvents>,
	pub supported_assets: HashMap<Asset, EvmEventSource<Erc20Events>>,
}
