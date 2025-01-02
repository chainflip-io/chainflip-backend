use core::{iter::Step, ops::RangeInclusive};
use cf_chains::witness_period::BlockZero;
use codec::{Decode, Encode};
use frame_support::{ensure, Hashable};
use log::trace;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque};
use sp_std::vec::Vec;
use sp_std::ops::Add;

use itertools::Either;

use crate::electoral_systems::block_height_tracking::state_machine::IndexAndValue;
use crate::electoral_systems::block_height_tracking::{
	consensus::{ConsensusMechanism, SupermajorityConsensus, Threshold}, state_machine::{ConstantIndex, IndexOf, StateMachine, Validate}, state_machine_es::SMInput, ChainProgress
};
use crate::{SharedData, SharedDataHash};

use super::BlockWitnesserSettings;


pub struct BWConsensus<BlockData: Eq, N, ElectionProperties> {
	pub consensus: SupermajorityConsensus<SharedDataHash>,
	pub data: BTreeMap::<SharedDataHash, BlockData>,
	pub _phantom: sp_std::marker::PhantomData<(N, ElectionProperties)>
}

impl<BlockData: Eq, N, ElectionProperties> Default for BWConsensus<BlockData, N, ElectionProperties> {
	fn default() -> Self {
		Self { consensus: Default::default(), data: Default::default(), _phantom: Default::default() }
	}
}

impl<BlockData: Eq + Clone + sp_std::fmt::Debug + Hashable, N: Clone, ElectionProperties: Clone> ConsensusMechanism for BWConsensus<BlockData, N, ElectionProperties> {
	type Vote = ConstantIndex<(N, ElectionProperties, u32), BlockData>;

	type Result = IndexAndValue<(N, ElectionProperties, u32), BlockData>;

	type Settings = (Threshold, (N, ElectionProperties, u32));

	fn insert_vote(&mut self, vote: Self::Vote) {
		let vote_hash = SharedDataHash::of(&vote.data);
		self.data.insert(vote_hash, vote.data.clone());
		self.consensus.insert_vote(vote_hash);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		self.consensus.check_consensus(&settings.0)
			.map(|consensus| self.data.get(&consensus).expect("hash of vote should exist").clone())
			.map(|data| IndexAndValue(settings.1.clone(), data))
	}
}
