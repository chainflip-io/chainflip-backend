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

use crate::{
	ArbitrumIngressEgress, BitcoinIngressEgress, EthereumIngressEgress, PolkadotIngressEgress,
	SolanaIngressEgress,
};
use cf_primitives::AssetAmount;
use cf_traits::BoostApi;
use sp_core::crypto::AccountId32;

pub struct IngressEgressBoostApi;

impl BoostApi for IngressEgressBoostApi {
	type AccountId = AccountId32;
	type AssetMap = cf_chains::assets::any::AssetMap<AssetAmount>;

	fn boost_pool_account_balances(who: &Self::AccountId) -> Self::AssetMap {
		Self::AssetMap {
			eth: EthereumIngressEgress::boost_pool_account_balances(who),
			dot: PolkadotIngressEgress::boost_pool_account_balances(who),
			btc: BitcoinIngressEgress::boost_pool_account_balances(who),
			arb: ArbitrumIngressEgress::boost_pool_account_balances(who),
			sol: SolanaIngressEgress::boost_pool_account_balances(who),
		}
	}
}
