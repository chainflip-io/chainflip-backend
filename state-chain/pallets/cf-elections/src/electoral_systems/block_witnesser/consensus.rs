use crate::{
	electoral_systems::state_machine::{
		consensus::{ConsensusMechanism, SupermajorityConsensus, Threshold},
		core::ConstantIndex,
	},
	SharedDataHash,
};
use frame_support::Hashable;
use sp_std::collections::btree_map::BTreeMap;

pub struct BWConsensus<BlockData: Eq, N, ElectionProperties> {
	pub consensus: SupermajorityConsensus<SharedDataHash>,
	pub data: BTreeMap<SharedDataHash, BlockData>,
	pub _phantom: sp_std::marker::PhantomData<(N, ElectionProperties)>,
}

impl<BlockData: Eq, N, ElectionProperties> Default
	for BWConsensus<BlockData, N, ElectionProperties>
{
	fn default() -> Self {
		Self {
			consensus: Default::default(),
			data: Default::default(),
			_phantom: Default::default(),
		}
	}
}

impl<
		BlockData: Eq + Clone + sp_std::fmt::Debug + Hashable,
		N: Clone,
		ElectionProperties: Clone,
	> ConsensusMechanism for BWConsensus<BlockData, N, ElectionProperties>
{
	type Vote = ConstantIndex<(N, ElectionProperties, u8), BlockData>;
	type Result = ConstantIndex<(N, ElectionProperties, u8), BlockData>;
	type Settings = (Threshold, (N, ElectionProperties, u8));

	fn insert_vote(&mut self, vote: Self::Vote) {
		let vote_hash = SharedDataHash::of(&vote.data);
		self.data.insert(vote_hash, vote.data.clone());
		self.consensus.insert_vote(vote_hash);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		self.consensus
			.check_consensus(&settings.0)
			.map(|consensus| self.data.get(&consensus).expect("hash of vote should exist").clone())
			.map(|data| ConstantIndex::new(data))
	}
}
