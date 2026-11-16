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
	($mod_name:ident, $name:ident, $contract_events_type:ident, $($abi_path:tt)+) => {
		pub mod $mod_name {
			use super::Erc20Events;
			use crate::evm::conversions::IntoSpCore;

			alloy_sol_types::sol!(#[sol(all_derives)] $name, $($abi_path)+);

			pub use $name::$contract_events_type;

			impl From<$contract_events_type> for Erc20Events {
				fn from(event: $contract_events_type) -> Self {
					match event {
						$contract_events_type::Transfer($name::Transfer { to, from, value }) =>
							Self::TransferFilter {
								to: to.into_sp_core(),
								from: from.into_sp_core(),
								value: value.into_sp_core(),
							},
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
	concat!(env!("CF_ETH_CONTRACT_ABI_ROOT"), "/", env!("CF_ETH_CONTRACT_ABI_TAG"), "/IFLIP.json")
);
define_erc20!(usdc, Usdc, UsdcEvents, concat!(env!("CF_ETH_CONTRACT_ABI_ROOT"), "/IUSDC.json"));
define_erc20!(usdt, Usdt, UsdtEvents, concat!(env!("CF_ETH_CONTRACT_ABI_ROOT"), "/IUSDT.json"));
define_erc20!(wbtc, Wbtc, WbtcEvents, concat!(env!("CF_ETH_CONTRACT_ABI_ROOT"), "/IWBTC.json"));
define_erc20!(cbbtc, Cbbtc, CbbtcEvents, concat!(env!("CF_ETH_CONTRACT_ABI_ROOT"), "/ICBBTC.json"));
