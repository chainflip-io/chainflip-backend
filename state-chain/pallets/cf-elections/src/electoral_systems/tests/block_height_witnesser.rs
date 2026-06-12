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
use crate::electoral_systems::{
	block_height_witnesser::{
		consensus::BlockHeightWitnesserConsensus,
		primitives::{Header, NonemptyContinuousHeaders, NonemptyContinuousHeadersError},
		state_machine::{BlockHeightWitnesser, VoteValidationError},
		BHWTypes, ChainTypes, HeightWitnesserProperties,
	},
	state_machine::{
		consensus::{ConsensusMechanism, SuccessThreshold},
		state_machine::AbstractApi,
	},
};
use cf_traits::hook_test_utils::EmptyHook;
use cf_utilities::define_empty_struct;

define_empty_struct! { struct BHTypes; }

impl ChainTypes for BHTypes {
	type ChainBlockNumber = u64;
	type ChainBlockHash = u64;
	const NAME: &'static str = "Mock";
}

impl BHWTypes for BHTypes {
	type Chain = BHTypes;
	type BlockHeightChangeHook = EmptyHook;

	type ReorgHook = EmptyHook;
}

const BHW_PROPERTIES_STARTUP: HeightWitnesserProperties<BHTypes> =
	HeightWitnesserProperties { witness_from_index: 0 };
const BHW_PROPERTIES_RUNNING: HeightWitnesserProperties<BHTypes> =
	HeightWitnesserProperties { witness_from_index: 5 };

/// In case we are bootstrapping the HW election, the first consensus will return the highest block
/// we reach supermajority over.
#[test]
fn block_height_witnesser_first_consensus() {
	let mut bh_consensus: BlockHeightWitnesserConsensus<BHTypes> =
		BlockHeightWitnesserConsensus::default();

	bh_consensus.insert_vote(
		[
			Header { block_height: 5, hash: 1234, parent_hash: 000 },
			Header { block_height: 6, hash: 1234, parent_hash: 000 },
		]
		.into(),
	);
	bh_consensus.insert_vote(
		[
			Header { block_height: 6, hash: 1234, parent_hash: 000 },
			Header { block_height: 7, hash: 1234, parent_hash: 000 },
		]
		.into(),
	);
	bh_consensus.insert_vote(
		[
			Header { block_height: 5, hash: 1234, parent_hash: 000 },
			Header { block_height: 6, hash: 1234, parent_hash: 000 },
		]
		.into(),
	);
	bh_consensus.insert_vote([Header { block_height: 5, hash: 1234, parent_hash: 000 }].into());
	let consensus = bh_consensus
		.check_consensus(&(SuccessThreshold { success_threshold: 3 }, BHW_PROPERTIES_STARTUP));
	assert_eq!(
		consensus,
		Some(NonemptyContinuousHeaders::new(Header::<BHTypes> {
			block_height: 6,
			hash: 1234,
			parent_hash: 000
		}))
	)
}

/// In case we are running the consensus will return the longest sub-chain of continuous blocks
/// The sub-chain in order to be valid needs to start from the correct `witness_from_index` and have
/// all the correct hashes matching (`parent_hash` matching previous block hash)
#[test]
fn block_height_witnesser_running_consensus() {
	let mut bh_consensus: BlockHeightWitnesserConsensus<BHTypes> =
		BlockHeightWitnesserConsensus::default();

	bh_consensus.insert_vote(
		[
			Header { block_height: 5, hash: 5, parent_hash: 0 },
			Header { block_height: 6, hash: 6, parent_hash: 5 },
			Header { block_height: 7, hash: 7, parent_hash: 6 },
		]
		.into(),
	);
	bh_consensus.insert_vote(
		[
			Header { block_height: 5, hash: 5, parent_hash: 0 },
			Header { block_height: 6, hash: 6, parent_hash: 5 },
			Header { block_height: 7, hash: 7, parent_hash: 6 },
		]
		.into(),
	);
	bh_consensus.insert_vote(
		[
			Header { block_height: 5, hash: 5, parent_hash: 0 },
			Header { block_height: 6, hash: 6, parent_hash: 5 },
		]
		.into(),
	);
	bh_consensus.insert_vote(
		[
			Header { block_height: 5, hash: 5, parent_hash: 0 },
			Header { block_height: 6, hash: 6, parent_hash: 5 },
			Header { block_height: 7, hash: 777, parent_hash: 6 },
		]
		.into(),
	);
	let consensus = bh_consensus
		.check_consensus(&(SuccessThreshold { success_threshold: 3 }, BHW_PROPERTIES_RUNNING));
	assert_eq!(
		consensus,
		Some(
			NonemptyContinuousHeaders::<BHTypes>::try_new(
				[
					Header { block_height: 5, hash: 5, parent_hash: 0 },
					Header { block_height: 6, hash: 6, parent_hash: 5 },
				]
				.into()
			)
			.unwrap()
		)
	);
	bh_consensus.insert_vote(
		[
			Header { block_height: 5, hash: 5, parent_hash: 0 },
			Header { block_height: 6, hash: 6, parent_hash: 5 },
			Header { block_height: 7, hash: 7, parent_hash: 6 },
		]
		.into(),
	);
	let consensus = bh_consensus
		.check_consensus(&(SuccessThreshold { success_threshold: 3 }, BHW_PROPERTIES_RUNNING));
	assert_eq!(
		consensus,
		Some(
			NonemptyContinuousHeaders::<BHTypes>::try_new(
				[
					Header { block_height: 5, hash: 5, parent_hash: 0 },
					Header { block_height: 6, hash: 6, parent_hash: 5 },
					Header { block_height: 7, hash: 7, parent_hash: 6 },
				]
				.into()
			)
			.unwrap()
		)
	);
}

#[test]
fn test_validate_vote_and_height() {
	let result = BlockHeightWitnesser::<BHTypes>::validate(
		&BHW_PROPERTIES_RUNNING,
		&[
			Header { block_height: 5, hash: 5, parent_hash: 0 },
			Header { block_height: 6, hash: 6, parent_hash: 5 },
		]
		.into(),
	);
	assert!(result.is_ok());
	let result = BlockHeightWitnesser::<BHTypes>::validate(
		&BHW_PROPERTIES_RUNNING,
		&[Header { block_height: 6, hash: 6, parent_hash: 5 }].into(),
	);
	assert_eq!(result.unwrap_err(), VoteValidationError::BlockNotMatchingRequestedHeight);

	let result = BlockHeightWitnesser::<BHTypes>::validate(
		&BHW_PROPERTIES_RUNNING,
		&[
			Header { block_height: 5, hash: 5, parent_hash: 0 },
			Header { block_height: 6, hash: 6, parent_hash: 4 },
		]
		.into(),
	);
	assert_eq!(
		result.unwrap_err(),
		VoteValidationError::NonemptyContinuousHeadersError(
			NonemptyContinuousHeadersError::matching_hashes
		)
	);

	let result = BlockHeightWitnesser::<BHTypes>::validate(
		&BHW_PROPERTIES_RUNNING,
		&[
			Header { block_height: 5, hash: 5, parent_hash: 0 },
			Header { block_height: 7, hash: 7, parent_hash: 5 },
		]
		.into(),
	);
	assert_eq!(
		result.unwrap_err(),
		VoteValidationError::NonemptyContinuousHeadersError(
			NonemptyContinuousHeadersError::continuous_heights
		)
	);
}
