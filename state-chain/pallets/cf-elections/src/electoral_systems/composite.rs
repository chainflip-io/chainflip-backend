// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

/// Allows the composition of multiple ElectoralSystems while allowing the ability to configure the
/// `on_finalize` behaviour without exposing the internal composite types.
pub struct CompositeRunner<T, ValidatorId, StateChainBlockNumber, StorageAccess, H> {
	_phantom: core::marker::PhantomData<(T, ValidatorId, StateChainBlockNumber, StorageAccess, H)>,
}

/// The access wrappers need to impl the access traits once for each variant,
/// these tags ensure these trait impls don't overlap.
pub mod tags {
	pub struct A;
	pub struct B;
	pub struct C;
	pub struct D;
	pub struct EE;
	pub struct FF;
	pub struct G;
}

macro_rules! generate_electoral_system_tuple_impls {
    ($module:ident: ($(($electoral_system:ident, $electoral_system_alt_name_0:ident)),*$(,)?)) => {
        #[allow(dead_code)]
        // We use the type names as variable names.
        #[allow(non_snake_case)]
        // In the 1/identity case, no invalid combinations are possible, so error cases are unreachable.

        // Macro expands tuples, but only uses 1 element in some cases.
        #[allow(unused_variables)]
        pub mod $module {
            use super::{
                CompositeRunner,
                tags,
            };

            use crate::{
                CorruptStorageError,
                electoral_system::{
                    ElectoralSystem,
                    ElectoralSystemTypes,
                    ConsensusVote,
                    ElectionReadAccess,
                    ElectionWriteAccess,
                    ElectoralReadAccess,
                    ElectoralWriteAccess,
                    ConsensusVotes,
                    ElectionIdentifierOf,
                    PartialVoteOf,
                    VoteOf,
                    ConsensusStatus,
                },
                electoral_system_runner::{ElectoralSystemRunner, RunnerStorageAccessTrait},
                electoral_system::{AuthorityVoteOf, VotePropertiesOf},
                vote_storage::AuthorityVote,
                ElectionIdentifier,
            };
            use crate::vote_storage::composite::$module::{CompositeVoteProperties, CompositeVote, CompositePartialVote};

            use frame_support::{Parameter, pallet_prelude::{Member, MaybeSerializeDeserialize}};

            use codec::{Encode, Decode};
            use scale_info::TypeInfo;
            use sp_std::vec::Vec;

            /// This trait specifies the behaviour of the composite's `ElectoralSystem::on_finalize` function.
            pub trait Hooks<$($electoral_system: ElectoralSystem,)*> {
                fn on_finalize(
                    election_identifiers: ($(Vec<ElectionIdentifierOf<$electoral_system>>,)*),
                ) -> Result<(), CorruptStorageError>;
            }

            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
            pub enum CompositeElectoralUnsynchronisedStateMapKey<$($electoral_system,)*> {
                $($electoral_system($electoral_system),)*
            }
            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
            pub enum CompositeElectoralUnsynchronisedStateMapValue<$($electoral_system,)*> {
                $($electoral_system($electoral_system),)*
            }
            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
            pub enum CompositeElectionIdentifierExtra<$($electoral_system,)*> {
                $($electoral_system($electoral_system),)*
            }
            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
            pub enum CompositeElectionProperties<$($electoral_system,)*> {
                $($electoral_system($electoral_system),)*
            }
            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
            pub enum CompositeElectionState<$($electoral_system,)*> {
                $($electoral_system($electoral_system),)*
            }
            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
            pub enum CompositeConsensus<$($electoral_system,)*> {
                $($electoral_system($electoral_system),)*
            }

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId, StateChainBlockNumber = StateChainBlockNumber>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = Self> + 'static, H: Hooks<$($electoral_system),*> + 'static> CompositeRunner<($($electoral_system,)*), ValidatorId, StateChainBlockNumber, StorageAccess, H> {
                pub fn with_identifiers<R, F: for<'a> FnOnce(
                    ($(
                        Vec<ElectionIdentifierOf<$electoral_system>>,
                    )*)
                ) -> R>(
                    election_identifiers: Vec<ElectionIdentifierOf<Self>>,
                    f: F,
                ) -> R {
                    $(let mut $electoral_system_alt_name_0 = Vec::new();)*

                    for election_identifier in election_identifiers {
                        match *election_identifier.extra() {
                            $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                                $electoral_system_alt_name_0.push(election_identifier.with_extra(extra));
                            })*
                        }
                    }

                    f((
                        $($electoral_system_alt_name_0,)*
                    ))
                }
            }

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId, StateChainBlockNumber = StateChainBlockNumber>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = Self> + 'static, H: Hooks<$($electoral_system),*> + 'static> ElectoralSystemTypes for CompositeRunner<($($electoral_system,)*), ValidatorId, StateChainBlockNumber, StorageAccess, H> {
                type ValidatorId = ValidatorId;
                type StateChainBlockNumber = StateChainBlockNumber;
                type ElectoralUnsynchronisedState = ($(<$electoral_system as ElectoralSystemTypes>::ElectoralUnsynchronisedState,)*);
                type ElectoralUnsynchronisedStateMapKey = CompositeElectoralUnsynchronisedStateMapKey<$(<$electoral_system as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,)*>;
                type ElectoralUnsynchronisedStateMapValue = CompositeElectoralUnsynchronisedStateMapValue<$(<$electoral_system as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,)*>;
                type ElectoralUnsynchronisedSettings = ($(<$electoral_system as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,)*);
                type ElectoralSettings = ($(<$electoral_system as ElectoralSystemTypes>::ElectoralSettings,)*);

                type ElectionIdentifierExtra = CompositeElectionIdentifierExtra<$(<$electoral_system as ElectoralSystemTypes>::ElectionIdentifierExtra,)*>;
                type ElectionProperties = CompositeElectionProperties<$(<$electoral_system as ElectoralSystemTypes>::ElectionProperties,)*>;
                type ElectionState = CompositeElectionState<$(<$electoral_system as ElectoralSystemTypes>::ElectionState,)*>;
                type VoteStorage = ($(<$electoral_system as ElectoralSystemTypes>::VoteStorage,)*);
                type Consensus = CompositeConsensus<$(<$electoral_system as ElectoralSystemTypes>::Consensus,)*>;
                type OnFinalizeContext = ();
                type OnFinalizeReturn = ();
            }

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId, StateChainBlockNumber = StateChainBlockNumber>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = Self> + 'static, H: Hooks<$($electoral_system),*> + 'static> ElectoralSystemRunner for CompositeRunner<($($electoral_system,)*), ValidatorId, StateChainBlockNumber, StorageAccess, H> {
                fn is_vote_desired(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    current_vote: Option<(
                        VotePropertiesOf<Self>,
                        AuthorityVoteOf<Self>,
                    )>,
                    state_chain_block_number: Self::StateChainBlockNumber,
                ) -> Result<bool, CorruptStorageError> {
                    match *election_identifier.extra() {
                        $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                            <$electoral_system as ElectoralSystem>::is_vote_desired(
                                &DerivedElectionAccess::<tags::$electoral_system, $electoral_system, StorageAccess>::new(election_identifier.with_extra(extra)),
                                current_vote.map(|(properties, vote)| {
                                    Ok((
                                        match properties {
                                            CompositeVoteProperties::$electoral_system(properties) => properties,
                                            // For when we have a composite of 1
                                            #[allow(unreachable_patterns)]
                                            _ => return Err(CorruptStorageError::new()),
                                        },
                                        match vote {
                                            AuthorityVote::PartialVote(CompositePartialVote::$electoral_system(partial_vote)) => AuthorityVote::PartialVote(partial_vote),
                                            AuthorityVote::Vote(CompositeVote::$electoral_system(vote)) => AuthorityVote::Vote(vote),
                                            // For when we have a composite of 1
                                            #[allow(unreachable_patterns)]
                                            _ => return Err(CorruptStorageError::new()),
                                        },
                                    ))
                                }).transpose()?,
                                state_chain_block_number,
                            )
                        },)*
                    }
                }

                fn is_vote_needed(
                    (current_vote_properties, current_partial_vote, current_authority_vote): (VotePropertiesOf<Self>, PartialVoteOf<Self>, AuthorityVoteOf<Self>),
                    (proposed_partial_vote, proposed_vote): (PartialVoteOf<Self>, VoteOf<Self>),
                ) -> bool {
                    match (current_vote_properties, current_partial_vote, current_authority_vote, proposed_partial_vote, proposed_vote) {
                        $(
                            (
                                CompositeVoteProperties::$electoral_system(current_vote_properties),
                                CompositePartialVote::$electoral_system(current_partial_vote),
                                AuthorityVote::Vote(CompositeVote::$electoral_system(current_authority_vote)),
                                CompositePartialVote::$electoral_system(proposed_partial_vote),
                                CompositeVote::$electoral_system(proposed_vote),
                            ) => {
                                <$electoral_system as ElectoralSystem>::is_vote_needed(
                                    (current_vote_properties, current_partial_vote, AuthorityVote::Vote(current_authority_vote)),
                                    (proposed_partial_vote, proposed_vote),
                                )
                            },
                            (
                                CompositeVoteProperties::$electoral_system(current_vote_properties),
                                CompositePartialVote::$electoral_system(current_partial_vote),
                                AuthorityVote::PartialVote(CompositePartialVote::$electoral_system(current_authority_partial_vote)),
                                CompositePartialVote::$electoral_system(proposed_partial_vote),
                                CompositeVote::$electoral_system(proposed_vote),
                            ) => {
                                <$electoral_system as ElectoralSystem>::is_vote_needed(
                                    (current_vote_properties, current_partial_vote, AuthorityVote::PartialVote(current_authority_partial_vote)),
                                    (proposed_partial_vote, proposed_vote),
                                )
                            },
                        )*
                        // For when we have a composite of 1
                        #[allow(unreachable_patterns)]
                        _ => true,
                    }
                }

                fn generate_vote_properties(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    previous_vote: Option<(
                        VotePropertiesOf<Self>,
                        AuthorityVoteOf<Self>,
                    )>,
                    partial_vote: &PartialVoteOf<Self>,
                ) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
                    match (*election_identifier.extra(), partial_vote) {
                        $((CompositeElectionIdentifierExtra::$electoral_system(extra), CompositePartialVote::$electoral_system(partial_vote)) => {
                            <$electoral_system as ElectoralSystem>::generate_vote_properties(
                                election_identifier.with_extra(extra),
                                previous_vote.map(|(previous_properties, previous_vote)| {
                                    Ok((
                                        match previous_properties {
                                            CompositeVoteProperties::$electoral_system(previous_properties) => previous_properties,
                                            // For when we have a composite of 1
                                            #[allow(unreachable_patterns)]
                                            _ => return Err(CorruptStorageError::new()),
                                        },
                                        match previous_vote {
                                            AuthorityVote::PartialVote(CompositePartialVote::$electoral_system(partial_vote)) => AuthorityVote::PartialVote(partial_vote),
                                            AuthorityVote::Vote(CompositeVote::$electoral_system(vote)) => AuthorityVote::Vote(vote),
                                            // For when we have a composite of 1
                                            #[allow(unreachable_patterns)]
                                            _ => return Err(CorruptStorageError::new()),
                                        },
                                    ))
                                }).transpose()?,
                                partial_vote,
                            ).map(CompositeVoteProperties::$electoral_system)
                        },)*
                        // For when we have a composite of 1
                        #[allow(unreachable_patterns)]
                        _ => Err(CorruptStorageError::new()),
                    }
                }

                fn on_finalize(
                    election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
                ) -> Result<(), CorruptStorageError> {
                    Self::with_identifiers(election_identifiers, |election_identifiers| {
                        H::on_finalize(
                            election_identifiers,
                        )
                    })
                }

                fn check_consensus(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    previous_consensus: Option<&Self::Consensus>,
                    consensus_votes: ConsensusVotes<Self>,
                ) -> Result<Option<Self::Consensus>, CorruptStorageError> {
                    Ok(match *election_identifier.extra() {
                        $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                            <$electoral_system as ElectoralSystem>::check_consensus(
                                &DerivedElectionAccess::<tags::$electoral_system, _, StorageAccess>::new(election_identifier.with_extra(extra)),
                                previous_consensus.map(|previous_consensus| {
                                    match previous_consensus {
                                        CompositeConsensus::$electoral_system(previous_consensus) => Ok(previous_consensus),
                                        // For when we have a composite of 1
                                        #[allow(unreachable_patterns)]
                                        _ => Err(CorruptStorageError::new()),
                                    }
                                }).transpose()?,
                                ConsensusVotes {
                                    votes: consensus_votes.votes.into_iter().map(|ConsensusVote { vote, validator_id }| {
                                        if let Some((properties, vote)) = vote {
                                            match (properties, vote) {
                                                (
                                                    CompositeVoteProperties::$electoral_system(properties),
                                                    CompositeVote::$electoral_system(vote),
                                                ) => Ok(ConsensusVote {
                                                    vote: Some((properties, vote)),
                                                    validator_id
                                                }),
                                                // For when we have a composite of 1
                                                #[allow(unreachable_patterns)]
                                                _ => Err(CorruptStorageError::new()),
                                            }
                                        } else {
                                            Ok(ConsensusVote {
                                                vote: None,
                                                validator_id
                                            })
                                        }

                                    }).collect::<Result<Vec<_>, _>>()?
                                }
                            )?.map(CompositeConsensus::$electoral_system)
                        },)*
                    })
                }
            }

            pub struct DerivedElectionAccess<Tag, ES: ElectoralSystem, StorageAccess> {
                id: ElectionIdentifierOf<ES>,
                _phantom: core::marker::PhantomData<(Tag, ES, StorageAccess)>,
            }

            impl<Tag, ES: ElectoralSystem, StorageAccess: RunnerStorageAccessTrait> DerivedElectionAccess<Tag, ES, StorageAccess> {
                fn new(id: ElectionIdentifierOf<ES>) -> Self {
                    Self {
                        id,
                        _phantom: Default::default(),
                    }
                }
            }
            pub struct DerivedElectoralAccess<Tag, ES, StorageAccess> {
                _phantom: core::marker::PhantomData<(Tag, ES, StorageAccess)>,
            }

            // This macro solves the problem of taking a repeating argument and generating the
            // product of the arguments elements. As we need to be able to refer to every element
            // individually, while also referencing to the whole list.
            generate_electoral_system_tuple_impls!(@;$($electoral_system,)*:$($electoral_system,)*);
        }
    };
    (@ $($previous:ident,)*;: $($electoral_system:ident,)*) => {};
    (@ $($previous:ident,)*; $current:ident, $($remaining:ident,)*: $($electoral_system:ident,)*) => {

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId, StateChainBlockNumber = StateChainBlockNumber>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize, H: Hooks<$($electoral_system),*> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StateChainBlockNumber, StorageAccess, H>> + 'static> ElectionReadAccess for DerivedElectionAccess<tags::$current, $current, StorageAccess> {
            type ElectoralSystem = $current;

            fn settings(&self) -> Result<$current::ElectoralSettings, CorruptStorageError> {
                let ($($previous,)* settings, $($remaining,)*) = StorageAccess::electoral_settings_for_election(*self.id.unique_monotonic())?;
                Ok(settings)
            }
            fn properties(&self) -> Result<$current::ElectionProperties, CorruptStorageError> {
                match StorageAccess::election_properties(self.id.with_extra(CompositeElectionIdentifierExtra::$current(*self.id.extra())))? {
                    CompositeElectionProperties::$current(properties) => {
                        Ok(properties)
                    },
                    // For when we have a composite of 1
                    #[allow(unreachable_patterns)]
                    _ => Err(CorruptStorageError::new())
                }
            }
            fn state(&self) -> Result<$current::ElectionState, CorruptStorageError> {
                match StorageAccess::election_state(*self.id.unique_monotonic())? {
                    CompositeElectionState::$current(state) => {
                        Ok(state)
                    },
                    // For when we have a composite of 1
                    #[allow(unreachable_patterns)]
                    _ => Err(CorruptStorageError::new())
                }
            }

            fn election_identifier(&self) -> ElectionIdentifierOf<Self::ElectoralSystem> {
                self.id
            }
        }

        impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId, StateChainBlockNumber = StateChainBlockNumber>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize, H: Hooks<$($electoral_system),*> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StateChainBlockNumber, StorageAccess, H>> + 'static> ElectionWriteAccess for DerivedElectionAccess<tags::$current, $current, StorageAccess> {
            fn set_state(&self, state: $current::ElectionState) -> Result<(), CorruptStorageError> {
                StorageAccess::set_election_state(*self.id.unique_monotonic(), CompositeElectionState::$current(state))
            }
            fn clear_votes(&self) {
                StorageAccess::clear_election_votes(*self.id.unique_monotonic());
            }
            fn delete(self) {
                StorageAccess::delete_election(self.id.with_extra(CompositeElectionIdentifierExtra::$current(*self.id.extra())));
            }
            fn refresh(
                &mut self,
                new_extra: $current::ElectionIdentifierExtra,
                properties: $current::ElectionProperties,
            ) -> Result<(), CorruptStorageError> {
                StorageAccess::refresh_election(
                    // The current election id + extra that we want to refresh.
                    self.id.with_extra(CompositeElectionIdentifierExtra::$current(*self.id.extra())),
                    // The new extra we want to use.
                    CompositeElectionIdentifierExtra::$current(new_extra),
                    CompositeElectionProperties::$current(properties),
                )?;
                self.id = self.id.with_extra(new_extra);
                Ok(())
            }
            fn check_consensus(
                &self,
            ) -> Result<ConsensusStatus<$current::Consensus>, CorruptStorageError> {
                StorageAccess::check_election_consensus(self.id.with_extra(CompositeElectionIdentifierExtra::$current(*self.id.extra()))).and_then(|consensus_status| {
                    consensus_status.try_map(|consensus| {
                        match consensus {
                            CompositeConsensus::$current(consensus) => Ok(consensus),
                            // For when we have a composite of 1
                            #[allow(unreachable_patterns)]
                            _ => Err(CorruptStorageError::new()),
                        }
                    })
                })
            }
        }

        impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId, StateChainBlockNumber = StateChainBlockNumber>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize, H: Hooks<$($electoral_system),*> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StateChainBlockNumber, StorageAccess, H>> + 'static> ElectoralReadAccess for DerivedElectoralAccess<tags::$current, $current, StorageAccess> {
            type ElectoralSystem = $current;
            type ElectionReadAccess = DerivedElectionAccess<tags::$current, $current, StorageAccess>;

            fn election(
                id: ElectionIdentifier<<$current as ElectoralSystemTypes>::ElectionIdentifierExtra>,
            ) -> Self::ElectionReadAccess {
                DerivedElectionAccess::<tags::$current, _, StorageAccess>::new(id)
            }
            fn unsynchronised_settings(
            ) -> Result<$current::ElectoralUnsynchronisedSettings, CorruptStorageError> {
                let ($($previous,)* unsynchronised_settings, $($remaining,)*) = StorageAccess::unsynchronised_settings()?;
                Ok(unsynchronised_settings)
            }
            fn unsynchronised_state(
            ) -> Result<$current::ElectoralUnsynchronisedState, CorruptStorageError> {
                let ($($previous,)* unsynchronised_state, $($remaining,)*) = StorageAccess::unsynchronised_state()?;
                Ok(unsynchronised_state)
            }
            fn unsynchronised_state_map(
                key: &$current::ElectoralUnsynchronisedStateMapKey,
            ) -> Result<Option<$current::ElectoralUnsynchronisedStateMapValue>, CorruptStorageError> {
                match StorageAccess::unsynchronised_state_map(&CompositeElectoralUnsynchronisedStateMapKey::$current(key.clone())) {
                    Some(CompositeElectoralUnsynchronisedStateMapValue::$current(value)) => Ok(Some(value)),
                    None => Ok(None),
                    // For when we have a composite of 1
                    #[allow(unreachable_patterns)]
                    _ => Err(CorruptStorageError::new()),
                }
            }
        }

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId, StateChainBlockNumber = StateChainBlockNumber>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize, H: Hooks<$($electoral_system),*> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StateChainBlockNumber, StorageAccess, H>> + 'static> ElectoralWriteAccess for DerivedElectoralAccess<tags::$current, $current, StorageAccess> {
            type ElectionWriteAccess = DerivedElectionAccess<tags::$current, $current, StorageAccess>;

            fn new_election(
                extra: $current::ElectionIdentifierExtra,
                properties: $current::ElectionProperties,
                state: $current::ElectionState,
            ) -> Result<Self::ElectionWriteAccess, CorruptStorageError> {
                let election_identifier = StorageAccess::new_election(CompositeElectionIdentifierExtra::$current(extra), CompositeElectionProperties::$current(properties), CompositeElectionState::$current(state))?;
                Ok(Self::election_mut(election_identifier.with_extra(extra)))
            }

            fn election_mut(
                id: ElectionIdentifier<$current::ElectionIdentifierExtra>,
            ) -> Self::ElectionWriteAccess {
                Self::ElectionWriteAccess::new(id)
            }

            fn set_unsynchronised_state(
                unsynchronised_state: $current::ElectoralUnsynchronisedState,
            ) -> Result<(), CorruptStorageError> {
                let ($($previous,)* _, $($remaining,)*) = StorageAccess::unsynchronised_state()?;
                StorageAccess::set_unsynchronised_state(($($previous,)* unsynchronised_state, $($remaining,)*));
                Ok(())
            }

            fn set_unsynchronised_state_map(
                key: $current::ElectoralUnsynchronisedStateMapKey,
                value: Option<$current::ElectoralUnsynchronisedStateMapValue>,
            ) {
                StorageAccess::set_unsynchronised_state_map(
                    CompositeElectoralUnsynchronisedStateMapKey::$current(key),
                    value.map(CompositeElectoralUnsynchronisedStateMapValue::$current),
                );
            }

            fn mutate_unsynchronised_state<
                T,
                F: for<'b> FnOnce(
                    &'b mut $current::ElectoralUnsynchronisedState,
                ) -> Result<T, CorruptStorageError>,
            >(
                f: F,
            ) -> Result<T, CorruptStorageError> {
                let ($($previous,)* mut unsynchronised_state, $($remaining,)*) = StorageAccess::unsynchronised_state()?;
                let t = f( &mut unsynchronised_state)?;
                StorageAccess::set_unsynchronised_state(($($previous,)* unsynchronised_state, $($remaining,)*));
                Ok(t)
            }
        }

        generate_electoral_system_tuple_impls!(@ $($previous,)* $current,; $($remaining,)*: $($electoral_system,)*);
    };
}

generate_electoral_system_tuple_impls!(tuple_1_impls: ((A, A0),));
generate_electoral_system_tuple_impls!(tuple_6_impls: ((A, A0), (B, B0), (C, C0), (D, D0), (EE, E0), (FF, F0)));
generate_electoral_system_tuple_impls!(tuple_7_impls: ((A, A0), (B, B0), (C, C0), (D, D0), (EE, E0), (FF, F0), (G, G0)));
