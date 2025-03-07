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

use crate::AssetBalances;
use cf_primitives::EpochIndex;
use cf_traits::EpochTransitionHandler;

use crate::{ArbitrumVault, BitcoinVault, EthereumVault, PolkadotVault, SolanaVault, Witnesser};

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	fn on_expired_epoch(expired: EpochIndex) {
		<Witnesser as EpochTransitionHandler>::on_expired_epoch(expired);
		<EthereumVault as EpochTransitionHandler>::on_expired_epoch(expired);
		<PolkadotVault as EpochTransitionHandler>::on_expired_epoch(expired);
		<BitcoinVault as EpochTransitionHandler>::on_expired_epoch(expired);
		<ArbitrumVault as EpochTransitionHandler>::on_expired_epoch(expired);
		<SolanaVault as EpochTransitionHandler>::on_expired_epoch(expired);
	}
	fn on_new_epoch(_new: EpochIndex) {
		AssetBalances::trigger_reconciliation();
	}
}
