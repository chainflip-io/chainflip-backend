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

	// TODO: Use Settings?
	type ElectoralSystemRunner = crate::electoral_systems::mock::MockElectoralSystemRunner;

	type WeightInfo = ();

	type SafeMode = ();

	type ElectoralSystemConfiguration = MockGovernanceHook;
}

/// A runtime whose elections instance runs a real `CompositeRunner`, so tests exercise the
/// composite vote storage and variant dispatch that `MockElectoralSystemRunner` bypasses. It is a
/// separate runtime because a second instance in `Test` would break instance inference in the
/// existing tests.
pub mod composite {
	use super::{pallet_cf_elections, MockGovernanceHook};
	use crate::{
		electoral_system::{
			ConsensusVotes, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
			ElectoralSystemTypes, ElectoralWriteAccess,
		},
		electoral_systems::composite::{
			tags,
			tuple_5_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		vote_storage::bitmap::Bitmap,
		AuthorityVoteOf, CorruptStorageError, ElectionIdentifierOf, PartialVoteOf,
		RunnerStorageAccess,
	};
	use cf_traits::impl_mock_chainflip;
	use frame_support::{derive_impl, instances::Instance1, pallet_prelude::Member, Parameter};
	use frame_system::pallet_prelude::BlockNumberFor;
	use sp_std::vec::Vec;

	type Block = frame_system::mocking::MockBlock<CompositeTest>;

	frame_support::construct_runtime!(
		pub enum CompositeTest {
			System: frame_system,
			Elections: pallet_cf_elections::<Instance1>,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
	impl frame_system::Config for CompositeTest {
		type Block = Block;
	}

	impl pallet_cf_elections::Config<Instance1> for CompositeTest {
		const TYPE_INFO_SUFFIX: &'static str = "CompositeTest";

		type ElectoralSystemRunner = MockCompositeRunner;

		type WeightInfo = ();

		type SafeMode = ();

		type ElectoralSystemConfiguration = MockGovernanceHook;
	}

	impl_mock_chainflip!(CompositeTest);

	cf_test_utilities::impl_test_helpers! {
		CompositeTest,
		RuntimeGenesisConfig {
			system: Default::default(),
			elections: Default::default(),
		},
	}

	/// Opens one election at a time and reports any full vote as consensus. Votes are stored as
	/// hashed shared data, the shape used by most production electoral systems.
	pub struct MockBitmapElectoralSystem<T>(core::marker::PhantomData<T>);

	impl<T: Parameter + Member + Eq> ElectoralSystemTypes for MockBitmapElectoralSystem<T> {
		type ValidatorId = <CompositeTest as cf_traits::Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<CompositeTest>;
		type ElectoralUnsynchronisedState = ();
		type ElectoralUnsynchronisedStateMapKey = ();
		type ElectoralUnsynchronisedStateMapValue = ();
		type ElectoralUnsynchronisedSettings = ();
		type ElectoralSettings = ();
		type ElectionIdentifierExtra = ();
		type ElectionProperties = ();
		type ElectionState = ();
		type VoteStorage = Bitmap<T>;
		type Consensus = T;
		type OnFinalizeContext = ();
		type OnFinalizeReturn = ();
	}

	impl<T: Parameter + Member + Eq> ElectoralSystem for MockBitmapElectoralSystem<T> {
		fn generate_vote_properties(
			_election_identifier: ElectionIdentifierOf<Self>,
			_previous_vote: Option<((), AuthorityVoteOf<Self>)>,
			_vote: &PartialVoteOf<Self>,
		) -> Result<(), CorruptStorageError> {
			Ok(())
		}

		fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
			election_identifiers: Vec<ElectionIdentifierOf<Self>>,
			_context: &Self::OnFinalizeContext,
		) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
			if election_identifiers.is_empty() {
				ElectoralAccess::new_election((), (), ())?;
			}
			for election_identifier in election_identifiers {
				ElectoralAccess::election_mut(election_identifier).check_consensus()?;
			}
			Ok(())
		}

		fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
			_election_access: &ElectionAccess,
			_previous_consensus: Option<&Self::Consensus>,
			votes: ConsensusVotes<Self>,
		) -> Result<Option<Self::Consensus>, CorruptStorageError> {
			Ok(votes.active_votes().into_iter().next())
		}
	}

	// Distinct types per position, so each electoral system is identified by its type alone.
	pub type EsA = MockBitmapElectoralSystem<u64>;
	pub type EsB = MockBitmapElectoralSystem<u32>;
	pub type EsC = MockBitmapElectoralSystem<u16>;
	pub type EsD = MockBitmapElectoralSystem<u8>;
	pub type EsE = MockBitmapElectoralSystem<u128>;

	pub struct MockCompositeHooks;

	impl Hooks<EsA, EsB, EsC, EsD, EsE> for MockCompositeHooks {
		fn on_finalize(
			(a, b, c, d, e): (
				Vec<ElectionIdentifierOf<EsA>>,
				Vec<ElectionIdentifierOf<EsB>>,
				Vec<ElectionIdentifierOf<EsC>>,
				Vec<ElectionIdentifierOf<EsD>>,
				Vec<ElectionIdentifierOf<EsE>>,
			),
		) -> Result<(), CorruptStorageError> {
			type Access<Tag, ES> =
				DerivedElectoralAccess<Tag, ES, RunnerStorageAccess<CompositeTest, Instance1>>;
			EsA::on_finalize::<Access<tags::A, EsA>>(a, &())?;
			EsB::on_finalize::<Access<tags::B, EsB>>(b, &())?;
			EsC::on_finalize::<Access<tags::C, EsC>>(c, &())?;
			EsD::on_finalize::<Access<tags::D, EsD>>(d, &())?;
			EsE::on_finalize::<Access<tags::EE, EsE>>(e, &())?;
			Ok(())
		}
	}

	pub type MockCompositeRunner = CompositeRunner<
		(EsA, EsB, EsC, EsD, EsE),
		<CompositeTest as cf_traits::Chainflip>::ValidatorId,
		BlockNumberFor<CompositeTest>,
		RunnerStorageAccess<CompositeTest, Instance1>,
		MockCompositeHooks,
	>;
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
	#[allow(clippy::allow_attributes)]
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
