
use sp_std::collections::btree_map::BTreeMap;

/// Abstract consensus mechanism.
///
/// This trait is an abstraction over simple consensus mechanisms,
/// where there is the concept of incrementally adding votes,
/// and checking if the votes result in consensus.
pub trait ConsensusMechanism: Default {
	/// type of votes.
	type Vote;

	/// result type of the consensus.
	type Result;

	/// additional information required to check consensus
	type Settings;

	fn insert_vote(&mut self, vote: Self::Vote);
	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result>;
}

//-----------------------------------------------
// majority consensus

/// Simple implementation of a (super-)majority consensus
pub struct SupermajorityConsensus<Vote: PartialEq> {
	votes: BTreeMap<Vote, u32>,
}

pub struct Threshold {
	pub threshold: u32,
}

impl<Vote: PartialEq> Default for SupermajorityConsensus<Vote> {
	fn default() -> Self {
		Self { votes: Default::default() }
	}
}

impl<Vote: Ord + PartialEq + Clone> ConsensusMechanism for SupermajorityConsensus<Vote> {
	type Vote = Vote;
	type Result = Vote;
	type Settings = Threshold;

	fn insert_vote(&mut self, vote: Self::Vote) {
		if let Some(count) = self.votes.get_mut(&vote) {
			*count += 1;
		} else {
			self.votes.insert(vote, 1);
		}
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		let best = self.votes.iter().last();

		if let Some((best_vote, best_count)) = best {
			if best_count >= &settings.threshold {
				return Some(best_vote.clone());
			}
		}

		return None;
	}
}

//-----------------------------------------------
// staged consensus

/// Staged consensus.
///
/// Votes are indexed by stages, and each stage is evaluated
/// separately. Evaluation happens in reverse order of the stage index,
/// i.e. the highest stage which achieves consensus determines the result.
/// If no stage achieves consensus, the result is inconclusive.
pub struct StagedConsensus<Stage: ConsensusMechanism, Index: Ord> {
	stages: BTreeMap<Index, Stage>,
}

impl<Stage: ConsensusMechanism, Index: Ord> StagedConsensus<Stage, Index> {
	pub fn new() -> Self {
		Self { stages: BTreeMap::new() }
	}
}

impl<Stage: ConsensusMechanism, Index: Ord> Default for StagedConsensus<Stage, Index> {
	fn default() -> Self {
		Self { stages: Default::default() }
	}
}

impl<Stage: ConsensusMechanism, Index: Ord + Copy> ConsensusMechanism
	for StagedConsensus<Stage, Index>
{
	type Result = Stage::Result;
	type Vote = (Index, Stage::Vote);
	type Settings = Stage::Settings;

	fn insert_vote(&mut self, (index, vote): Self::Vote) {
		if let Some(stage) = self.stages.get_mut(&index) {
			stage.insert_vote(vote)
		} else {
			let mut stage = Stage::default();
			stage.insert_vote(vote);
			self.stages.insert(index, stage);
		}
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		// we check all stages starting with the highest index,
		// the first one that has consensus wins
		for (_, stage) in self.stages.iter().rev() {
			if let Some(result) = stage.check_consensus(settings) {
				return Some(result);
			}
		}

		None
	}
}
