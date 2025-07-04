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

use cf_primitives::AuthorityCount;

use super::{mocks::*, register_checks};
use crate::{
	electoral_system::ConsensusStatus,
	electoral_systems::{tests::utils::generate_votes, unsafe_median::*},
};

pub struct FeeHook;
impl UpdateFeeHook<u64> for FeeHook {
	fn update_fee(_fee: u64) {}
}
type SimpleUnsafeMedian = UnsafeMedian<u64, (), (), FeeHook, (), u32>;

const INIT_UNSYNCHRONISED_STATE: u64 = 22;
const NEW_UNSYNCHRONISED_STATE: u64 = 33;

fn with_default_setup() -> TestSetup<SimpleUnsafeMedian> {
	TestSetup::<_>::default().with_unsynchronised_state(INIT_UNSYNCHRONISED_STATE)
}

fn with_default_context() -> TestContext<SimpleUnsafeMedian> {
	with_default_setup().build_with_initial_election()
}

register_checks! {
	SimpleUnsafeMedian {
		started_at_initial_state(pre_finalize, _a) {
			assert_eq!(
				pre_finalize.unsynchronised_state,
				INIT_UNSYNCHRONISED_STATE,
				"Expected initial state pre-finalization."
			);
		},
		ended_at_initial_state(_pre, post_finalize) {
			assert_eq!(
				post_finalize.unsynchronised_state,
				INIT_UNSYNCHRONISED_STATE,
				"Expected initial state post-finalization."
			);
		},
		ended_at_new_state(_pre, post_finalize) {
			assert_eq!(
				post_finalize.unsynchronised_state,
				NEW_UNSYNCHRONISED_STATE,
				"Expected new state post-finalization."
			);
		},
	}
}

#[test]
fn if_consensus_update_unsynchronised_state() {
	with_default_context()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: NEW_UNSYNCHRONISED_STATE,
		})
		.test_on_finalize(
			&(),
			|_| {},
			vec![
				Check::started_at_initial_state(),
				Check::ended_at_new_state(),
				Check::last_election_deleted(),
				Check::election_id_incremented(),
			],
		);
}

#[test]
fn if_no_consensus_do_not_update_unsynchronised_state() {
	with_default_context()
		.force_consensus_update(ConsensusStatus::None)
		.test_on_finalize(
			&(),
			|_| {},
			vec![
				Check::started_at_initial_state(),
				Check::ended_at_initial_state(),
				Check::assert_unchanged(),
			],
		);
}

#[test]
fn check_consensus_correctly_calculates_median_when_all_authorities_vote() {
	const AUTHORITY_COUNT: AuthorityCount = 10;

	with_default_context().expect_consensus(
		generate_votes(AUTHORITY_COUNT, AUTHORITY_COUNT),
		Some((AUTHORITY_COUNT / 2) as u64 - 1u64),
	);
}

// Note: This is the reason the median is "unsafe" as 1/3 of validators can influence the value
// in this case.
#[test]
fn check_consensus_correctly_calculates_median_when_exactly_super_majority_authorities_vote() {
	const AUTHORITY_COUNT: AuthorityCount = 10;
	const THRESHOLD: AuthorityCount = cf_utilities::threshold_from_share_count(AUTHORITY_COUNT);
	const SUCCESS_THRESHOLD: AuthorityCount =
		cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT);

	// Default is no consensus:
	with_default_context().expect_consensus(generate_votes(0, AUTHORITY_COUNT), None);
	// Threshold number of votes is not enough:
	with_default_context().expect_consensus(generate_votes(THRESHOLD, AUTHORITY_COUNT), None);
	// // Success threshold number of votes is enough:
	with_default_context().expect_consensus(
		generate_votes(SUCCESS_THRESHOLD, AUTHORITY_COUNT),
		Some((SUCCESS_THRESHOLD / 2) as u64),
	);
}
