use super::{
	super::state_machine::{
		core::Validate, state_machine::Statemachine, state_machine_es::SMInput,
	},
	primitives::{trim_to_length, ChainBlocks, Header, MergeFailure, VoteValidationError},
	ChainProgress, HWTypes, HeightWitnesserProperties,
};
use crate::electoral_systems::state_machine::core::{Hook, IndexedValidate};
use cf_chains::witness_period::{BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use frame_support::pallet_prelude::MaxEncodedLen;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::vec_deque::VecDeque, fmt::Debug, vec::Vec};

//------------------------ inputs ---------------------------
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct InputHeaders<Types: HWTypes>(
	pub VecDeque<Header<Types::ChainBlockHash, Types::ChainBlockNumber>>,
);

impl<T: HWTypes> IndexedValidate<HeightWitnesserProperties<T>, InputHeaders<T>>
	for BlockHeightTrackingSM<T>
{
	type Error = VoteValidationError;

	fn validate(
		base: &HeightWitnesserProperties<T>,
		this: &InputHeaders<T>,
	) -> Result<(), Self::Error> {
		this.is_valid()?;

		if base.witness_from_index.is_zero() {
			Ok(())
		} else {
			match this.0.front() {
				Some(first) if first.block_height == base.witness_from_index => Ok(()),
				Some(_) => Err(VoteValidationError::BlockNotMatchingRequestedHeight),
				None => Err(VoteValidationError::EmptyVote),
			}
		}
	}
}

impl<T: HWTypes> Validate for InputHeaders<T> {
	type Error = VoteValidationError;
	fn is_valid(&self) -> Result<(), Self::Error> {
		if self.0.is_empty() {
			Err(VoteValidationError::EmptyVote)
		} else {
			ChainBlocks { headers: self.0.clone() }.is_valid()
		}
	}
}

//------------------------ state ---------------------------

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum BHWState<T: HWTypes> {
	Starting,
	Running {
		headers: VecDeque<Header<T::ChainBlockHash, T::ChainBlockNumber>>,
		witness_from: T::ChainBlockNumber,
	},
}

impl<T: HWTypes> Default for BHWState<T> {
	fn default() -> Self {
		Self::Starting
	}
}

impl<T: HWTypes> Validate for BHWState<T> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		match self {
			BHWState::Starting => Ok(()),

			BHWState::Running { headers, witness_from: _ } =>
				if !headers.is_empty() {
					InputHeaders::<T>(headers.clone())
						.is_valid()
						.map_err(|_| "blocks should be continuous")
				} else {
					Err("Block height tracking state should always be non-empty after start-up.")
				},
		}
	}
}

#[derive(
	Debug,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	Deserialize,
	Serialize,
	Ord,
	PartialOrd,
	Default,
)]
pub struct BHWStateWrapper<T: HWTypes> {
	pub state: BHWState<T>,
	pub block_height_update: T::BlockHeightChangeHook,
}

impl<T: HWTypes> Validate for BHWStateWrapper<T> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.state.is_valid()
	}
}

//------------------------ state machine ---------------------------

pub struct BlockHeightTrackingSM<T: HWTypes> {
	_phantom: core::marker::PhantomData<T>,
}

impl<T: HWTypes> Statemachine for BlockHeightTrackingSM<T> {
	type State = BHWStateWrapper<T>;
	type Input = SMInput<(HeightWitnesserProperties<T>, InputHeaders<T>), ()>;
	type InputIndex = Vec<HeightWitnesserProperties<T>>;
	type Settings = ();
	type Output = Result<ChainProgress<T::ChainBlockNumber>, &'static str>;

	fn input_index(s: &mut Self::State) -> Self::InputIndex {
		let witness_from_index = match s.state {
			BHWState::Starting => T::ChainBlockNumber::zero(),
			BHWState::Running { headers: _, witness_from } => witness_from,
		};
		Vec::from([HeightWitnesserProperties { witness_from_index }])
	}

	// specification for step function
	#[cfg(test)]
	fn step_specification(
		before: &mut Self::State,
		input: &Self::Input,
		_output: &Self::Output,
		_settings: &Self::Settings,
		after: &Self::State,
	) {
		use cf_chains::witness_period::SaturatingStep;

		use BHWState::*;

		match (&before.state, &after.state) {
			(Starting, Starting) => assert!(
				*input == SMInput::Context(()),
				"BHW should remain in Starting state only if it doesn't get a vote as input."
			),

			(Starting, Running { .. }) => (),

			(Running { .. }, Starting) =>
				panic!("BHW should never transit into Starting state once its running."),

			(
				Running { headers: headers0, witness_from: from0 },
				Running { headers: headers1, witness_from: from1 },
			) => {
				assert!(
					// there are two different cases:
					// - in case of a reorg, the `witness_from` is reset to the beginning of the
					//   headers we have:
					(*from1 == headers0.front().unwrap().block_height)
					||
					// - in the normal case, the `witness_from` should always be the next
					//   height after the last header that we have
					(*from1 == headers1.back().unwrap().block_height.saturating_forward(1)),
					"witness_from should be either next height, or height of first header"
				);

				assert!(
					// if the input is *not* the empty context, then `witness_from` should
					// always change after running the transition function.
					// This ensures that we always have "fresh" election properties,
					// and are thus deleting/recreating elections as expected.
					(*input == SMInput::Context(())) || (*from1 != *from0),
					"witness_from should always change, except when we get a non-vote input"
				);
			},
		}
	}

	fn step(s: &mut Self::State, input: Self::Input, _settings: &()) -> Self::Output {
		let new_headers = match input {
			SMInput::Consensus((_properties, consensus)) => consensus,
			SMInput::Context(_) => return Ok(ChainProgress::None),
		};

		match &mut s.state {
			BHWState::Starting => {
				let first = new_headers.0.front().unwrap().block_height;
				let last = new_headers.0.back().unwrap().block_height;
				s.state = BHWState::Running {
					headers: new_headers.0.clone(),
					witness_from: last.saturating_forward(1),
				};
				Ok(ChainProgress::Range(first..=last))
			},

			BHWState::Running { headers, witness_from } => {
				let mut chainblocks = ChainBlocks { headers: headers.clone() };

				match chainblocks.merge(new_headers.0) {
					Ok(merge_info) => {
						log::info!(
							"added new blocks: {:?}, replacing these blocks: {:?}",
							merge_info.added,
							merge_info.removed
						);

						let _ = trim_to_length(&mut chainblocks.headers, T::BLOCK_BUFFER_SIZE);

						*headers = chainblocks.headers;
						*witness_from = headers.back().unwrap().block_height.saturating_forward(1);

						let highest_seen = headers.back().unwrap().block_height;
						s.block_height_update.run(highest_seen);

						// if we merge after a reorg, and the blocks we got are the same
						// as the ones we previously had, then `into_chain_progress` might
						// return `None`. In that case we return our current state.
						Ok(merge_info.into_chain_progress().unwrap_or(ChainProgress::None))
					},
					Err(MergeFailure::ReorgWithUnknownRoot {
						new_block,
						existing_wrong_parent,
					}) => {
						log::info!("detected a reorg: got block {new_block:?} whose parent hash does not match the parent block we have recorded: {existing_wrong_parent:?}");
						*witness_from = headers.front().unwrap().block_height;
						Ok(ChainProgress::None)
					},

					Err(MergeFailure::InternalError(reason)) => {
						log::error!("internal error in block height tracker: {reason}");
						Err("internal error in block height tracker")
					},
				}
			},
		}
	}
}

#[cfg(test)]
pub mod tests {

	use crate::{
		electoral_systems::{
			block_height_tracking::BlockHeightChangeHook,
			block_witnesser::state_machine::HookTypeFor,
			state_machine::core::{hook_test_utils::MockHook, Serde},
		},
		prop_do,
	};
	use cf_chains::{
		self,
		witness_period::{BlockWitnessRange, BlockZero, SaturatingStep},
		ChainWitnessConfig,
	};
	use proptest::{
		prelude::{any, prop, Arbitrary, Just, Strategy},
		prop_oneof,
		sample::select,
	};

	use crate::electoral_systems::block_height_tracking::state_machine::BHWStateWrapper;

	use super::{
		super::{
			super::state_machine::{state_machine::Statemachine, state_machine_es::SMInput},
			primitives::Header,
			HWTypes, HeightWitnesserProperties,
		},
		BHWState, BlockHeightTrackingSM, InputHeaders,
	};

	pub fn generate_input<T: HWTypes>(
		properties: HeightWitnesserProperties<T>,
	) -> impl Strategy<Value = InputHeaders<T>>
	where
		T::ChainBlockHash: Arbitrary,
		T::ChainBlockNumber: Arbitrary + BlockZero,
	{
		prop_do! {
			let header_data in prop::collection::vec(any::<T::ChainBlockHash>(), 2..10);
			let random_index in any::<T::ChainBlockNumber>();
			let first_height = if properties.witness_from_index.is_zero() { random_index } else { properties.witness_from_index };
			return {
				let headers =
					header_data.iter().zip(header_data.iter().skip(1)).enumerate().map(|(ix, (h0, h1))| Header {
						block_height: first_height.saturating_forward(ix),
						hash: h1.clone(),
						parent_hash: h0.clone(),
					});
				InputHeaders::<T>(headers.collect())
			}
		}
	}

	pub fn generate_state<T: HWTypes>() -> impl Strategy<Value = BHWStateWrapper<T>>
	where
		T::ChainBlockHash: Arbitrary,
		T::ChainBlockNumber: Arbitrary + BlockZero,
		T::BlockHeightChangeHook: Default + sp_std::fmt::Debug,
	{
		prop_oneof![
			Just(BHWState::Starting),
			prop_do! {
				let is_reorg_without_known_root in any::<bool>();
				let n in any::<HeightWitnesserProperties<T>>();
				let headers in generate_input::<T>(n);
				return {
					let witness_from = if is_reorg_without_known_root {
						headers.0.front().unwrap().block_height
					} else {
						headers.0.back().unwrap().block_height.saturating_forward(1)
					};
					BHWState::Running { headers: headers.0, witness_from }
				}
			}
		]
		.prop_map(|state| BHWStateWrapper { state, block_height_update: Default::default() })
	}

	impl<
			N: Serde
				+ Copy
				+ Ord
				+ SaturatingStep
				+ BlockZero
				+ sp_std::fmt::Debug
				+ Default
				+ 'static,
		> HWTypes for N
	{
		const BLOCK_BUFFER_SIZE: usize = 6;
		type ChainBlockNumber = N;
		type ChainBlockHash = Vec<char>;
		type BlockHeightChangeHook = MockHook<HookTypeFor<Self, BlockHeightChangeHook>>;
	}

	#[test]
	pub fn test_dsm() {
		BlockHeightTrackingSM::<u32>::test(module_path!(), generate_state(), Just(()), |indices| {
			prop_oneof![
				Just(SMInput::Context(())),
				prop_do! {
					let index in select(indices);
					let input in generate_input(index);
					return SMInput::Consensus((index, input))
				}
			]
			.boxed()
		});
	}

	struct TestChain {}
	impl ChainWitnessConfig for TestChain {
		const WITNESS_PERIOD: Self::ChainBlockNumber = 1;
		type ChainBlockNumber = u32;
	}

	#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
	struct TestTypes2 {}
	impl HWTypes for TestTypes2 {
		const BLOCK_BUFFER_SIZE: usize = 6;
		type ChainBlockNumber = BlockWitnessRange<TestChain>;
		type ChainBlockHash = bool;
		type BlockHeightChangeHook = MockHook<HookTypeFor<Self, BlockHeightChangeHook>>;
	}

	#[test]
	pub fn test_dsm2() {
		BlockHeightTrackingSM::<TestTypes2>::test(
			module_path!(),
			generate_state(),
			Just(()),
			|indices| {
				prop_do! {
					let index in select(indices);
					let input in generate_input(index);
					return SMInput::Consensus((index, input))
				}
				.boxed()
			},
		);
	}
}
