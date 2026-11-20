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

use crate::Runtime;

/// Checks run regardless of the role being deregistered: state is keyed by account, and an
/// account can hold state from a previous role (e.g. a delegation bond after `undelegate`), so
/// gating checks by the current role lets that state leak into the next one.
pub type RuntimeDeregistrationHooks = (
	pallet_cf_flip::PalletDeregistrationHooks<Runtime>,
	pallet_cf_validator::PalletDeregistrationHooks<Runtime>,
	pallet_cf_asset_balances::PalletDeregistrationHooks<Runtime>,
	pallet_cf_trading_strategy::PalletDeregistrationHooks<Runtime>,
	pallet_cf_pools::PalletDeregistrationHooks<Runtime>,
	pallet_cf_lending_pools::PalletDeregistrationHooks<Runtime>,
	// Last: its pending swap check iterates over all swap requests.
	pallet_cf_swapping::PalletDeregistrationHooks<Runtime>,
);
