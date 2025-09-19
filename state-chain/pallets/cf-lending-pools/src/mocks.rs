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

use crate as pallet_cf_lending_pools;
use cf_chains::Ethereum;
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		egress_handler::MockEgressHandler, pool_api::MockPoolApi,
		swap_request_api::MockSwapRequestHandler,
	},
	AccountRoleRegistry,
};
use frame_support::derive_impl;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		LendingPools: pallet_cf_lending_pools
	}
);

impl_mock_chainflip!(Test);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlock<Test>;
}

impl_mock_runtime_safe_mode!(lending_pools: crate::PalletSafeMode);

impl pallet_cf_lending_pools::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Balance = cf_traits::mocks::balance_api::MockBalance;
	type PoolApi = MockPoolApi;
	type PriceApi = cf_traits::mocks::price_feed_api::MockPriceFeedApi;
	type SwapRequestHandler = MockSwapRequestHandler<(Ethereum, MockEgressHandler<Ethereum>)>;
	type SafeMode = MockRuntimeSafeMode;
}

pub type AccountId = <Test as frame_system::Config>::AccountId;

pub const LP: AccountId = 123u64;
pub const OTHER_LP: AccountId = 234u64;
pub const BOOSTER_1: AccountId = 1;
pub const BOOSTER_2: AccountId = 2;
pub const BOOSTER_3: AccountId = 3;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&LP,
		));
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&OTHER_LP,
		));
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&BOOSTER_1,
		));
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&BOOSTER_2,
		));
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&BOOSTER_3,
		));
	}
}
