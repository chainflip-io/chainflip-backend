/// Implements `VoteStorage` for tuples of `VoteStorage` types.
///
/// Requires a generic list of tuple identifiers. The first should be named `A` otherwise the impl
/// for BenchmarkValues doesn't work. For example (A,) or (A, B, C) both work, but not (First,
/// Second, Third).
macro_rules! generate_vote_storage_tuple_impls {
    ($module:ident: ($($t:ident),* $(,)?)) => {
        pub mod $module {
            #[allow(unused_imports)]
            use crate::{CorruptStorageError, SharedDataHash};

            use super::super::{private, VoteStorage, AuthorityVote, VoteComponents};

            use codec::{Encode, Decode};
            use scale_info::TypeInfo;

            #[cfg(feature = "runtime-benchmarks")]
            use cf_chains::benchmarking_value::BenchmarkValue;


            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum CompositeVoteProperties<$($t,)*> {
                $($t($t),)*
            }
            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum CompositeVote<$($t,)*> {
                $($t($t),)*
            }
            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum CompositePartialVote<$($t,)*> {
                $($t($t),)*
            }
            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum CompositeIndividualComponent<$($t,)*> {
                $($t($t),)*
            }
            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum CompositeBitmapComponent<$($t,)*> {
                $($t($t),)*
            }
            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum CompositeSharedData<$($t,)*> {
                $($t($t),)*
            }

            // In the 1/identity case, no invalid combinations are possible, so error cases are unreachable.

            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            impl<$($t: VoteStorage),*> VoteStorage for ($($t,)*) {
                type Properties = CompositeVoteProperties<$(<$t as VoteStorage>::Properties,)*>;
                type Vote = CompositeVote<$(<$t as VoteStorage>::Vote,)*>;
                type PartialVote = CompositePartialVote<$(<$t as VoteStorage>::PartialVote,)*>;
                type IndividualComponent = CompositeIndividualComponent<$(<$t as VoteStorage>::IndividualComponent,)*>;
                type BitmapComponent = CompositeBitmapComponent<$(<$t as VoteStorage>::BitmapComponent,)*>;
                type SharedData = CompositeSharedData<$(<$t as VoteStorage>::SharedData,)*>;

                fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(
                    vote: &Self::Vote,
                    mut h: H,
                ) -> Self::PartialVote {
                    match vote {
                        $(
                            CompositeVote::$t(vote) => CompositePartialVote::$t(<$t as VoteStorage>::vote_into_partial_vote(vote, |shared_data| {
                                h(CompositeSharedData::$t(shared_data))
                            })),
                        )*
                    }
                }

                fn partial_vote_into_components(
                    properties: Self::Properties,
                    partial_vote: Self::PartialVote,
                ) -> Result<VoteComponents<Self>, CorruptStorageError> {
                    #[allow(unreachable_patterns)]
                    match (properties, partial_vote) {
                        $(
                            (
                                CompositeVoteProperties::$t(properties),
                                CompositePartialVote::$t(partial_vote),
                            ) => {
                                let vote_components = <$t as VoteStorage>::partial_vote_into_components(properties, partial_vote)?;

                                Ok(VoteComponents {
                                    individual_component: vote_components.individual_component.map(|(properties, individual_component)| (CompositeVoteProperties::$t(properties), CompositeIndividualComponent::$t(individual_component))),
                                    bitmap_component: vote_components.bitmap_component.map(CompositeBitmapComponent::$t),
                                })
                            },
                        )*
                        _ => Err(CorruptStorageError::new()),
                    }
                }

                fn components_into_authority_vote<GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>>(
                    vote_components: VoteComponents<Self>,
                    mut get_shared_data: GetSharedData,
                ) -> Result<Option<(Self::Properties, AuthorityVote<Self::PartialVote, Self::Vote>)>, CorruptStorageError> {
                    #[allow(unreachable_patterns)]
                    match vote_components {
                        $(
                            VoteComponents {
                                individual_component: Some((CompositeVoteProperties::$t(properties), CompositeIndividualComponent::$t(individual_component))),
                                bitmap_component: Some(CompositeBitmapComponent::$t(bitmap_component)),
                            } => {
                                Ok(<$t as VoteStorage>::components_into_authority_vote(
                                    VoteComponents {
                                        individual_component: Some((properties, individual_component)),
                                        bitmap_component: Some(bitmap_component)
                                    },
                                    |shared_data_hash| {
                                        #[allow(unreachable_patterns)]
                                        match get_shared_data(shared_data_hash)? {
                                            Some(CompositeSharedData::$t(shared_data)) => Ok(Some(shared_data)),
                                            None => Ok(None),
                                            _ => Err(CorruptStorageError::new())
                                        }
                                    },
                                )?.map(|(properties, authority_vote)| {
                                    (
                                        CompositeVoteProperties::$t(properties),
                                        match authority_vote {
                                            AuthorityVote::PartialVote(partial_vote) => AuthorityVote::PartialVote(CompositePartialVote::$t(partial_vote)),
                                            AuthorityVote::Vote(vote) => AuthorityVote::Vote(CompositeVote::$t(vote)),
                                        },
                                    )
                                }))
                            },
                            VoteComponents {
                                individual_component: Some((CompositeVoteProperties::$t(properties), CompositeIndividualComponent::$t(individual_component))),
                                bitmap_component: None,
                            } => {
                                Ok(<$t as VoteStorage>::components_into_authority_vote(
                                    VoteComponents {
                                        individual_component: Some((properties, individual_component)),
                                        bitmap_component: None
                                    },
                                    |shared_data_hash| {
                                        match get_shared_data(shared_data_hash)? {
                                            Some(CompositeSharedData::$t(shared_data)) => Ok(Some(shared_data)),
                                            None => Ok(None),
                                            _ => Err(CorruptStorageError::new()),
                                        }
                                    },
                                )?.map(|(properties, authority_vote)| {
                                    (
                                        CompositeVoteProperties::$t(properties),
                                        match authority_vote {
                                            AuthorityVote::PartialVote(partial_vote) => AuthorityVote::PartialVote(CompositePartialVote::$t(partial_vote)),
                                            AuthorityVote::Vote(vote) => AuthorityVote::Vote(CompositeVote::$t(vote)),
                                        },
                                    )
                                }))
                            },
                            VoteComponents {
                                individual_component: None,
                                bitmap_component: Some(CompositeBitmapComponent::$t(bitmap_component)),
                            } => {
                                Ok(<$t as VoteStorage>::components_into_authority_vote(
                                    VoteComponents {
                                        individual_component: None,
                                        bitmap_component: Some(bitmap_component)
                                    },
                                    |shared_data_hash| {
                                        match get_shared_data(shared_data_hash)? {
                                            Some(CompositeSharedData::$t(shared_data)) => Ok(Some(shared_data)),
                                            None => Ok(None),
                                            _ => Err(CorruptStorageError::new()),
                                        }
                                    },
                                )?.map(|(properties, authority_vote)| {
                                    (
                                        CompositeVoteProperties::$t(properties),
                                        match authority_vote {
                                            AuthorityVote::PartialVote(partial_vote) => AuthorityVote::PartialVote(CompositePartialVote::$t(partial_vote)),
                                            AuthorityVote::Vote(vote) => AuthorityVote::Vote(CompositeVote::$t(vote)),
                                        },
                                    )
                                }))
                            },
                        )*
                        VoteComponents {
                            individual_component: None,
                            bitmap_component: None,
                        } => Ok(None),
                        _ => Err(CorruptStorageError::new()),
                    }
                }

                fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
                    vote: Self::Vote,
                    f: F,
                ) -> Result<(), E> {
                    match vote {
                        $(CompositeVote::$t(vote) => {
                            <$t as VoteStorage>::visit_shared_data_in_vote(
                                vote,
                                |shared_data| {
                                    f(CompositeSharedData::$t(shared_data))
                                }
                            )
                        })*
                    }
                }

                fn visit_shared_data_references_in_individual_component<F: Fn(SharedDataHash)>(
                    individual_component: &Self::IndividualComponent,
                    f: F,
                ) {
                    match individual_component {
                        $(CompositeIndividualComponent::$t(individual_component) => {
                            <$t as VoteStorage>::visit_shared_data_references_in_individual_component(
                                individual_component,
                                |shared_data_hash| {
                                    f(shared_data_hash)
                                }
                            )
                        },)*
                    }
                }

                fn visit_shared_data_references_in_bitmap_component<F: Fn(SharedDataHash)>(
                    bitmap_component: &Self::BitmapComponent,
                    f: F,
                ) {
                    match bitmap_component {
                        $(CompositeBitmapComponent::$t(bitmap_component) => {
                            <$t as VoteStorage>::visit_shared_data_references_in_bitmap_component(
                                bitmap_component,
                                |shared_data_hash| {
                                    f(shared_data_hash)
                                }
                            )
                        },)*
                    }
                }

            }
            impl<$($t: VoteStorage),*> private::Sealed for ($($t,)*) {}

            #[cfg(feature = "runtime-benchmarks")]
            impl<$($t),*> BenchmarkValue for CompositeVote<$($t),*>
            where
                A: BenchmarkValue,
            {
                fn benchmark_value() -> Self {
                    CompositeVote::A(A::benchmark_value())
                }
            }
            #[cfg(feature = "runtime-benchmarks")]
            impl<$($t),*> BenchmarkValue for CompositeSharedData<$($t),*>
            where
                A: BenchmarkValue,
            {
                fn benchmark_value() -> Self {
                    CompositeSharedData::A(A::benchmark_value())
                }
            }
            #[cfg(feature = "runtime-benchmarks")]
            impl<$($t),*> BenchmarkValue for CompositeIndividualComponent<$($t),*>
            where
                A: BenchmarkValue,
            {
                fn benchmark_value() -> Self {
                    CompositeIndividualComponent::A(A::benchmark_value())
                }
            }
            #[cfg(feature = "runtime-benchmarks")]
            impl<$($t),*> BenchmarkValue for CompositeVoteProperties<$($t),*>
            where
                A: BenchmarkValue,
            {
                fn benchmark_value() -> Self {
                    CompositeVoteProperties::A(A::benchmark_value())
                }
            }
        }
    }
}

generate_vote_storage_tuple_impls!(tuple_6_impls: (A, B, C, D, EE, FF));
