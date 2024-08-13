macro_rules! generate_simple_vote_storage_tuple_impls {
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

            use super::super::{private, SimpleVoteStorage};

            use codec::{Encode, Decode};
            use scale_info::TypeInfo;

            #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
            pub enum SharedDataEnum<$($t,)*> {
                $($t($t),)*
            }

            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            impl<$($t: SimpleVoteStorage),*> SimpleVoteStorage for ($($t,)*) {
                type Vote = ($(<$t as SimpleVoteStorage>::Vote,)*);
                type PartialVote = ($(<$t as SimpleVoteStorage>::PartialVote,)*);
                type SharedData = SharedDataEnum<$(<$t as SimpleVoteStorage>::SharedData,)*>;

                #[allow(clippy::unused_unit)]
                fn vote_into_partial_vote<H: Fn(Self::SharedData) -> SharedDataHash>(($($t,)*): &Self::Vote, h: H) -> Self::PartialVote {
                    ($(
                        <$t as SimpleVoteStorage>::vote_into_partial_vote($t, |shared_data| {
                            (&h)(SharedDataEnum::$t(shared_data))
                        })
                    ,)*)
                }

                #[allow(unused_mut)]
                fn partial_vote_into_vote<GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>>(($($t,)*): &Self::PartialVote, mut get_shared_data: GetSharedData) -> Result<Option<Self::Vote>, CorruptStorageError> {
                    Ok(Some(($(
                        if let Some(vote) = <$t as SimpleVoteStorage>::partial_vote_into_vote(
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
                        <$t as SimpleVoteStorage>::visit_shared_data_in_vote($t, |shared_data| {
                            f(SharedDataEnum::$t(shared_data))
                        })?;
                    )*
                    Ok(())
                }
                fn visit_shared_data_references_in_partial_vote<F: Fn(SharedDataHash)>(($($t,)*): &Self::PartialVote, f: F) {
                    $(
                        <$t as SimpleVoteStorage>::visit_shared_data_references_in_partial_vote($t, &f);
                    )*
                }
            }
            impl<$($t: SimpleVoteStorage),*> private::Sealed for ($($t,)*) {}
        }
    }
}

generate_simple_vote_storage_tuple_impls!(tuple_0_impls: ());
generate_simple_vote_storage_tuple_impls!(tuple_1_impls: (A));
generate_simple_vote_storage_tuple_impls!(tuple_2_impls: (A, B));
generate_simple_vote_storage_tuple_impls!(tuple_3_impls: (A, B, C));
generate_simple_vote_storage_tuple_impls!(tuple_4_impls: (A, B, C, D));
