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

use crate::{self as pallet_cf_witness, PalletOffence, WitnessDataExtraction};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::offence_reporting::MockOffenceReporter, AccountRoleRegistry, CallDispatchFilter,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{derive_impl, pallet_prelude::RuntimeDebug, parameter_types};
use frame_system as system;
use scale_info::TypeInfo;

pub mod dummy;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Witnesser: pallet_cf_witness,
		Dummy: dummy,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type Block = Block;
}

impl_mock_runtime_safe_mode! { witnesser: pallet_cf_witness::PalletSafeMode<MockCallFilter> }

parameter_types! {
	pub static AllowCall: bool = true;
	pub const GracePeriod: u64 = 10u64;
}

#[derive(
	serde::Serialize,
	serde::Deserialize,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Copy,
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
)]
pub struct MockCallFilter;

impl CallDispatchFilter<RuntimeCall> for MockCallFilter {
	fn should_dispatch(&self, _call: &RuntimeCall) -> bool {
		AllowCall::get()
	}
}

pub type OffenceReporter = MockOffenceReporter<u64, PalletOffence>;

impl pallet_cf_witness::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type SafeMode = MockRuntimeSafeMode;
	type CallDispatchPermission = MockCallFilter;
	type Offence = PalletOffence;
	type OffenceReporter = OffenceReporter;
	type LateWitnessGracePeriod = GracePeriod;
	type WeightInfo = ();
}

impl_mock_chainflip!(Test);

impl dummy::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EnsureWitnessed = pallet_cf_witness::EnsureWitnessed;
}

impl WitnessDataExtraction for RuntimeCall {
	fn extract(&mut self) -> Option<Vec<u8>> {
		if let RuntimeCall::Dummy(dummy::Call::put_value { value }) = self {
			Some(core::mem::take(value).encode())
		} else {
			None
		}
	}

	fn combine_and_inject(&mut self, data: &mut [Vec<u8>]) {
		if let RuntimeCall::Dummy(dummy::Call::put_value { value }) = self {
			*value = data.iter_mut().map(|encoded| u32::decode(&mut &encoded[..]).unwrap()).sum();
		}
	}
}

pub const ALISSA: <Test as frame_system::Config>::AccountId = 1u64;
pub const BOBSON: <Test as frame_system::Config>::AccountId = 2u64;
pub const CHARLEMAGNE: <Test as frame_system::Config>::AccountId = 3u64;
pub const DEIRDRE: <Test as frame_system::Config>::AccountId = 4u64;
const GENESIS_AUTHORITIES: [u64; 3] = [ALISSA, BOBSON, CHARLEMAGNE];

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	||{
		MockEpochInfo::next_epoch(Vec::from(GENESIS_AUTHORITIES));
		for id in GENESIS_AUTHORITIES.iter().chain(&[DEIRDRE]) {
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id)
				.unwrap();
		}
	}
}
