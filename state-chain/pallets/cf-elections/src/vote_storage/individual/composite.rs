macro_rules! generate_individual_vote_storage_tuple_impls {
    ($module:ident: ($($t:ident),*$(,)?)) => {
        mod $module {
            #[allow(unused_imports)]
            use crate::{
                vote_storage::{
                    AuthorityVote,
                },
                CorruptStorageError,
                SharedDataHash,
            };

            use super::super::{private, IndividualVoteStorage};

            use codec::{Encode, Decode};
            use scale_info::TypeInfo;

            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum SharedDataEnum<$($t,)*> {
                $($t($t),)*
            }

            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            #[allow(unused_mut)]
            impl<$($t: IndividualVoteStorage),*> IndividualVoteStorage for ($($t,)*) {
                type Vote = ($(<$t as IndividualVoteStorage>::Vote,)*);
                type PartialVote = ($(<$t as IndividualVoteStorage>::PartialVote,)*);
                type SharedData = SharedDataEnum<$(<$t as IndividualVoteStorage>::SharedData,)*>;

                #[allow(clippy::unused_unit)]
                fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(($($t,)*): &Self::Vote, mut h: H) -> Self::PartialVote {
                    ($(
                        <$t as IndividualVoteStorage>::vote_into_partial_vote($t, |shared_data| {
                            (&mut h)(SharedDataEnum::$t(shared_data))
                        })
                    ,)*)
                }

                #[allow(unused_mut)]
                fn partial_vote_into_vote<GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>>(($($t,)*): &Self::PartialVote, mut get_shared_data: GetSharedData) -> Result<Option<Self::Vote>, CorruptStorageError> {
                    Ok(Some(($(
                        if let Some(vote) = <$t as IndividualVoteStorage>::partial_vote_into_vote(
                            $t,
                            |shared_data_hash| {
                                Ok(match get_shared_data(shared_data_hash)? {
                                    Some(SharedDataEnum::$t(shared_data)) => Some(shared_data),
                                    _ => None,
                                })
                            }
                        )? {
                            vote
                        } else {
                            return Ok(None)
                        },
                    )*)))
                }

                fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(($($t,)*): Self::Vote, f: F) -> Result<(), E> {
                    $(
                        <$t as IndividualVoteStorage>::visit_shared_data_in_vote($t, |shared_data| {
                            f(SharedDataEnum::$t(shared_data))
                        })?;
                    )*
                    Ok(())
                }
                fn visit_shared_data_references_in_partial_vote<F: Fn(SharedDataHash)>(($($t,)*): &Self::PartialVote, f: F) {
                    $(
                        <$t as IndividualVoteStorage>::visit_shared_data_references_in_partial_vote($t, &f);
                    )*
                }
            }
            impl<$($t: IndividualVoteStorage),*> private::Sealed for ($($t,)*) {}
        }
    }
}

generate_individual_vote_storage_tuple_impls!(tuple_0_impls: ());
generate_individual_vote_storage_tuple_impls!(tuple_1_impls: (A));
generate_individual_vote_storage_tuple_impls!(tuple_2_impls: (A, B));
generate_individual_vote_storage_tuple_impls!(tuple_3_impls: (A, B, C));
generate_individual_vote_storage_tuple_impls!(tuple_4_impls: (A, B, C, D));
generate_individual_vote_storage_tuple_impls!(tuple_5_impls: (A, B, C, D, EE));

use crate::vote_storage::composite::tuple_3_impls::CompositeVoteStorageEnum;
use cf_chains::benchmarking_value::BenchmarkValue;
impl BenchmarkValue for CompositeVoteStorageEnum<u64, u64, ()> {
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self {
		CompositeVoteStorageEnum::A(1_000u64)
	}
}
use crate::electoral_systems::blockchain::delta_based_ingress::{
	ChannelTotalIngressed, MAXIMUM_CHANNELS_PER_ELECTION,
};
use cf_chains::{sol::SolAddress, Solana};
use frame_support::BoundedBTreeMap;
use sp_core::ConstU32;
impl BenchmarkValue
	for CompositeVoteStorageEnum<
		u64,
		u64,
		BoundedBTreeMap<
			SolAddress,
			ChannelTotalIngressed<Solana>,
			ConstU32<MAXIMUM_CHANNELS_PER_ELECTION>,
		>,
	>
{
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self {
		CompositeVoteStorageEnum::A(1_000u64)
	}
}
