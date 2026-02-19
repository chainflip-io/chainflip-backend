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

use crate as pallet_cf_lp;
use crate::PalletSafeMode;
use cf_chains::{assets::any::Asset, AnyChain, Ethereum};
use cf_primitives::{chains::assets, AssetAmount};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		address_converter::MockAddressConverter, deposit_handler::MockDepositHandler,
		egress_handler::MockEgressHandler, pool_api::MockPoolApi,
		swap_request_api::MockSwapRequestHandler,
	},
	AccountRoleRegistry, BalanceApi, BoostBalancesApi, MinimumDeposit,
};
use frame_support::{assert_ok, derive_impl, parameter_types};
use frame_system as system;
use sp_runtime::{traits::IdentityLookup, Permill};
use std::{cell::RefCell, collections::BTreeMap};

type AccountId = u64;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		LiquidityProvider: pallet_cf_lp,
	}
);

thread_local! {
	pub static BALANCE_MAP: RefCell<BTreeMap<(AccountId, Asset), AssetAmount>> =
		RefCell::new(BTreeMap::new());
}

pub const MINIMUM_DEPOSIT: u128 = 100;
pub struct MockMinimumDepositProvider;
impl MinimumDeposit for MockMinimumDepositProvider {
	fn get(_asset: Asset) -> AssetAmount {
		MINIMUM_DEPOSIT
	}
}

pub struct MockBalanceApi;

impl BalanceApi for MockBalanceApi {
	type AccountId = AccountId;

	fn credit_account(who: &Self::AccountId, _asset: Asset, amount: AssetAmount) {
		BALANCE_MAP.with(|balance_map| {
			let mut balance_map = balance_map.borrow_mut();
			*balance_map.entry((*who, _asset)).or_default() += amount;
		});
	}

	fn try_credit_account(
		who: &Self::AccountId,
		asset: cf_primitives::Asset,
		amount: cf_primitives::AssetAmount,
	) -> frame_support::dispatch::DispatchResult {
		Self::credit_account(who, asset, amount);
		Ok(())
	}

	fn try_debit_account(
		who: &Self::AccountId,
		asset: cf_primitives::Asset,
		amount: cf_primitives::AssetAmount,
	) -> frame_support::dispatch::DispatchResult {
		BALANCE_MAP.with(|balance_map| {
			let mut balance_map = balance_map.borrow_mut();
			let balance = balance_map.entry((who.to_owned(), asset)).or_default();
			*balance = balance.checked_sub(amount).ok_or("Insufficient balance")?;
			Ok(())
		})
	}

	fn free_balances(who: &Self::AccountId) -> assets::any::AssetMap<cf_primitives::AssetAmount> {
		BALANCE_MAP.with(|balance_map| {
			assets::any::AssetMap::from_iter_or_default(Asset::all().map(|asset| {
				(asset, balance_map.borrow().get(&(*who, asset)).cloned().unwrap_or_default())
			}))
		})
	}

	fn get_balance(who: &Self::AccountId, asset: Asset) -> AssetAmount {
		BALANCE_MAP
			.with(|balance_map| balance_map.borrow().get(&(who.to_owned(), asset)).cloned())
			.unwrap_or_default()
	}
}

impl MockBalanceApi {
	pub fn insert_balance(account: AccountId, asset: Asset, amount: AssetAmount) {
		BALANCE_MAP.with(|balance_map| {
			balance_map.borrow_mut().insert((account, asset), amount);
		});
	}

	pub fn get_balance(account: &AccountId, asset: Asset) -> Option<AssetAmount> {
		BALANCE_MAP.with(|balance_map| balance_map.borrow().get(&(*account, asset)).cloned())
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

impl_mock_chainflip!(Test);

parameter_types! {
	pub const NetworkFee: Permill = Permill::from_percent(0);
	pub static BoostBalance: AssetAmount = Default::default();
}

impl_mock_runtime_safe_mode!(liquidity_provider: PalletSafeMode);
impl crate::Config for Test {
	type DepositHandler = MockDepositHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type AddressConverter = MockAddressConverter;
	type SafeMode = MockRuntimeSafeMode;
	type WeightInfo = ();
	type PoolApi = MockPoolApi;
	type BalanceApi = MockBalanceApi;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = cf_traits::mocks::fee_payment::MockFeePayment<Self>;
	type BoostBalancesApi = MockIngressEgressBoostApi;
	type SwapRequestHandler = MockSwapRequestHandler<(Ethereum, MockEgressHandler<Ethereum>)>;
	type MinimumDeposit = MockMinimumDepositProvider;
}

pub struct MockIngressEgressBoostApi;
impl BoostBalancesApi for MockIngressEgressBoostApi {
	type AccountId = AccountId;

	fn boost_pool_account_balance(_who: &Self::AccountId, _asset: Asset) -> AssetAmount {
		BoostBalance::get()
	}
}

impl MockIngressEgressBoostApi {
	pub fn set_boost_funds(amount: AssetAmount) -> Result<(), ()> {
		BoostBalance::set(amount);
		Ok(())
	}
	pub fn remove_boost_funds(amount: AssetAmount) -> Result<(), ()> {
		if amount > BoostBalance::get() {
			return Err(());
		}
		BoostBalance::set(amount - BoostBalance::get());
		Ok(())
	}
}

pub const LP_ACCOUNT: AccountId = 1;
pub const LP_ACCOUNT_2: AccountId = 3;
pub const NON_LP_ACCOUNT: AccountId = 2;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&LP_ACCOUNT,
		));
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&LP_ACCOUNT_2,
		));
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(
			&NON_LP_ACCOUNT,
		));
	}
}
