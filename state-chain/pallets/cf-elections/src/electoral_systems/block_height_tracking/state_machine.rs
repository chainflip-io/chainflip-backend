use super::{
	super::state_machine::{
		core::Validate, state_machine::Statemachine, state_machine_es::SMInput,
	},
	primitives::{
		trim_to_length, Header, MergeFailure, NonemptyContinuousHeaders, VoteValidationError,
	},
	ChainProgress, ChainProgressFor, HWTypes, HeightWitnesserProperties,
};
use crate::electoral_systems::state_machine::{
	core::{defx, Hook},
	state_machine::AbstractApi,
};
use cf_chains::witness_period::{BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use frame_support::pallet_prelude::MaxEncodedLen;
use itertools::Either;
use scale_info::{prelude::format, TypeInfo};
use serde::{Deserialize, Serialize};
use sp_std::{collections::vec_deque::VecDeque, fmt::Debug, vec::Vec};

//------------------------ state ---------------------------

defx! {

	pub enum BHWState[T: HWTypes] {
		Starting,
		Running { headers: NonemptyContinuousHeaders<T>, witness_from: T::ChainBlockNumber },
	}

	validate this (else BHWStateError) {
		is_valid: match this {
			BHWState::Starting => true,
			BHWState::Running { headers, witness_from: _ } => headers.is_valid().is_ok()
		}
	}

	impl Default {
		fn default() -> Self {
			Self::Starting
		}
	}
}


defx! {

	#[derive(Default)]
	pub struct BlockHeightWitnesser[T: HWTypes] {
		pub state: BHWState<T>,
		pub block_height_update: T::BlockHeightChangeHook,
	}

	validate _this (else BlockHeightWitnesserError) {}

	impl AbstractApi {
		type Query = HeightWitnesserProperties<T>;
		type Response = NonemptyContinuousHeaders<T>;
		type Error = VoteValidationError<T>;

		fn validate(
			base: &HeightWitnesserProperties<T>,
			this: &NonemptyContinuousHeaders<T>,
		) -> Result<(), Self::Error> {
			this.is_valid().map_err(VoteValidationError::NonemptyContinuousHeadersError)?;

			if base.witness_from_index.is_zero() {
				Ok(())
			} else {
				if this.first().block_height == base.witness_from_index {
					Ok(())
				} else {
					Err(VoteValidationError::BlockNotMatchingRequestedHeight)
				}
			}
		}
	}

	impl Statemachine {
		type State = BlockHeightWitnesser<T>;
		type Context = ();
		type Settings = ();
		type Output = Result<ChainProgressFor<T>, &'static str>;

		fn input_index(s: &mut Self::State) -> Vec<Self::Query> {
			let witness_from_index = match s.state {
				BHWState::Starting => T::ChainBlockNumber::zero(),
				BHWState::Running { headers: _, witness_from } => witness_from,
			};
			Vec::from([HeightWitnesserProperties { witness_from_index }])
		}

		/*

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

		*/

		fn step(
			s: &mut Self::State,
			input: Either<Self::Context, (Self::Query, Self::Response)>,
			_settings: &(),
		) -> Self::Output {
			let new_headers = match input {
				Either::Left(_) => return Ok(ChainProgress::None),
				Either::Right((_properties, consensus)) => consensus,
			};

			match &mut s.state {
				BHWState::Starting => {
					let first = new_headers.headers.front().unwrap().block_height;
					let last = new_headers.headers.back().unwrap().block_height;
					s.state = BHWState::Running {
						headers: new_headers.clone(),
						witness_from: last.saturating_forward(1),
					};
					let hashes = new_headers
						.headers
						.into_iter()
						.map(|hash| (hash.block_height, hash.hash))
						.collect();
					Ok(ChainProgress::Range(hashes, first..=last))
				},

				BHWState::Running { headers, witness_from } => {
					let mut chainblocks = headers.clone();

					match chainblocks.merge(new_headers) {
						Ok(merge_info) => {
							log::info!(
								"added new blocks: {:?}, replacing these blocks: {:?}",
								merge_info.added,
								merge_info.removed
							);

							let _ = trim_to_length(&mut chainblocks.headers, T::BLOCK_BUFFER_SIZE);

							*headers = chainblocks;
							*witness_from = headers.headers.back().unwrap().block_height.saturating_forward(1);

							let highest_seen = headers.headers.back().unwrap().block_height;
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
							*witness_from = headers.headers.front().unwrap().block_height;
							Ok(ChainProgress::None)
						},

						Err(MergeFailure::InternalError(reason)) => {
							let str = format!("internal error in block height tracker: {reason}");
							log::error!("internal error in block height tracker: {reason}");
							Err(str.leak())
						},
					}
				},
			}
		}
	}

}

//------------------------ state machine ---------------------------

#[cfg(test)]
pub mod tests {
	use crate::{
		electoral_systems::{
			block_height_tracking::{
				primitives::NonemptyContinuousHeaders, BlockHeightChangeHook, ChainTypes,
			},
			block_witnesser::state_machine::HookTypeFor,
			state_machine::core::{hook_test_utils::MockHook, Serde, TypesFor, Validate},
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
	use sp_std::{fmt::Debug, iter::Step};

	use super::{
		super::{
			super::state_machine::{state_machine::Statemachine, state_machine_es::SMInput},
			primitives::Header,
			HWTypes, HeightWitnesserProperties,
		},
		BHWState, BlockHeightWitnesser,
	};

	pub fn generate_input<T: HWTypes>(
		properties: HeightWitnesserProperties<T>,
	) -> impl Strategy<Value = NonemptyContinuousHeaders<T>>
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
				NonemptyContinuousHeaders::<T>{ headers: headers.collect() }
			}
		}
	}

	pub fn generate_state<T: HWTypes>() -> impl Strategy<Value = BlockHeightWitnesser<T>>
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
						headers.headers.front().unwrap().block_height
					} else {
						headers.headers.back().unwrap().block_height.saturating_forward(1)
					};
					BHWState::Running { headers, witness_from }
				}
			}
		]
		.prop_map(|state| BlockHeightWitnesser { state, block_height_update: Default::default() })
	}

	impl<
			N: Validate
				+ Serde
				+ Copy
				+ Ord
				+ SaturatingStep
				+ Step
				+ BlockZero
				+ Debug
				+ Default
				+ 'static,
			H: Validate + Serde + Ord + Clone + Debug + 'static,
			D: Validate + Serde + Ord + Clone + Debug + 'static,
		> ChainTypes for TypesFor<(N, H, D)>
	{
		type ChainBlockNumber = N;
		type ChainBlockHash = H;

		// TODO we could make this a parameter to test with different margins
		const SAFETY_MARGIN: u32 = 16;
	}

	impl<
			N: Validate
				+ Serde
				+ Copy
				+ Ord
				+ SaturatingStep
				+ Step
				+ BlockZero
				+ sp_std::fmt::Debug
				+ Default
				+ 'static,
			H: Validate + Serde + Ord + Clone + Debug + 'static,
			D: Validate + Serde + Ord + Clone + Debug + 'static,
		> HWTypes for TypesFor<(N, H, D)>
	{
		const BLOCK_BUFFER_SIZE: usize = 16;
		type BlockHeightChangeHook = MockHook<HookTypeFor<Self, BlockHeightChangeHook>>;
	}

	#[test]
	pub fn test_dsm() {
		BlockHeightWitnesser::<TypesFor<(u32, Vec<char>, ())>>::test(
			module_path!(),
			generate_state(),
			Just(()),
			|index| {
				prop_do! {
					let input in generate_input(index);
					return input
				}
				.boxed()
			},
			Just(()),
		);
	}

	struct TestChain {}
	impl ChainWitnessConfig for TestChain {
		const WITNESS_PERIOD: Self::ChainBlockNumber = 1;
		type ChainBlockNumber = u32;
	}

	impl ChainTypes for TestTypes2 {
		type ChainBlockNumber = BlockWitnessRange<TestChain>;
		type ChainBlockHash = bool;

		const SAFETY_MARGIN: u32 = 16;
	}

	#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
	struct TestTypes2 {}
	impl HWTypes for TestTypes2 {
		const BLOCK_BUFFER_SIZE: usize = 16;
		type BlockHeightChangeHook = MockHook<HookTypeFor<Self, BlockHeightChangeHook>>;
	}

	#[test]
	pub fn test_dsm2() {
		BlockHeightWitnesser::<TestTypes2>::test(
			module_path!(),
			generate_state(),
			Just(()),
			|index| {
				prop_do! {
					let input in generate_input(index);
					return input
				}
				.boxed()
			},
			Just(()),
		);
	}
}
