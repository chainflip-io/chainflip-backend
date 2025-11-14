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

use crate::{self as pallet_cf_pools, PalletSafeMode};
use cf_chains::{Ethereum, ForeignChain};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		balance_api::MockLpRegistration, egress_handler::MockEgressHandler,
		lp_stats_api::MockLpStatsApi, swap_request_api::MockSwapRequestHandler,
	},
	AccountRoleRegistry,
};
use frame_support::{assert_ok, derive_impl};
use frame_system as system;

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 124u64;
pub const CHARLIE: <Test as frame_system::Config>::AccountId = 125u64;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		LiquidityPools: pallet_cf_pools,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

impl_mock_runtime_safe_mode!(pools: PalletSafeMode);
impl pallet_cf_pools::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type LpBalance = cf_traits::mocks::balance_api::MockBalance;
	type SwapRequestHandler = MockSwapRequestHandler<(Ethereum, MockEgressHandler<Ethereum>)>;
	type LpRegistrationApi = MockLpRegistration;
	type LpStats = MockLpStatsApi;
	type SafeMode = MockRuntimeSafeMode;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&ALICE,
		));
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&BOB,
		));
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&CHARLIE,
		));

		for lp in [ALICE, BOB, CHARLIE] {
			for chain in ForeignChain::iter() {
				MockLpRegistration::register_refund_address(lp, chain);
			}
		}

	}
}
