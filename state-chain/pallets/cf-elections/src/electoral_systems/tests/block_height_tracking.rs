use crate::electoral_systems::{
	block_height_tracking::{
		consensus::BlockHeightTrackingConsensus,
		primitives::{validate_vote_and_height, Header, VoteValidationError},
		state_machine::InputHeaders,
		ChainTypes, HWTypes, HeightWitnesserProperties,
	},
	state_machine::{
		consensus::{ConsensusMechanism, Threshold},
		core::{hook_test_utils::EmptyHook, TypesFor},
	},
};
use cf_chains::{mocks::MockEthereum, Chain};
use std::collections::{BTreeSet, VecDeque};
type ChainBlockNumber = <MockEthereum as Chain>::ChainBlockNumber;
type ValidatorId = u16;
type BlockData = Vec<u8>;
type ElectionProperties = BTreeSet<u16>;
type ElectionCount = u16;

struct BlockHeightWitnesserDefinition;

type BHTypes = TypesFor<BlockHeightWitnesserDefinition>;

impl ChainTypes for BHTypes {
	type ChainBlockNumber = u64;
	type ChainBlockHash = u64;
	const SAFETY_MARGIN: u32 = 8;
}

impl HWTypes for BHTypes {
	const BLOCK_BUFFER_SIZE: usize = 6;
	type BlockHeightChangeHook = EmptyHook;
}

const BHW_PROPERTIES_STARTUP: HeightWitnesserProperties<BHTypes> =
	HeightWitnesserProperties { witness_from_index: 0 };
const BHW_PROPERTIES_RUNNING: HeightWitnesserProperties<BHTypes> =
	HeightWitnesserProperties { witness_from_index: 5 };

/// In case we are bootstrapping the HW election, the first consensus will return the highest block
/// we reach supermajority over.
#[test]
fn block_height_witnesser_first_consensus() {
	let mut bh_consensus: BlockHeightTrackingConsensus<BHTypes> =
		BlockHeightTrackingConsensus::default();

	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([
		Header::<BHTypes> { block_height: 5, hash: 1234, parent_hash: 000 },
		Header::<BHTypes> { block_height: 6, hash: 1234, parent_hash: 000 },
	])));
	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([
		Header::<BHTypes> { block_height: 6, hash: 1234, parent_hash: 000 },
		Header::<BHTypes> { block_height: 7, hash: 1234, parent_hash: 000 },
	])));
	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([
		Header::<BHTypes> { block_height: 5, hash: 1234, parent_hash: 000 },
		Header::<BHTypes> { block_height: 6, hash: 1234, parent_hash: 000 },
	])));
	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([Header::<BHTypes> {
		block_height: 5,
		hash: 1234,
		parent_hash: 000,
	}])));
	let consensus =
		bh_consensus.check_consensus(&(Threshold { threshold: 3 }, BHW_PROPERTIES_STARTUP));
	assert_eq!(
		consensus,
		Some(InputHeaders::<BHTypes>(VecDeque::from([Header::<BHTypes> {
			block_height: 6,
			hash: 1234,
			parent_hash: 000,
		},])))
	)
}

/// In case we are running the consensus will return the longest sub-chain of continuous blocks
/// The sub-chain in order to be valid needs to start from the correct `witness_from_index` and have
/// all the correct hashes matching (`parent_hash` matching previous block hash)
#[test]
fn block_height_witnesser_running_consensus() {
	let mut bh_consensus: BlockHeightTrackingConsensus<BHTypes> =
		BlockHeightTrackingConsensus::default();

	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([
		Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
		Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 },
		Header::<BHTypes> { block_height: 7, hash: 7, parent_hash: 6 },
	])));
	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([
		Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
		Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 },
		Header::<BHTypes> { block_height: 7, hash: 7, parent_hash: 6 },
	])));
	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([
		Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
		Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 },
	])));
	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([
		Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
		Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 },
		Header::<BHTypes> { block_height: 7, hash: 777, parent_hash: 6 },
	])));
	let consensus =
		bh_consensus.check_consensus(&(Threshold { threshold: 3 }, BHW_PROPERTIES_RUNNING));
	assert_eq!(
		consensus,
		Some(InputHeaders::<BHTypes>(VecDeque::from([
			Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
			Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 },
		])))
	);
	bh_consensus.insert_vote(InputHeaders::<BHTypes>(VecDeque::from([
		Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
		Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 },
		Header::<BHTypes> { block_height: 7, hash: 7, parent_hash: 6 },
	])));
	let consensus =
		bh_consensus.check_consensus(&(Threshold { threshold: 3 }, BHW_PROPERTIES_RUNNING));
	assert_eq!(
		consensus,
		Some(InputHeaders::<BHTypes>(VecDeque::from([
			Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
			Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 },
			Header::<BHTypes> { block_height: 7, hash: 7, parent_hash: 6 },
		])))
	);
}

#[test]
fn test_validate_vote_and_height() {
	let result = validate_vote_and_height(
		BHW_PROPERTIES_RUNNING.witness_from_index,
		&VecDeque::<Header<BHTypes>>::from([]),
	);
	assert_eq!(result.unwrap_err(), VoteValidationError::EmptyVote);
	let result = validate_vote_and_height(
		BHW_PROPERTIES_RUNNING.witness_from_index,
		&VecDeque::from([
			Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
			Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 },
		]),
	);
	assert!(result.is_ok());
	let result = validate_vote_and_height(
		BHW_PROPERTIES_RUNNING.witness_from_index,
		&VecDeque::from([Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 5 }]),
	);
	assert_eq!(result.unwrap_err(), VoteValidationError::BlockNotMatchingRequestedHeight);

	let result = validate_vote_and_height(
		BHW_PROPERTIES_RUNNING.witness_from_index,
		&VecDeque::from([
			Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
			Header::<BHTypes> { block_height: 6, hash: 6, parent_hash: 4 },
		]),
	);
	assert_eq!(result.unwrap_err(), VoteValidationError::ParentHashMismatch);

	let result = validate_vote_and_height(
		BHW_PROPERTIES_RUNNING.witness_from_index,
		&VecDeque::from([
			Header::<BHTypes> { block_height: 5, hash: 5, parent_hash: 0 },
			Header::<BHTypes> { block_height: 7, hash: 7, parent_hash: 7 },
		]),
	);
	assert_eq!(result.unwrap_err(), VoteValidationError::BlockHeightsNotContinuous);
}
