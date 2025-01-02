
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
use super::primitives::ElectionTracker;
use super::helpers::*;


#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BWSettings {
	pub safe_mode_enabled: bool,
	pub max_concurrent_elections: u32,
}

pub trait BWHooks<N, InputIndex> {
	fn active_deposit_channels_at(height: N) -> InputIndex;
}

pub trait Hook<A,B> {
	fn run(input: A) -> B;
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BWState<N: Ord> {
	elections: ElectionTracker<N>,
}

impl<N: Ord> Validate for BWState<N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.elections.is_valid()
	}
}

impl<N: BlockZero + Ord> Default for BWState<N> {
	fn default() -> Self {
		Self { elections: Default::default() }
	}
}




pub struct BWStateMachine<
	ElectionProperties,
	BlockData,
	N,
	BlockElectionPropertiesGenerator: Hook<N, ElectionProperties>
	> {
	_phantom: sp_std::marker::PhantomData<(ElectionProperties, BlockData, N, BlockElectionPropertiesGenerator)>,
}

impl<
	ElectionProperties: PartialEq + Clone + sp_std::fmt::Debug + 'static,
	BlockData: PartialEq + Clone + sp_std::fmt::Debug + 'static,
	N : Copy + Ord + Step + sp_std::fmt::Debug + 'static,
	BlockElectionPropertiesGenerator: Hook<N, ElectionProperties> + 'static
> StateMachine for BWStateMachine<ElectionProperties, BlockData, N, BlockElectionPropertiesGenerator> {
	type Input = SMInput<IndexAndValue<(N, ElectionProperties, u32), BlockData>, ChainProgress<N>>;

	type Settings = BWSettings;

	type Output = Result<(), &'static str>;

	type State = BWState<N>;

	fn input_index(s: &Self::State) -> Vec<IndexOf<Self::Input>> {
		s.elections.ongoing.clone().into_iter().map(|(height, extra)| (height, BlockElectionPropertiesGenerator::run(height), extra)).collect()
	}

	fn step(s: &mut Self::State, i: Self::Input, settings: &Self::Settings) -> Self::Output {
		log::info!("BW: input {i:?}");
		match i {
			SMInput::Context(ChainProgress::Reorg(range) | ChainProgress::Continuous(range)) => {
				s.elections.schedule_up_to(*range.end());
				for election in range {
					s.elections.restart_election(election);
				}
			},

			SMInput::Context(ChainProgress::WaitingForFirstConsensus | ChainProgress::None(_)) => {},

			SMInput::Vote(blockdata) => {
				// insert blockdata into our cache of blocks
				s.elections.mark_election_done(blockdata.0.0);
				log::info!("got block data: {:?}", blockdata.1);
			},
		};

		if !settings.safe_mode_enabled {
			s.elections.start_more_elections(settings.max_concurrent_elections as usize);
		}

		log::info!("BW: done. current elections: {:?}", s.elections.ongoing);

		Ok(())
	}

	#[cfg(test)]
	fn step_specification(before: &Self::State, input: &Self::Input, settings: &Self::Settings, after: &Self::State) {
		assert!(
			// there should always be at most as many elections as given in the settings
			after.elections.ongoing.len() <= settings.max_concurrent_elections as usize, 
			"too many concurrent elections"
		);

		use SMInput::*;
		use ChainProgress::*;
		match input {
			Vote(IndexAndValue((height, _, _), _)) => {
				assert!(
					// after receiving a vote, the ongoing elections should be the same as previously,
					// except with the vote's height removed
					after.elections.ongoing.key_set().with(*height) == before.elections.ongoing.key_set(),
					"wrong ongoing election set after received vote"
				)
			},

			Context(Reorg(range) | Continuous(range)) => {

				let all_elections = before.elections.ongoing.key_set().merge(range.clone().into_set());

				assert!(
					// There should be exactly those elections ongoing which have the lowest heights (at most max_concurrent_elections of them)
					all_elections.into_iter().take(settings.max_concurrent_elections as usize).collect::<BTreeSet<_>>() == after.elections.ongoing.key_set(),
					"wrong ongoing election after receiving new block height range"
				)
			},

			Context(WaitingForFirstConsensus | None(_)) => (),
		}
	}

}
