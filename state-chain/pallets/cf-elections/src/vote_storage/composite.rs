macro_rules! generate_vote_storage_tuple_impls {
    ($module:ident: ($($t:ident),*$(,)?)) => {
        pub(crate) mod $module {
            #[allow(unused_imports)]
            use crate::{CorruptStorageError, SharedDataHash};

            use super::super::{private, VoteStorage, AuthorityVote, VoteComponents};

            use codec::{Encode, Decode};
            use scale_info::TypeInfo;

            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum CompositeVoteStorageEnum<$($t,)*> {
                $($t($t),)*
            }

            // In the 1/identity case, no invalid combinations are possible, so error cases are unreachable.
            #[allow(unreachable_patterns)]
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            impl<$($t: VoteStorage),*> VoteStorage for ($($t,)*) {
                type Properties = CompositeVoteStorageEnum<$(<$t as VoteStorage>::Properties,)*>;
                type Vote = CompositeVoteStorageEnum<$(<$t as VoteStorage>::Vote,)*>;
                type PartialVote = CompositeVoteStorageEnum<$(<$t as VoteStorage>::PartialVote,)*>;
                type IndividualComponent = CompositeVoteStorageEnum<$(<$t as VoteStorage>::IndividualComponent,)*>;
                type BitmapComponent = CompositeVoteStorageEnum<$(<$t as VoteStorage>::BitmapComponent,)*>;
                type SharedData = CompositeVoteStorageEnum<$(<$t as VoteStorage>::SharedData,)*>;

                fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(
                    vote: &Self::Vote,
                    mut h: H,
                ) -> Self::PartialVote {
                    match vote {
                        $(
                            CompositeVoteStorageEnum::$t(vote) => CompositeVoteStorageEnum::$t(<$t as VoteStorage>::vote_into_partial_vote(vote, |shared_data| {
                                h(CompositeVoteStorageEnum::$t(shared_data))
                            })),
                        )*
                    }
                }

                fn partial_vote_into_components(
                    properties: Self::Properties,
                    partial_vote: Self::PartialVote,
                ) -> Result<VoteComponents<Self>, CorruptStorageError> {
                    match (properties, partial_vote) {
                        $(
                            (
                                CompositeVoteStorageEnum::$t(properties),
                                CompositeVoteStorageEnum::$t(partial_vote),
                            ) => {
                                let vote_components = <$t as VoteStorage>::partial_vote_into_components(properties, partial_vote)?;

                                Ok(VoteComponents {
                                    individual_component: vote_components.individual_component.map(|(properties, individual_component)| (CompositeVoteStorageEnum::$t(properties), CompositeVoteStorageEnum::$t(individual_component))),
                                    bitmap_component: vote_components.bitmap_component.map(CompositeVoteStorageEnum::$t),
                                })
                            },
                        )*
                        _ => Err(CorruptStorageError),
                    }
                }

                fn components_into_authority_vote<GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>>(
                    vote_components: VoteComponents<Self>,
                    mut get_shared_data: GetSharedData,
                ) -> Result<Option<(Self::Properties, AuthorityVote<Self::PartialVote, Self::Vote>)>, CorruptStorageError> {
                    match vote_components {
                        $(
                            VoteComponents {
                                individual_component: Some((CompositeVoteStorageEnum::$t(properties), CompositeVoteStorageEnum::$t(individual_component))),
                                bitmap_component: Some(CompositeVoteStorageEnum::$t(bitmap_component)),
                            } => {
                                Ok(<$t as VoteStorage>::components_into_authority_vote(
                                    VoteComponents {
                                        individual_component: Some((properties, individual_component)),
                                        bitmap_component: Some(bitmap_component)
                                    },
                                    |shared_data_hash| {
                                        match get_shared_data(shared_data_hash)? {
                                            Some(CompositeVoteStorageEnum::$t(shared_data)) => Ok(Some(shared_data)),
                                            None => Ok(None),
                                            _ => Err(CorruptStorageError),
                                        }
                                    },
                                )?.map(|(properties, authority_vote)| {
                                    (
                                        CompositeVoteStorageEnum::$t(properties),
                                        match authority_vote {
                                            AuthorityVote::PartialVote(partial_vote) => AuthorityVote::PartialVote(CompositeVoteStorageEnum::$t(partial_vote)),
                                            AuthorityVote::Vote(vote) => AuthorityVote::Vote(CompositeVoteStorageEnum::$t(vote)),
                                        },
                                    )
                                }))
                            },
                            VoteComponents {
                                individual_component: Some((CompositeVoteStorageEnum::$t(properties), CompositeVoteStorageEnum::$t(individual_component))),
                                bitmap_component: None,
                            } => {
                                Ok(<$t as VoteStorage>::components_into_authority_vote(
                                    VoteComponents {
                                        individual_component: Some((properties, individual_component)),
                                        bitmap_component: None
                                    },
                                    |shared_data_hash| {
                                        match get_shared_data(shared_data_hash)? {
                                            Some(CompositeVoteStorageEnum::$t(shared_data)) => Ok(Some(shared_data)),
                                            None => Ok(None),
                                            _ => Err(CorruptStorageError),
                                        }
                                    },
                                )?.map(|(properties, authority_vote)| {
                                    (
                                        CompositeVoteStorageEnum::$t(properties),
                                        match authority_vote {
                                            AuthorityVote::PartialVote(partial_vote) => AuthorityVote::PartialVote(CompositeVoteStorageEnum::$t(partial_vote)),
                                            AuthorityVote::Vote(vote) => AuthorityVote::Vote(CompositeVoteStorageEnum::$t(vote)),
                                        },
                                    )
                                }))
                            },
                            VoteComponents {
                                individual_component: None,
                                bitmap_component: Some(CompositeVoteStorageEnum::$t(bitmap_component)),
                            } => {
                                Ok(<$t as VoteStorage>::components_into_authority_vote(
                                    VoteComponents {
                                        individual_component: None,
                                        bitmap_component: Some(bitmap_component)
                                    },
                                    |shared_data_hash| {
                                        match get_shared_data(shared_data_hash)? {
                                            Some(CompositeVoteStorageEnum::$t(shared_data)) => Ok(Some(shared_data)),
                                            None => Ok(None),
                                            _ => Err(CorruptStorageError),
                                        }
                                    },
                                )?.map(|(properties, authority_vote)| {
                                    (
                                        CompositeVoteStorageEnum::$t(properties),
                                        match authority_vote {
                                            AuthorityVote::PartialVote(partial_vote) => AuthorityVote::PartialVote(CompositeVoteStorageEnum::$t(partial_vote)),
                                            AuthorityVote::Vote(vote) => AuthorityVote::Vote(CompositeVoteStorageEnum::$t(vote)),
                                        },
                                    )
                                }))
                            },
                        )*
                        VoteComponents {
                            individual_component: None,
                            bitmap_component: None,
                        } => Ok(None),
                        _ => Err(CorruptStorageError),
                    }
                }

                fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
                    vote: Self::Vote,
                    f: F,
                ) -> Result<(), E> {
                    match vote {
                        $(CompositeVoteStorageEnum::$t(vote) => {
                            <$t as VoteStorage>::visit_shared_data_in_vote(
                                vote,
                                |shared_data| {
                                    f(CompositeVoteStorageEnum::$t(shared_data))
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
                        $(CompositeVoteStorageEnum::$t(individual_component) => {
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
                        $(CompositeVoteStorageEnum::$t(bitmap_component) => {
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
        }
    }
}

generate_vote_storage_tuple_impls!(tuple_1_impls: (A));
generate_vote_storage_tuple_impls!(tuple_2_impls: (A, B));
generate_vote_storage_tuple_impls!(tuple_3_impls: (A, B, C));
generate_vote_storage_tuple_impls!(tuple_4_impls: (A, B, C, D));
