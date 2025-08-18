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

	/// Every vote should be representable as consensus.
	fn vote_as_consensus(vote: &Self::Vote) -> Self::Result;
	fn insert_vote(&mut self, vote: Self::Vote);
	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result>;
}

//-----------------------------------------------
// majority consensus

/// Simple implementation of a (super-)majority consensus, in case of ties the last element is
/// chosen
pub struct SupermajorityConsensus<Vote: PartialEq> {
	votes: BTreeMap<Vote, u32>,
}

pub struct SuccessThreshold {
	pub success_threshold: u32,
}

impl<Vote: PartialEq> Default for SupermajorityConsensus<Vote> {
	fn default() -> Self {
		Self { votes: Default::default() }
	}
}

impl<Vote: Ord + PartialEq + Clone> ConsensusMechanism for SupermajorityConsensus<Vote> {
	type Vote = Vote;
	type Result = Vote;
	type Settings = SuccessThreshold;

	fn insert_vote(&mut self, vote: Self::Vote) {
		*self.votes.entry(vote).or_insert(0) += 1;
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		self.votes.iter().max_by_key(|(_, count)| *count).and_then(|(vote, count)| {
			if *count >= settings.success_threshold {
				Some(vote.clone())
			} else {
				None
			}
		})
	}

	fn vote_as_consensus(vote: &Self::Vote) -> Self::Result {
		vote.clone()
	}
}

//-----------------------------------------------
// staged consensus

/// Staged consensus.
///
/// Votes are indexed by stages, and each stage is evaluated
/// separately. Evaluation happens from highest priority to lowest,
/// i.e. the highest priority stage which achieves consensus determines the result.
/// If no stage achieves consensus, the result is inconclusive.
pub struct StagedConsensus<Stage: ConsensusMechanism, Priority: Ord> {
	stages: BTreeMap<Priority, Stage>,
}

pub struct StagedVote<Stage: ConsensusMechanism, Priority: Ord> {
	pub priority: Priority,
	pub vote: Stage::Vote,
}

impl<Stage: ConsensusMechanism, Priority: Ord> From<(Priority, Stage::Vote)>
	for StagedVote<Stage, Priority>
{
	fn from((priority, vote): (Priority, Stage::Vote)) -> Self {
		Self { priority, vote }
	}
}

impl<Stage: ConsensusMechanism, Priority: Ord> StagedConsensus<Stage, Priority> {
	pub fn new() -> Self {
		Self { stages: BTreeMap::new() }
	}
}

impl<Stage: ConsensusMechanism, Priority: Ord> Default for StagedConsensus<Stage, Priority> {
	fn default() -> Self {
		Self { stages: Default::default() }
	}
}

impl<Stage: ConsensusMechanism, Priority: Ord + Copy> ConsensusMechanism
	for StagedConsensus<Stage, Priority>
{
	type Result = Stage::Result;
	type Vote = StagedVote<Stage, Priority>;
	type Settings = Stage::Settings;

	fn insert_vote(&mut self, vote: Self::Vote) {
		self.stages.entry(vote.priority).or_default().insert_vote(vote.vote);
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

	fn vote_as_consensus(vote: &Self::Vote) -> Self::Result {
		Stage::vote_as_consensus(&vote.vote)
	}
}

#[cfg(test)]
mod tests {
	use crate::electoral_systems::state_machine::consensus::{
		ConsensusMechanism, StagedConsensus, SuccessThreshold, SupermajorityConsensus,
	};

	#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Debug)]
	struct MockVote(u32);

	///Consensus is reached when the count of a vote is >= then the threshold
	/// in case of a tie the last vote is the one chosen (based on the ordering)
	#[test]
	fn test_super_majority_consensus() {
		let mut supermajority = SupermajorityConsensus::<MockVote>::default();

		supermajority.insert_vote(MockVote(1));
		supermajority.insert_vote(MockVote(1));
		supermajority.insert_vote(MockVote(2));
		supermajority.insert_vote(MockVote(2));
		let consensus = supermajority.check_consensus(&SuccessThreshold { success_threshold: 3 });
		assert_eq!(consensus, None);

		supermajority.insert_vote(MockVote(1));
		let consensus = supermajority.check_consensus(&SuccessThreshold { success_threshold: 3 });
		assert_eq!(consensus, Some(MockVote(1)));

		supermajority.insert_vote(MockVote(2));
		// ??
		let consensus = supermajority.check_consensus(&SuccessThreshold { success_threshold: 3 });
		assert_eq!(consensus, Some(MockVote(2)));
	}

	#[test]
	fn test_staged_consensus() {
		let mut staged = StagedConsensus::<SupermajorityConsensus<MockVote>, u32>::default();
		staged.insert_vote((1, MockVote(2)).into());
		staged.insert_vote((1, MockVote(2)).into());
		staged.insert_vote((1, MockVote(2)).into());
		let consensus = staged.check_consensus(&SuccessThreshold { success_threshold: 3 });
		assert_eq!(consensus, Some(MockVote(2)));

		staged.insert_vote((2, MockVote(1)).into());
		staged.insert_vote((2, MockVote(1)).into());
		staged.insert_vote((2, MockVote(2)).into());
		let consensus = staged.check_consensus(&SuccessThreshold { success_threshold: 3 });
		assert_eq!(consensus, Some(MockVote(2)));

		staged.insert_vote((3, MockVote(1)).into());
		staged.insert_vote((3, MockVote(1)).into());
		staged.insert_vote((3, MockVote(1)).into());
		let consensus = staged.check_consensus(&SuccessThreshold { success_threshold: 3 });
		assert_eq!(consensus, Some(MockVote(1)));

		staged.insert_vote((4, MockVote(6)).into());
		staged.insert_vote((4, MockVote(6)).into());
		staged.insert_vote((4, MockVote(6)).into());
		let consensus = staged.check_consensus(&SuccessThreshold { success_threshold: 3 });
		assert_eq!(consensus, Some(MockVote(6)));
	}
}
