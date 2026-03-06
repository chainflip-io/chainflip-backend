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

use cf_chains::evm::{Address as EvmAddress, U256};

#[derive(Debug)]
pub enum Erc20Events {
	TransferFilter { to: EvmAddress, from: EvmAddress, value: U256 },
	Other,
}

macro_rules! define_erc20 {
	($mod_name:ident, $name:ident, $contract_events_type:ident, $abi_path:literal) => {
		pub mod $mod_name {
			use super::Erc20Events;
			use ethers::prelude::abigen;

			abigen!($name, $abi_path);

			impl From<$contract_events_type> for Erc20Events {
				fn from(event: $contract_events_type) -> Self {
					match event {
						$contract_events_type::TransferFilter(TransferFilter {
							to,
							from,
							value,
						}) => Self::TransferFilter { to, from, value },
						_ => Self::Other,
					}
				}
			}
		}
	};
}

define_erc20!(
	flip,
	Flip,
	FlipEvents,
	"$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IFLIP.json"
);
define_erc20!(usdc, Usdc, UsdcEvents, "$CF_ETH_CONTRACT_ABI_ROOT/IUSDC.json");
define_erc20!(usdt, Usdt, UsdtEvents, "$CF_ETH_CONTRACT_ABI_ROOT/IUSDT.json");
define_erc20!(wbtc, Wbtc, WbtcEvents, "$CF_ETH_CONTRACT_ABI_ROOT/IWBTC.json");
