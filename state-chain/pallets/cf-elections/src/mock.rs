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

pub use crate::{self as pallet_cf_elections};
use crate::{ElectoralSystemConfiguration, InitialStateOf, Pallet, UniqueMonotonicIdentifier};

use cf_traits::{impl_mock_chainflip, AccountRoleRegistry};
use frame_support::{assert_ok, derive_impl, instances::Instance1, traits::OriginTrait};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Elections: pallet_cf_elections::<Instance1>,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

pub struct MockGovernanceHook;

impl ElectoralSystemConfiguration for MockGovernanceHook {
	type SafeMode = ();
	type ElectoralEvents = ();

	type Properties = ();

	fn start(_: ()) {}
}

impl pallet_cf_elections::Config<Instance1> for Test {
	const TYPE_INFO_SUFFIX: &'static str = "Test";
	type RuntimeEvent = RuntimeEvent;

	// TODO: Use Settings?
	type ElectoralSystemRunner = crate::electoral_systems::mock::MockElectoralSystemRunner;

	type WeightInfo = ();

	type SafeMode = ();

	type ElectoralSystemConfiguration = MockGovernanceHook;
}

impl_mock_chainflip!(Test);

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		elections: Default::default(),
	},
}

#[derive(Clone, Debug)]
pub struct TestSetup {
	pub initial_state: InitialStateOf<Test, Instance1>,
	pub num_contributing_authorities: u64,
	pub num_non_contributing_authorities: u64,
}

impl Default for TestSetup {
	fn default() -> Self {
		Self {
			initial_state: InitialStateOf::<Test, _> {
				unsynchronised_state: (),
				unsynchronised_settings: (),
				settings: (),
				shared_data_reference_lifetime: Default::default(),
			},
			num_contributing_authorities: 3,
			num_non_contributing_authorities: 0,
		}
	}
}

impl TestSetup {
	pub fn all_authorities(&self) -> Vec<u64> {
		(0..self.num_contributing_authorities + self.num_non_contributing_authorities).collect()
	}

	pub fn contributing_authorities(&self) -> Vec<u64> {
		self.all_authorities()
			.into_iter()
			.take(self.num_contributing_authorities as usize)
			.collect()
	}

	pub fn non_contributing_authorities(&self) -> Vec<u64> {
		self.all_authorities()
			.into_iter()
			.skip(self.num_contributing_authorities as usize)
			.collect()
	}
}

#[derive(Clone, Debug)]
pub struct TestContext {
	#[allow(dead_code)]
	pub setup: TestSetup,
	pub umis: Vec<UniqueMonotonicIdentifier>,
}

/// Set up a test for the election pallet.
///
/// Intializes the pallet with the given initial state and contributing authorities. The authorities
/// are registered as validators and contributing authorities submit `stop_ignoring_my_votes`
/// extrinsics.
pub fn election_test_ext(test_setup: TestSetup) -> TestRunner<TestContext> {
	new_test_ext()
		.execute_with(|| {
			assert_ok!(Pallet::<Test, _>::internally_initialize(test_setup.initial_state.clone()));
			for id in test_setup.all_authorities() {
				<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&id)
					.unwrap();
			}
			MockEpochInfo::next_epoch(test_setup.all_authorities());

			Pallet::<Test, _>::do_try_state().expect("All try-state variants must hold");

			test_setup
		})
		.then_apply_extrinsics(|test_setup| {
			(0..test_setup.num_contributing_authorities)
				.map(|id| {
					(
						OriginTrait::signed(id),
						crate::Call::<Test, _>::stop_ignoring_my_votes {},
						Ok(()),
					)
				})
				.collect::<Vec<_>>()
		})
		.map_context(|test_setup| TestContext { setup: test_setup, umis: Vec::new() })
}
