use core::hash::Hash;

use crate::{
	electoral_systems::state_machine::consensus::{
		ConsensusMechanism, SupermajorityConsensus, Threshold,
	},
	SharedDataHash,
};
use frame_support::Hashable;
use sp_std::collections::btree_map::BTreeMap;

use super::state_machine::{BWElectionProperties, BWTypes};

pub struct BWConsensus<T: BWTypes> {
	pub consensus: SupermajorityConsensus<SharedDataHash>,
	pub data: BTreeMap<SharedDataHash, (T::BlockData, Option<T::ChainBlockHash>)>,
	pub _phantom: sp_std::marker::PhantomData<T>,
}

impl<T: BWTypes> Default for BWConsensus<T> {
	fn default() -> Self {
		Self {
			consensus: Default::default(),
			data: Default::default(),
			_phantom: Default::default(),
		}
	}
}

impl<T: BWTypes> ConsensusMechanism for BWConsensus<T>
where
	(T::BlockData, Option<T::ChainBlockHash>): Hashable,
{
	type Vote = (T::BlockData, Option<T::ChainBlockHash>);
	type Result = (T::BlockData, Option<T::ChainBlockHash>);
	type Settings = (Threshold, BWElectionProperties<T>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		let vote_hash = SharedDataHash::of(&vote);
		self.data.entry(vote_hash).or_insert_with(|| vote.clone());
		self.consensus.insert_vote(vote_hash);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		self.consensus
			.check_consensus(&settings.0)
			.map(|consensus| self.data.get(&consensus).expect("hash of vote should exist").clone())
	}
}
