use super::{
	super::state_machine::state_machine::Statemachine,
	primitives::{MergeFailure, NonemptyContinuousHeaders, NonemptyContinuousHeadersError},
	BHWTypes, ChainBlockNumberOf, ChainProgress, ChainTypes, HeightWitnesserProperties,
};
use crate::electoral_systems::{
	block_height_witnesser::{primitives::ContinuousHeaders, BlockHeightWitnesserSettings},
	state_machine::{core::defx, state_machine::AbstractApi},
};
use cf_chains::witness_period::SaturatingStep;
use cf_traits::{Hook, Validate};
use codec::{Decode, Encode};
use generic_typeinfo_derive::GenericTypeInfo;
use itertools::Either;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{fmt::Debug, vec::Vec};

defx! {
	/// Phase that the Block Height Witnesser is currently in.
	///
	/// When started for the first time, the BHW has a special `Starting` phase,
	/// in which it queries the engines to vote for an arbitrary continous chain of
	/// headers.
	///
	/// When it's already running it keeps a record of the previous chain of headers,
	/// and the next block it's going to witness from.
	#[derive(GenericTypeInfo)]
	#[expand_name_with(T::Chain::NAME)]
	pub enum BHWPhase[T: BHWTypes] {
		Starting,
		Running { headers: NonemptyContinuousHeaders<T::Chain>, witness_from: ChainBlockNumberOf<T::Chain> },
	}
	validate this (else BHWStateError) {
		// TODO since this only recursively checks that the contents are valid, it is possible to
		// derive this check in the defx macro, just as it is derived for structs.
		is_valid: match this {
			BHWPhase::Starting => true,
			BHWPhase::Running { headers, witness_from: _ } => headers.is_valid().is_ok()
		}
	}
	impl Default {
		fn default() -> Self {
			Self::Starting
		}
	}
}

defx! {
	/// The main Block Height Witnesser (BHW) type.
	///
	/// It contains the state of the BHW state machine, and it's also the type
	/// this state machine is associated to.
	#[derive(Default)]
	#[derive(GenericTypeInfo)]
	#[expand_name_with(T::Chain::NAME)]
	pub struct BlockHeightWitnesser[T: BHWTypes] {
		pub phase: BHWPhase<T>,
		pub block_height_update: T::BlockHeightChangeHook,
		pub on_reorg: T::ReorgHook,
	}
	validate _this (else BlockHeightWitnesserError) {}
}
impl<T: BHWTypes> AbstractApi for BlockHeightWitnesser<T> {
	type Query = HeightWitnesserProperties<T::Chain>;
	type Response = NonemptyContinuousHeaders<T::Chain>;
	type Error = VoteValidationError<T::Chain>;
	fn validate(
		query: &HeightWitnesserProperties<T::Chain>,
		response: &NonemptyContinuousHeaders<T::Chain>,
	) -> Result<(), Self::Error> {
		response
			.is_valid()
			.map_err(VoteValidationError::NonemptyContinuousHeadersError)?;
		// We always accept the first vote, when the electoral system is started.
		// See the `step` function for the block height witnessing.
		if query.witness_from_index == Default::default() ||
			response.first().block_height == query.witness_from_index
		{
			Ok(())
		} else {
			Err(VoteValidationError::BlockNotMatchingRequestedHeight)
		}
	}
}
impl<T: BHWTypes> Statemachine for BlockHeightWitnesser<T> {
	type State = BlockHeightWitnesser<T>;
	type Context = ();
	type Settings = BlockHeightWitnesserSettings;
	type Output = Result<Option<ChainProgress<T::Chain>>, &'static str>;
	fn get_queries(s: &mut Self::State) -> Vec<Self::Query> {
		let witness_from_index = match s.phase {
			BHWPhase::Starting => ChainBlockNumberOf::<T::Chain>::default(),
			BHWPhase::Running { headers: _, witness_from } => witness_from,
		};
		Vec::from([HeightWitnesserProperties { witness_from_index }])
	}
	#[cfg(test)]
	fn step_specification(
		before: &mut Self::State,
		input: &crate::electoral_systems::state_machine::state_machine::InputOf<Self>,
		_output: &Self::Output,
		_settings: &Self::Settings,
		after: &Self::State,
	) {
		use cf_chains::witness_period::SaturatingStep;
		use BHWPhase::*;
		match (&before.phase, &after.phase) {
			(Starting, Starting) => assert!(
				*input == Either::Left(()),
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
					(*from1 == headers0.first().block_height)
					||
					// - in the normal case, the `witness_from` should always be the next
					//   height after the last header that we have
					(*from1 == headers1.last().block_height.saturating_forward(1)),
					"witness_from should be either next height, or height of first header"
				);

				assert!(
					// if the input is *not* the empty context, then `witness_from` should
					// always change after running the transition function.
					// This ensures that we always have "fresh" election properties,
					// and are thus deleting/recreating elections as expected.
					(*input == Either::Left(())) || (*from1 != *from0),
					"witness_from should always change, except when we get a non-vote input"
				);
			},
		}
	}
	fn step(
		s: &mut Self::State,
		input: Either<Self::Context, (Self::Query, Self::Response)>,
		settings: &BlockHeightWitnesserSettings,
	) -> Self::Output {
		let new_headers = match input {
			Either::Left(_) => return Ok(None),
			Either::Right((_properties, consensus)) => consensus,
		};
		match &mut s.phase {
			BHWPhase::Starting => {
				s.phase = BHWPhase::Running {
					headers: new_headers.clone(),
					witness_from: new_headers.last().block_height.saturating_forward(1),
				};
				Ok(Some(ChainProgress { headers: new_headers.into(), removed: None }))
			},
			BHWPhase::Running { headers, witness_from } => match headers.merge(new_headers) {
				Ok(merge_info) => {
					log::debug!(
						"added new blocks: {:?}, replacing these blocks: {:?}",
						merge_info.added,
						merge_info.removed
					);

					headers.trim_to_length(settings.safety_buffer as usize);

					let highest_seen = headers.last().block_height;
					s.block_height_update.run(highest_seen);
					*witness_from = highest_seen.saturating_forward(1);

					match ContinuousHeaders::try_new(merge_info.added) {
						Ok(added_headers) => Ok(Some(ChainProgress {
							headers: added_headers,
							removed: merge_info.removed.front().and_then(|f| {
								merge_info.removed.back().map(|l| {
									s.on_reorg.run(f.block_height..=l.block_height);
									f.block_height..=l.block_height
								})
							}),
						})),
						Err(err) => {
							// this case should never happen. the logic of the BHW should ensure
							// that merge_info.added is always a continuous chain of headers
							log::error!("encountered `merge_info.added` which is not a continuous chain of headers! {err:?}");
							Err("encountered `merge_info.added` which is not a continuous chain of headers!")
						},
					}
				},
				Err(MergeFailure::Reorg) => {
					*witness_from = headers.first().block_height;
					Ok(None)
				},

				Err(MergeFailure::InternalError) => {
					log::error!("internal error in block height tracker with state: {:?}", s);
					Err("internal error in block height tracker")
				},
			},
		}
	}
}

/// A vote submitted to the BHW might not be valid due to these reasons.
#[derive(Debug, PartialEq)]
pub enum VoteValidationError<C: ChainTypes> {
	BlockNotMatchingRequestedHeight,
	NonemptyContinuousHeadersError(NonemptyContinuousHeadersError<C>),
}

#[cfg(test)]
pub mod tests {

	use crate::{
		electoral_systems::{
			block_height_witnesser::{
				primitives::{ContinuousHeaders, NonemptyContinuousHeaders},
				BlockHeightChangeHook, BlockHeightWitnesserSettings, ChainBlockHashOf,
				ChainBlockHashTrait, ChainBlockNumberOf, ChainBlockNumberTrait, ChainTypes,
				ReorgHook,
			},
			block_witnesser::state_machine::HookTypeFor,
			state_machine::core::TypesFor,
		},
		prop_do,
	};
	use cf_chains::{self, witness_period::BlockWitnessRange, ChainWitnessConfig};
	use cf_traits::hook_test_utils::MockHook;
	use proptest::{
		arbitrary::arbitrary_with,
		prelude::{any, prop, Arbitrary, Just, Strategy},
		prop_oneof,
	};
	use scale_info::TypeInfo;
	use serde::{Deserialize, Serialize};
	use sp_std::{fmt::Debug, vec::Vec};

	use super::{
		super::{
			super::state_machine::state_machine::Statemachine, primitives::Header, BHWTypes,
			HeightWitnesserProperties,
		},
		BHWPhase, BlockHeightWitnesser,
	};

	impl<C: ChainTypes> Arbitrary for ContinuousHeaders<C> {
		type Parameters = (ChainBlockNumberOf<C>, usize);

		fn arbitrary_with((witness_from_index, length): Self::Parameters) -> Self::Strategy {
			prop_do! {
				let header_data in prop::collection::vec(any::<ChainBlockHashOf<C>>(), 1..(length+3));
				let random_index in any::<ChainBlockNumberOf<C>>();
				let first_height = if witness_from_index == Default::default() { random_index } else { witness_from_index };
				return {
					let headers =
						header_data.iter().zip(header_data.iter().skip(1)).enumerate().map(|(ix, (h0, h1))| Header {
							block_height: first_height.saturating_forward(ix),
							hash: h1.clone(),
							parent_hash: h0.clone(),
						});
					headers.into()
				}
			}
		}

		type Strategy = impl Strategy<Value = ContinuousHeaders<C>> + Clone + Send;
	}

	impl<C: ChainTypes> Arbitrary for NonemptyContinuousHeaders<C> {
		type Parameters = (ChainBlockNumberOf<C>, usize);

		fn arbitrary_with((witness_from_index, length): Self::Parameters) -> Self::Strategy {
			prop_do! {
				let header_data in prop::collection::vec(any::<ChainBlockHashOf<C>>(), 2..(length+3));
				let random_index in any::<ChainBlockNumberOf<C>>();
				let first_height = if witness_from_index == Default::default() { random_index } else { witness_from_index };
				return {
					let headers =
						header_data.iter().zip(header_data.iter().skip(1)).enumerate().map(|(ix, (h0, h1))| Header {
							block_height: first_height.saturating_forward(ix),
							hash: h1.clone(),
							parent_hash: h0.clone(),
						});
					NonemptyContinuousHeaders::<C>::try_new(headers.collect()).unwrap()
				}
			}
		}

		type Strategy = impl Strategy<Value = NonemptyContinuousHeaders<C>> + Clone + Send;
	}

	pub fn generate_input<T: BHWTypes>(
		properties: HeightWitnesserProperties<T::Chain>,
	) -> impl Strategy<Value = NonemptyContinuousHeaders<T::Chain>> {
		arbitrary_with::<NonemptyContinuousHeaders<T::Chain>, _, _>((
			properties.witness_from_index,
			10,
		))
	}

	pub fn generate_state<T: BHWTypes>() -> impl Strategy<Value = BlockHeightWitnesser<T>>
	where
		T::BlockHeightChangeHook: Default + sp_std::fmt::Debug,
		T::ReorgHook: Default + sp_std::fmt::Debug,
	{
		prop_oneof![
			Just(BHWPhase::Starting),
			prop_do! {
				let is_reorg_without_known_root in any::<bool>();
				let n in any::<HeightWitnesserProperties<T::Chain>>();
				let headers in generate_input::<T>(n);
				return {
					let witness_from = if is_reorg_without_known_root {
						headers.first().block_height
					} else {
						headers.last().block_height.saturating_forward(1)
					};
					BHWPhase::Running { headers, witness_from }
				}
			}
		]
		.prop_map(|state| BlockHeightWitnesser {
			phase: state,
			block_height_update: Default::default(),
			on_reorg: Default::default(),
		})
	}

	impl<N: ChainBlockNumberTrait, H: ChainBlockHashTrait, D: 'static> ChainTypes
		for TypesFor<(N, H, D)>
	{
		type ChainBlockNumber = N;
		type ChainBlockHash = H;

		const NAME: &'static str = "Mock";
	}

	impl<N: ChainBlockNumberTrait, H: ChainBlockHashTrait, D: 'static> BHWTypes
		for TypesFor<(N, H, D)>
	{
		type BlockHeightChangeHook = MockHook<HookTypeFor<Self, BlockHeightChangeHook>>;
		type ReorgHook = MockHook<HookTypeFor<Self, ReorgHook>>;
		type Chain = Self;
	}

	#[test]
	pub fn test_dsm() {
		BlockHeightWitnesser::<TypesFor<(u32, Vec<u8>, ())>>::test(
			module_path!(),
			generate_state(),
			(2..20u32).prop_map(|safety_buffer| BlockHeightWitnesserSettings { safety_buffer }),
			|index| {
				prop_do! {
					let input in generate_input::<TypesFor<(u32, Vec<u8>, ())>>(index);
					return input
				}
				.boxed()
			},
			|_| Just(()).boxed(),
			|_| {},
		);
	}

	#[derive(TypeInfo)]
	struct TestChain {}
	impl ChainWitnessConfig for TestChain {
		const WITNESS_PERIOD: Self::ChainBlockNumber = 1;
		type ChainBlockNumber = u32;
	}

	impl ChainTypes for TypesFor<TestChain> {
		type ChainBlockNumber = BlockWitnessRange<TestChain>;
		type ChainBlockHash = bool;

		const NAME: &'static str = "Mock";
	}

	#[derive(
		Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Serialize, Deserialize, TypeInfo,
	)]
	struct TestTypes2 {}
	impl BHWTypes for TestTypes2 {
		type BlockHeightChangeHook = MockHook<HookTypeFor<Self, BlockHeightChangeHook>>;
		type ReorgHook = MockHook<HookTypeFor<Self, ReorgHook>>;
		type Chain = TypesFor<TestChain>;
	}

	#[test]
	pub fn test_dsm2() {
		BlockHeightWitnesser::<TestTypes2>::test(
			module_path!(),
			generate_state(),
			(2..20u32).prop_map(|safety_buffer| BlockHeightWitnesserSettings { safety_buffer }),
			|index| {
				prop_do! {
					let input in generate_input::<TestTypes2>(index);
					return input
				}
				.boxed()
			},
			|_| Just(()).boxed(),
			|_| {},
		);
	}
}
