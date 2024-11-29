/// Allows the composition of multiple ElectoralSystems while allowing the ability to configure the
/// `on_finalize` behaviour without exposing the internal composite types.
pub struct CompositeRunner<T, ValidatorId, StorageAccess, H> {
	_phantom: core::marker::PhantomData<(T, ValidatorId, StorageAccess, H)>,
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
	pub struct GG;
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
                    ConsensusVote,
                    ElectionReadAccess,
                    ElectionWriteAccess,
                    ElectoralReadAccess,
                    ElectoralWriteAccess,
                    ConsensusVotes,
                    ElectionIdentifierOf,
                    ConsensusStatus,
                },
                electoral_system_runner::{ElectoralSystemRunner, CompositeAuthorityVoteOf, RunnerStorageAccessTrait,
                    CompositeVotePropertiesOf, CompositeConsensusVotes, CompositeElectionIdentifierOf, CompositeConsensusVote},
                vote_storage::{
                    AuthorityVote,
                    VoteStorage
                },
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

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = Self> + 'static, H: Hooks<$($electoral_system),*> + 'static> CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H> {
                pub fn with_identifiers<R, F: for<'a> FnOnce(
                    ($(
                        Vec<ElectionIdentifierOf<$electoral_system>>,
                    )*)
                ) -> R>(
                    election_identifiers: Vec<CompositeElectionIdentifierOf<Self>>,
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

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = Self> + 'static, H: Hooks<$($electoral_system),*> + 'static> ElectoralSystemRunner for CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H> {
                type ValidatorId = ValidatorId;
                type ElectoralUnsynchronisedState = ($(<$electoral_system as ElectoralSystem>::ElectoralUnsynchronisedState,)*);
                type ElectoralUnsynchronisedStateMapKey = CompositeElectoralUnsynchronisedStateMapKey<$(<$electoral_system as ElectoralSystem>::ElectoralUnsynchronisedStateMapKey,)*>;
                type ElectoralUnsynchronisedStateMapValue = CompositeElectoralUnsynchronisedStateMapValue<$(<$electoral_system as ElectoralSystem>::ElectoralUnsynchronisedStateMapValue,)*>;
                type ElectoralUnsynchronisedSettings = ($(<$electoral_system as ElectoralSystem>::ElectoralUnsynchronisedSettings,)*);
                type ElectoralSettings = ($(<$electoral_system as ElectoralSystem>::ElectoralSettings,)*);

                type ElectionIdentifierExtra = CompositeElectionIdentifierExtra<$(<$electoral_system as ElectoralSystem>::ElectionIdentifierExtra,)*>;
                type ElectionProperties = CompositeElectionProperties<$(<$electoral_system as ElectoralSystem>::ElectionProperties,)*>;
                type ElectionState = CompositeElectionState<$(<$electoral_system as ElectoralSystem>::ElectionState,)*>;
                type Vote = ($(<$electoral_system as ElectoralSystem>::Vote,)*);
                type Consensus = CompositeConsensus<$(<$electoral_system as ElectoralSystem>::Consensus,)*>;

                fn is_vote_desired(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    current_vote: Option<(
                        CompositeVotePropertiesOf<Self>,
                        CompositeAuthorityVoteOf<Self>,
                    )>,
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
                            )
                        },)*
                    }
                }

                fn is_vote_needed(
                    (current_vote_properties, current_partial_vote, current_authority_vote): (CompositeVotePropertiesOf<Self>, <Self::Vote as VoteStorage>::PartialVote, CompositeAuthorityVoteOf<Self>),
                    (proposed_partial_vote, proposed_vote): (<Self::Vote as VoteStorage>::PartialVote, <Self::Vote as VoteStorage>::Vote),
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
                        CompositeVotePropertiesOf<Self>,
                        CompositeAuthorityVoteOf<Self>,
                    )>,
                    partial_vote: &<Self::Vote as VoteStorage>::PartialVote,
                ) -> Result<CompositeVotePropertiesOf<Self>, CorruptStorageError> {
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
                    consensus_votes: CompositeConsensusVotes<Self>,
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
                                    votes: consensus_votes.votes.into_iter().map(|CompositeConsensusVote { vote, validator_id }| {
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

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> ElectionReadAccess for DerivedElectionAccess<tags::$current, $current, StorageAccess> {
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

        impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> ElectionWriteAccess for DerivedElectionAccess<tags::$current, $current, StorageAccess> {
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

        impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> ElectoralReadAccess for DerivedElectoralAccess<tags::$current, $current, StorageAccess> {
            type ElectoralSystem = $current;
            type ElectionReadAccess = DerivedElectionAccess<tags::$current, $current, StorageAccess>;

            fn election(
                id: ElectionIdentifier<<$current as ElectoralSystem>::ElectionIdentifierExtra>,
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

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> ElectoralWriteAccess for DerivedElectoralAccess<tags::$current, $current, StorageAccess> {
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

generate_electoral_system_tuple_impls!(tuple_1_impls: ((A, A0)));
generate_electoral_system_tuple_impls!(tuple_7_impls: ((A, A0), (B, B0), (C, C0), (D, D0), (EE, E0), (FF, F0), (GG, G0)));
