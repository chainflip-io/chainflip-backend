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

use super::mocks::*;
use crate::{
	electoral_system::{ConsensusStatus, ElectoralSystem},
	electoral_systems::exact_value::*,
	ElectionIdentifier,
};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::assert_ok;
use scale_info::TypeInfo;

// Define two `ExactValue` ElectoralSystems.
type WitnessExactValueWithStorage = ExactValue<
	MockIdentifier,
	Vec<u32>,
	(),
	MockHook<true>,
	u32,
	u64,
	crate::vote_storage::bitmap::Bitmap<Vec<u32>>,
>;
type WitnessExactValueWithoutStorage = ExactValue<
	MockIdentifier,
	Vec<u32>,
	(),
	MockHook<false>,
	u32,
	u64,
	crate::vote_storage::bitmap::Bitmap<Vec<u32>>,
>;

#[derive(
	Clone,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	PartialOrd,
	Ord,
	Default,
)]
pub struct MockIdentifier(Vec<u32>);

impl From<Vec<u32>> for MockIdentifier {
	fn from(value: Vec<u32>) -> Self {
		Self(value)
	}
}

struct MockHook<const STORE: bool>;
// impl using STORE to decide whether or not to store the data.
impl<const STORE: bool> ExactValueHook<MockIdentifier, Vec<u32>> for MockHook<STORE> {
	type StorageKey = MockIdentifier;
	type StorageValue = Vec<u32>;

	fn on_consensus(id: MockIdentifier, value: Vec<u32>) -> Option<(MockIdentifier, Vec<u32>)> {
		STORE.then_some((id, value))
	}
}

fn with_default_state() -> TestContext<WitnessExactValueWithStorage> {
	TestSetup::<WitnessExactValueWithStorage>::default().build()
}

#[test]
fn solana_election_result_reference_counting_works() {
	with_default_state().then(|| {
		let mock_id: MockIdentifier = vec![1u32, 2u32].into();
		let id = |umi: u64| ElectionIdentifier::new(umi.into(), ());

		// Start first election and come to consensus.
		assert_ok!(WitnessExactValueWithStorage::witness_exact_value::<
			MockAccess<WitnessExactValueWithStorage>,
		>(mock_id.clone()));
		assert!(WitnessExactValueWithStorage::take_election_result::<
			MockAccess<WitnessExactValueWithStorage>,
		>(mock_id.clone())
		.is_none());

		MockStorageAccess::set_consensus_status::<WitnessExactValueWithStorage>(
			id(0),
			ConsensusStatus::Gained { most_recent: None, new: vec![11, 12] },
		);
		assert_ok!(WitnessExactValueWithStorage::on_finalize::<
			MockAccess<WitnessExactValueWithStorage>,
		>(vec![id(0)], &()));

		// First successful lookup `takes` the storage value.
		assert_eq!(
			WitnessExactValueWithStorage::take_election_result::<
				MockAccess<WitnessExactValueWithStorage>,
			>(mock_id.clone()),
			Some(vec![11, 12])
		);
		assert!(WitnessExactValueWithStorage::take_election_result::<
			MockAccess<WitnessExactValueWithStorage>,
		>(mock_id.clone())
		.is_none());

		// Start elections with overlapping and identical identifiers.
		let first_id: MockIdentifier = vec![1u32, 2u32].into();
		let overlap_id: MockIdentifier = vec![2u32, 3u32].into();

		assert_ok!(WitnessExactValueWithStorage::witness_exact_value::<
			MockAccess<WitnessExactValueWithStorage>,
		>(first_id.clone()));
		assert_ok!(WitnessExactValueWithStorage::witness_exact_value::<
			MockAccess<WitnessExactValueWithStorage>,
		>(overlap_id.clone()));
		assert_ok!(WitnessExactValueWithStorage::witness_exact_value::<
			MockAccess<WitnessExactValueWithStorage>,
		>(first_id.clone()));

		// Vote and come to consensus
		MockStorageAccess::set_consensus_status::<WitnessExactValueWithStorage>(
			id(1),
			ConsensusStatus::Gained { most_recent: None, new: vec![11, 12] },
		);
		MockStorageAccess::set_consensus_status::<WitnessExactValueWithStorage>(
			id(2),
			ConsensusStatus::Gained { most_recent: None, new: vec![22, 23] },
		);
		// Storage map should update with the new value
		MockStorageAccess::set_consensus_status::<WitnessExactValueWithStorage>(
			id(3),
			ConsensusStatus::Gained { most_recent: None, new: vec![31, 32] },
		);

		assert_ok!(WitnessExactValueWithStorage::on_finalize::<
			MockAccess<WitnessExactValueWithStorage>,
		>(vec![id(1), id(2), id(3)], &()));

		// First lookup takes the value from the storage map
		assert_eq!(
			WitnessExactValueWithStorage::take_election_result::<
				MockAccess<WitnessExactValueWithStorage>,
			>(first_id.clone()),
			Some(vec![31, 32])
		);
		assert_eq!(
			WitnessExactValueWithStorage::take_election_result::<
				MockAccess<WitnessExactValueWithStorage>,
			>(overlap_id.clone()),
			Some(vec![22, 23])
		);
		assert_eq!(
			WitnessExactValueWithStorage::take_election_result::<
				MockAccess<WitnessExactValueWithStorage>,
			>(first_id.clone()),
			Some(vec![31, 32])
		);

		assert!(WitnessExactValueWithStorage::take_election_result::<
			MockAccess<WitnessExactValueWithStorage>,
		>(first_id.clone())
		.is_none());
		assert!(WitnessExactValueWithStorage::take_election_result::<
			MockAccess<WitnessExactValueWithStorage>,
		>(overlap_id.clone())
		.is_none());
	});
}

#[test]
fn election_result_can_be_stored_into_unsynchronised_state_map() {
	with_default_state().then(|| {
		let mock_id: MockIdentifier = vec![1u32, 2u32].into();
		let id = |umi: u64| ElectionIdentifier::new(umi.into(), ());

		// If `on_consensus` returns false, the result is not stored into the state map.
		assert_ok!(WitnessExactValueWithoutStorage::witness_exact_value::<
			MockAccess<WitnessExactValueWithoutStorage>,
		>(mock_id.clone()));
		MockStorageAccess::set_consensus_status::<WitnessExactValueWithoutStorage>(
			id(0),
			ConsensusStatus::Gained { most_recent: None, new: vec![0x01, 0x02, 0x03] },
		);
		assert_ok!(WitnessExactValueWithoutStorage::on_finalize::<
			MockAccess<WitnessExactValueWithoutStorage>,
		>(vec![id(0)], &()));

		assert_eq!(
			WitnessExactValueWithoutStorage::take_election_result::<
				MockAccess<WitnessExactValueWithoutStorage>,
			>(mock_id.clone()),
			None,
		);

		// Only store into the state map if `on_consensus` returns true.
		assert_ok!(WitnessExactValueWithStorage::witness_exact_value::<
			MockAccess<WitnessExactValueWithStorage>,
		>(mock_id.clone()));
		MockStorageAccess::set_consensus_status::<WitnessExactValueWithStorage>(
			id(1),
			ConsensusStatus::Gained { most_recent: None, new: vec![0x00] },
		);
		assert_ok!(WitnessExactValueWithStorage::on_finalize::<
			MockAccess<WitnessExactValueWithStorage>,
		>(vec![id(1)], &()));

		// First successful lookup `takes` the storage value.
		assert_eq!(
			WitnessExactValueWithStorage::take_election_result::<
				MockAccess<WitnessExactValueWithStorage>,
			>(mock_id.clone()),
			Some(vec![0x00]),
		);
	});
}
