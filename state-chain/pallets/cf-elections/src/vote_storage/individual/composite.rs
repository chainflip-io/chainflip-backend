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
#[cfg(test)]
generate_individual_vote_storage_tuple_impls!(tuple_2_impls: (A, B));
generate_individual_vote_storage_tuple_impls!(tuple_7_impls: (A, B, C, D, EE, FF, GG));
