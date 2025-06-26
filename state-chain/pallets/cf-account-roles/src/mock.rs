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

#![cfg(test)]

use crate::{self as pallet_cf_account_roles, Config};
use cf_primitives::SubAccountIndex;
use cf_traits::{
	impl_mock_chainflip,
	mocks::{deregistration_check::MockDeregistrationCheck, fee_payment::MockFeePayment},
	SubAccountHandler,
};
use frame_support::derive_impl;
use sp_runtime::DispatchError;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		MockAccountRoles: pallet_cf_account_roles,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type OnNewAccount = MockAccountRoles;
	type OnKilledAccount = MockAccountRoles;
}

impl_mock_chainflip!(Test);

pub struct MockSubAccountHandler;

impl SubAccountHandler<u64> for MockSubAccountHandler {
	fn derive_and_fund_sub_account(
		parent_account_id: u64,
		sub_account_index: SubAccountIndex,
	) -> Result<u64, DispatchError> {
		Ok(parent_account_id + sub_account_index as u64)
	}
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EnsureGovernance = frame_system::EnsureRoot<<Self as frame_system::Config>::AccountId>;
	type DeregistrationCheck = MockDeregistrationCheck<Self::AccountId>;
	type RuntimeCall = RuntimeCall;
	type SubAccountHandler = MockSubAccountHandler;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = MockFeePayment<Self>;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers!(Test);
