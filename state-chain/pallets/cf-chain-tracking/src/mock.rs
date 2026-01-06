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

use std::cell::RefCell;

use crate::{self as pallet_cf_chain_tracking, Config};
use cf_chains::mocks::MockEthereum;
use cf_traits::{self, impl_mock_chainflip};
use frame_support::derive_impl;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		MockChainTracking: pallet_cf_chain_tracking,
	}
);

thread_local! {
	pub static NOMINATION: std::cell::RefCell<Option<u64>> = RefCell::new(Some(0xc001d00d_u64));
	pub static VALIDKEY: std::cell::RefCell<bool> = RefCell::new(true);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

impl Config for Test {
	type TargetChain = MockEthereum;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers!(Test);
