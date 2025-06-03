use crate::electoral_systems::{
	block_height_tracking::{
		consensus::BlockHeightTrackingConsensus,
		primitives::{Header, NonemptyContinuousHeaders, NonemptyContinuousHeadersError},
		state_machine::{BlockHeightWitnesser, VoteValidationError},
		BHWTypes, ChainTypes, HeightWitnesserProperties,
	},
	state_machine::{
		consensus::{ConsensusMechanism, SuccessThreshold},
		core::{hook_test_utils::EmptyHook, TypesFor},
		state_machine::AbstractApi,
	},
};

struct BlockHeightWitnesserDefinition;

type BHTypes = TypesFor<BlockHeightWitnesserDefinition>;

impl ChainTypes for BHTypes {
	type ChainBlockNumber = u64;
	type ChainBlockHash = u64;
	const SAFETY_BUFFER: usize = 8;
}

impl BHWTypes for BHTypes {
	type Chain = BHTypes;
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
		Some(NonemptyContinuousHeaders {
			headers: [Header::<BHTypes> { block_height: 6, hash: 1234, parent_hash: 000 }].into()
		})
	)
}

/// In case we are running the consensus will return the longest sub-chain of continuous blocks
/// The sub-chain in order to be valid needs to start from the correct `witness_from_index` and have
/// all the correct hashes matching (`parent_hash` matching previous block hash)
#[test]
fn block_height_witnesser_running_consensus() {
	let mut bh_consensus: BlockHeightTrackingConsensus<BHTypes> =
		BlockHeightTrackingConsensus::default();

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
		Some(NonemptyContinuousHeaders::<BHTypes> {
			headers: [
				Header { block_height: 5, hash: 5, parent_hash: 0 },
				Header { block_height: 6, hash: 6, parent_hash: 5 },
			]
			.into()
		})
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
		Some(NonemptyContinuousHeaders::<BHTypes> {
			headers: [
				Header { block_height: 5, hash: 5, parent_hash: 0 },
				Header { block_height: 6, hash: 6, parent_hash: 5 },
				Header { block_height: 7, hash: 7, parent_hash: 6 },
			]
			.into()
		})
	);
}

#[test]
fn test_validate_vote_and_height() {
	let result = BlockHeightWitnesser::<BHTypes>::validate(&BHW_PROPERTIES_RUNNING, &[].into());
	assert_eq!(
		result.unwrap_err(),
		VoteValidationError::NonemptyContinuousHeadersError(
			NonemptyContinuousHeadersError::is_nonempty
		)
	);
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
