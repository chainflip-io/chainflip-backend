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

use crate as pallet_cf_asset_balances;
use crate::PalletSafeMode;
use cf_chains::{
	btc::ScriptPubkey,
	dot::{PolkadotAccountId, PolkadotCrypto},
	AnyChain,
};
use cf_primitives::AccountId;

use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{egress_handler::MockEgressHandler, key_provider::MockKeyProvider},
	IncreaseOrDecrease, OrderId, PoolApi, PoolPairsMap, Side,
};
use frame_support::{derive_impl, sp_runtime::app_crypto::sp_core::H160};
use frame_system as system;
use sp_runtime::traits::IdentityLookup;

use cf_chains::ForeignChainAddress;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		AssetBalances: pallet_cf_asset_balances,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

impl_mock_chainflip!(Test);

pub const ETH_ADDR_1: ForeignChainAddress = ForeignChainAddress::Eth(H160([0; 20]));
pub const ETH_ADDR_2: ForeignChainAddress = ForeignChainAddress::Eth(H160([1; 20]));
pub const ARB_ADDR_1: ForeignChainAddress = ForeignChainAddress::Arb(H160([2; 20]));

pub const DOT_ADDR_1: ForeignChainAddress =
	ForeignChainAddress::Dot(PolkadotAccountId::from_aliased([1; 32]));

pub const BTC_ADDR_1: ForeignChainAddress =
	ForeignChainAddress::Btc(ScriptPubkey::Taproot([1u8; 32]));

pub const SOL_ADDR: ForeignChainAddress =
	ForeignChainAddress::Sol(cf_chains::sol::SolAddress([1u8; 32]));

impl_mock_runtime_safe_mode!(refunding: PalletSafeMode);

// Simple no-op PoolApi implementation for AssetBalances tests
pub struct TestMockPoolApi;

impl PoolApi for TestMockPoolApi {
	type AccountId = AccountId;

	fn sweep(_who: &Self::AccountId) -> sp_runtime::DispatchResult {
		// No-op implementation for testing
		Ok(())
	}

	fn open_order_count(
		_who: &Self::AccountId,
		_asset_pair: &PoolPairsMap<cf_primitives::Asset>,
	) -> Result<u32, sp_runtime::DispatchError> {
		Ok(0)
	}

	fn open_order_balances(
		_who: &Self::AccountId,
	) -> cf_chains::assets::any::AssetMap<cf_primitives::AssetAmount> {
		Default::default()
	}

	fn pools() -> std::vec::Vec<PoolPairsMap<cf_primitives::Asset>> {
		std::vec![]
	}

	fn update_limit_order(
		_account: &Self::AccountId,
		_base_asset: cf_primitives::Asset,
		_quote_asset: cf_primitives::Asset,
		_side: Side,
		_id: OrderId,
		_option_tick: Option<cf_primitives::Tick>,
		_amount_change: IncreaseOrDecrease<cf_primitives::AssetAmount>,
	) -> sp_runtime::DispatchResult {
		Ok(())
	}

	fn cancel_all_limit_orders(_account: &Self::AccountId) -> sp_runtime::DispatchResult {
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_pool(
		_base_asset: cf_primitives::Asset,
		_quote_asset: cf_primitives::Asset,
		_fee_hundredth_pips: u32,
		_initial_price: cf_primitives::Price,
	) -> sp_runtime::DispatchResult {
		Ok(())
	}
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type PolkadotKeyProvider = MockKeyProvider<PolkadotCrypto>;
	type PoolApi = TestMockPoolApi;
	type SafeMode = MockRuntimeSafeMode;
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		MockKeyProvider::<PolkadotCrypto>::set_key(
			PolkadotAccountId::from_aliased([0xff; 32]),
		);
	}
}
