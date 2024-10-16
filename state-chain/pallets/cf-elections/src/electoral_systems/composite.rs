use crate::electoral_system::{ElectoralSystem, ElectoralWriteAccess};

/// Allows the composition of multiple ElectoralSystems while allowing the ability to configure the
/// `on_finalize` behaviour without exposing the internal composite types.
pub struct CompositeRunner<T, ValidatorId, StorageAccess, H = DefaultHooks<(), StorageAccess>> {
	_phantom: core::marker::PhantomData<(T, ValidatorId, StorageAccess, H)>,
}

pub struct DefaultHooks<OnFinalizeContext, StorageAccess> {
	_phantom: core::marker::PhantomData<(OnFinalizeContext, StorageAccess)>,
}

/// Takes a generic storage access type and then can translate into an election access type for a
/// specific electoral system.
pub trait Translator<StorageAccess> {
	type ElectoralSystem: ElectoralSystem;
	type ElectionAccess<'a>: ElectoralWriteAccess<ElectoralSystem = Self::ElectoralSystem>
	where
		Self: 'a,
		StorageAccess: 'a;

	fn translate_electoral_access<'a>(
		&'a self,
		storage_access: &'a mut StorageAccess,
	) -> Self::ElectionAccess<'a>;
}

/// The access wrappers need to impl the access traits once for each variant,
/// these tags ensure these trait impls don't overlap.
mod tags {
	pub struct A;
	pub struct B;
	pub struct C;
	pub struct D;
	pub struct EE;
	pub struct FF;
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
                Translator,
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
                electoral_system_runner::{ElectoralSystemRunner, AuthorityVoteOf, RunnerStorageAccessTrait,
                    VotePropertiesOf, CompositeConsensusVotes, CompositeElectionIdentifierOf, CompositeConsensusVote},
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

            /// This trait specifies the behaviour of the composite's `ElectoralSystem::on_finalize` without that code being exposed to the internals of the composite by using the Translator trait to obtain ElectoralAccess objects that abstract those details.
            pub trait Hooks<$($electoral_system: ElectoralSystem,)*> {

                type StorageAccess: RunnerStorageAccessTrait;

                fn on_finalize<$($electoral_system_alt_name_0: Translator<Self::StorageAccess, ElectoralSystem = $electoral_system>),*>(
                    storage_access: &mut Self::StorageAccess,
                    electoral_access_translators: ($($electoral_system_alt_name_0,)*),
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

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = Self> + 'static, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static> CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H> {
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

                pub fn with_access_translators<R, F: for<'a> FnOnce(
                    ($(
                        ElectoralAccessTranslator<tags::$electoral_system, $electoral_system, StorageAccess>,
                    )*)
                ) -> R>(
                    f: F,
                ) -> R {
                    f((
                        $(ElectoralAccessTranslator::<tags::$electoral_system, $electoral_system, StorageAccess>::new(),)*
                    ))
                }
            }

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = Self> + 'static, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static> ElectoralSystemRunner for CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H> {
                type StorageAccess = StorageAccess;
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
                    storage_access: &Self::StorageAccess,
                    current_vote: Option<(
                        VotePropertiesOf<Self>,
                        AuthorityVoteOf<Self>,
                    )>,
                ) -> Result<bool, CorruptStorageError> {
                    match *election_identifier.extra() {
                        $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                            <$electoral_system as ElectoralSystem>::is_vote_desired(
                                election_identifier.with_extra(extra),
                                &CompositeElectionAccess::<tags::$electoral_system, $electoral_system, StorageAccess>::new(storage_access, election_identifier.with_extra(extra)),
                                current_vote.map(|(properties, vote)| {
                                    Ok((
                                        match properties {
                                            CompositeVoteProperties::$electoral_system(properties) => properties,
                                            _ => return Err(CorruptStorageError::new()),
                                        },
                                        match vote {
                                            AuthorityVote::PartialVote(CompositePartialVote::$electoral_system(partial_vote)) => AuthorityVote::PartialVote(partial_vote),
                                            AuthorityVote::Vote(CompositeVote::$electoral_system(vote)) => AuthorityVote::Vote(vote),
                                            _ => return Err(CorruptStorageError::new()),
                                        },
                                    ))
                                }).transpose()?,
                            )
                        },)*
                    }
                }

                fn is_vote_needed(
                    (current_vote_properties, current_partial_vote, current_authority_vote): (VotePropertiesOf<Self>, <Self::Vote as VoteStorage>::PartialVote, AuthorityVoteOf<Self>),
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
                        _ => true,
                    }
                }


                fn is_vote_valid(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    storage_access: &Self::StorageAccess,
                    partial_vote: &<Self::Vote as VoteStorage>::PartialVote,
                ) -> Result<bool, CorruptStorageError> {
                    Ok(match (*election_identifier.extra(), partial_vote) {
                        $((CompositeElectionIdentifierExtra::$electoral_system(extra), CompositePartialVote::$electoral_system(partial_vote)) => <$electoral_system as ElectoralSystem>::is_vote_valid(
                            election_identifier.with_extra(extra),
                            &CompositeElectionAccess::<tags::$electoral_system, $electoral_system, Self::StorageAccess>::new(storage_access, election_identifier.with_extra(extra)),
                            partial_vote,
                        )?,)*
                        _ => false,
                    })
                }

                fn generate_vote_properties(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    previous_vote: Option<(
                        VotePropertiesOf<Self>,
                        AuthorityVoteOf<Self>,
                    )>,
                    partial_vote: &<Self::Vote as VoteStorage>::PartialVote,
                ) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
                    match (*election_identifier.extra(), partial_vote) {
                        $((CompositeElectionIdentifierExtra::$electoral_system(extra), CompositePartialVote::$electoral_system(partial_vote)) => {
                            <$electoral_system as ElectoralSystem>::generate_vote_properties(
                                election_identifier.with_extra(extra),
                                previous_vote.map(|(previous_properties, previous_vote)| {
                                    Ok((
                                        match previous_properties {
                                            CompositeVoteProperties::$electoral_system(previous_properties) => previous_properties,
                                            _ => return Err(CorruptStorageError::new()),
                                        },
                                        match previous_vote {
                                            AuthorityVote::PartialVote(CompositePartialVote::$electoral_system(partial_vote)) => AuthorityVote::PartialVote(partial_vote),
                                            AuthorityVote::Vote(CompositeVote::$electoral_system(vote)) => AuthorityVote::Vote(vote),
                                            _ => return Err(CorruptStorageError::new()),
                                        },
                                    ))
                                }).transpose()?,
                                partial_vote,
                            ).map(CompositeVoteProperties::$electoral_system)
                        },)*
                        _ => Err(CorruptStorageError::new()),
                    }
                }

                fn on_finalize(
                    storage_access: &mut Self::StorageAccess,
                    election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
                ) -> Result<(), CorruptStorageError> {
                    Self::with_access_translators(|access_translators| {
                        Self::with_identifiers(election_identifiers, |election_identifiers| {
                            H::on_finalize(
                                storage_access,
                                access_translators,
                                election_identifiers,
                            )
                        })
                    })
                }

                fn check_consensus(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    election_access: &Self::StorageAccess,
                    previous_consensus: Option<&Self::Consensus>,
                    consensus_votes: CompositeConsensusVotes<Self>,
                ) -> Result<Option<Self::Consensus>, CorruptStorageError> {
                    Ok(match *election_identifier.extra() {
                        $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                            <$electoral_system as ElectoralSystem>::check_consensus(
                                // The elction access should already have this, why do we need to pass it in again?
                                election_identifier.with_extra(extra),
                                &CompositeElectionAccess::<tags::$electoral_system, _, Self::StorageAccess>::new(election_access, election_identifier.with_extra(extra)),
                                previous_consensus.map(|previous_consensus| {
                                    match previous_consensus {
                                        CompositeConsensus::$electoral_system(previous_consensus) => Ok(previous_consensus),
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

            pub struct CompositeElectionAccess<'a, Tag, ES: ElectoralSystem, StorageAccess> {
                id: ElectionIdentifierOf<ES>,
                storage_access: &'a StorageAccess,
                _phantom: core::marker::PhantomData<(Tag, ES)>,
            }

            impl<'a, Tag, ES: ElectoralSystem, StorageAccess: RunnerStorageAccessTrait> CompositeElectionAccess<'a, Tag, ES, StorageAccess> {
                fn new(storage_access: &'a StorageAccess, id: ElectionIdentifierOf<ES>) -> Self {
                    Self {
                        id,
                        storage_access,
                        _phantom: Default::default(),
                    }
                }
            }
            pub struct CompositeElectoralAccess<'a, Tag, ES, StorageAccess> {
                storage_access: &'a StorageAccess,
                _phantom: core::marker::PhantomData<(Tag, ES)>,
            }
            impl<'a, Tag, ES, StorageAccess> CompositeElectoralAccess<'a, Tag, ES, StorageAccess> {
                fn new(storage_access: &'a StorageAccess) -> Self {
                    Self {
                        storage_access,
                        _phantom: Default::default(),
                    }
                }
            }

            pub struct ElectoralAccessTranslator<Tag, ES, Runner> {
                _phantom: core::marker::PhantomData<(Tag, ES, Runner)>,
            }
            impl<Tag, ES, Runner> ElectoralAccessTranslator<Tag, ES, Runner> {
                fn new() -> Self {
                    Self {
                        _phantom: Default::default(),
                    }
                }
            }

            // This macro solves the problem of taking a repeating argument and generating the
            // product of the arguments elements. As we need to be able to refer to every element
            // individually, while also referencing to the whole list.
            generate_electoral_system_tuple_impls!(@;$($electoral_system,)*:$($electoral_system,)*);
        }
    };
    (@ $($previous:ident,)*;: $($electoral_system:ident,)*) => {};
    (@ $($previous:ident,)*; $current:ident, $($remaining:ident,)*: $($electoral_system:ident,)*) => {

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> ElectionReadAccess for CompositeElectionAccess<'a, tags::$current, $current, StorageAccess> {
            type ElectoralSystem = $current;

            fn settings(&self) -> Result<$current::ElectoralSettings, CorruptStorageError> {
                let ($($previous,)* settings, $($remaining,)*) = self.storage_access.electoral_settings_for_election(*self.id.unique_monotonic())?;
                Ok(settings)
            }
            fn properties(&self) -> Result<$current::ElectionProperties, CorruptStorageError> {
                match self.storage_access.election_properties(self.id.with_extra(CompositeElectionIdentifierExtra::$current(*self.id.extra())))? {
                    CompositeElectionProperties::$current(properties) => {
                        Ok(properties)
                    },
                    _ => Err(CorruptStorageError::new())
                }
            }
            fn state(&self) -> Result<$current::ElectionState, CorruptStorageError> {
                match self.storage_access.election_state(*self.id.unique_monotonic())? {
                    CompositeElectionState::$current(state) => {
                        Ok(state)
                    },
                    _ => Err(CorruptStorageError::new())
                }
            }

            // This is broken now - since we don't actually store the identifier in the storage.
            #[cfg(test)]
            fn election_identifier(&self) -> Result<ElectionIdentifierOf<Self::ElectoralSystem>, CorruptStorageError> {
                let composite_identifier = self.runner.borrow().election_identifier()?;
                let extra = match composite_identifier.extra() {
                    CompositeElectionIdentifierExtra::$current(extra) => Ok(extra),
                    _ => Err(CorruptStorageError::new()),
                }?;
                Ok(composite_identifier.with_extra(*extra))
            }
        }

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> ElectionWriteAccess for CompositeElectionAccess<'a, tags::$current, $current, StorageAccess> {
            fn set_state(&self, state: $current::ElectionState) -> Result<(), CorruptStorageError> {
                self.storage_access.set_election_state(*self.id.unique_monotonic(), CompositeElectionState::$current(state))
            }
            fn clear_votes(&self) {
                StorageAccess::clear_election_votes(*self.id.unique_monotonic());
            }
            fn delete(self) {
                self.storage_access.delete_election(self.id.with_extra(CompositeElectionIdentifierExtra::$current(*self.id.extra())));
            }
            fn refresh(
                &mut self,
                extra: $current::ElectionIdentifierExtra,
                properties: $current::ElectionProperties,
            ) -> Result<(), CorruptStorageError> {
                StorageAccess::refresh(
                    self.id.with_extra(CompositeElectionIdentifierExtra::$current(*self.id.extra())),
                    CompositeElectionIdentifierExtra::$current(extra),
                    CompositeElectionProperties::$current(properties),
                )?;
                self.id = self.id.with_extra(extra);
                Ok(())
            }
            fn check_consensus(
                &self,
            ) -> Result<ConsensusStatus<$current::Consensus>, CorruptStorageError> {
                self.storage_access.check_consensus(self.id.with_extra(CompositeElectionIdentifierExtra::$current(*self.id.extra()))).and_then(|consensus_status| {
                    consensus_status.try_map(|consensus| {
                        match consensus {
                            CompositeConsensus::$current(composite_consensus) => {
                                Ok(composite_consensus)

                            },
                            _ => Err(CorruptStorageError::new()),
                        }
                    })
                })
            }
        }

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> ElectoralReadAccess for CompositeElectoralAccess<'a, tags::$current, $current, StorageAccess> {
            type ElectoralSystem = $current;
            type ElectionReadAccess<'b> = CompositeElectionAccess<'a, tags::$current, $current, StorageAccess>
            where
                Self: 'b;

            fn election(
                &self,
                id: ElectionIdentifier<<$current as ElectoralSystem>::ElectionIdentifierExtra>,
            ) -> Result<Self::ElectionReadAccess<'_>, CorruptStorageError> {
                Ok(CompositeElectionAccess::<tags::$current, _, StorageAccess>::new(self.storage_access, id))
            }
            fn unsynchronised_settings(
                &self,
            ) -> Result<$current::ElectoralUnsynchronisedSettings, CorruptStorageError> {
                let ($($previous,)* unsynchronised_settings, $($remaining,)*) = self.storage_access.unsynchronised_settings()?;
                Ok(unsynchronised_settings)
            }
            fn unsynchronised_state(
                &self,
            ) -> Result<$current::ElectoralUnsynchronisedState, CorruptStorageError> {
                let ($($previous,)* unsynchronised_state, $($remaining,)*) = self.storage_access.unsynchronised_state()?;
                Ok(unsynchronised_state)
            }
            fn unsynchronised_state_map(
                &self,
                key: &$current::ElectoralUnsynchronisedStateMapKey,
            ) -> Result<Option<$current::ElectoralUnsynchronisedStateMapValue>, CorruptStorageError> {
                match self.storage_access.unsynchronised_state_map(&CompositeElectoralUnsynchronisedStateMapKey::$current(key.clone()))? {
                    Some(CompositeElectoralUnsynchronisedStateMapValue::$current(value)) => Ok(Some(value)),
                    None => Ok(None),
                    _ => Err(CorruptStorageError::new()),
                }
            }
        }

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> ElectoralWriteAccess for CompositeElectoralAccess<'a, tags::$current, $current, StorageAccess> {
            type ElectionWriteAccess<'b> = CompositeElectionAccess<'a, tags::$current, $current, StorageAccess>
            where
                Self: 'b;

            fn new_election(
                &mut self,
                extra: $current::ElectionIdentifierExtra,
                properties: $current::ElectionProperties,
                state: $current::ElectionState,
            ) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError> {
                let election_identifier = self.storage_access.new_election(CompositeElectionIdentifierExtra::$current(extra), CompositeElectionProperties::$current(properties), CompositeElectionState::$current(state))?;
                Ok(Self::ElectionWriteAccess::new(self.storage_access, election_identifier.with_extra(extra)))
            }

            fn election_mut(
                &mut self,
                id: ElectionIdentifier<$current::ElectionIdentifierExtra>,
            ) -> Self::ElectionWriteAccess<'_> {
                Self::ElectionWriteAccess::new(self.storage_access, id)
            }

            fn set_unsynchronised_state(
                &self,
                unsynchronised_state: $current::ElectoralUnsynchronisedState,
            ) -> Result<(), CorruptStorageError> {
                let ($($previous,)* _, $($remaining,)*) = self.storage_access.unsynchronised_state()?;
                self.storage_access.set_unsynchronised_state(($($previous,)* unsynchronised_state, $($remaining,)*))
            }

            fn set_unsynchronised_state_map(
                &self,
                key: $current::ElectoralUnsynchronisedStateMapKey,
                value: Option<$current::ElectoralUnsynchronisedStateMapValue>,
            ) -> Result<(), CorruptStorageError> {
                self.storage_access.set_unsynchronised_state_map(
                    CompositeElectoralUnsynchronisedStateMapKey::$current(key),
                    value.map(CompositeElectoralUnsynchronisedStateMapValue::$current),
                )
            }

            fn mutate_unsynchronised_state<
                T,
                F: for<'b> FnOnce(
                    &mut Self,
                    &'b mut $current::ElectoralUnsynchronisedState,
                ) -> Result<T, CorruptStorageError>,
            >(
                &mut self,
                f: F,
            ) -> Result<T, CorruptStorageError> {
                let ($($previous,)* mut unsynchronised_state, $($remaining,)*) = self.storage_access.unsynchronised_state()?;
                let t = f(self, &mut unsynchronised_state)?;
                self.storage_access.set_unsynchronised_state(($($previous,)* unsynchronised_state, $($remaining,)*))?;
                Ok(t)
            }
        }

        impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H>> + 'static> Translator<StorageAccess> for ElectoralAccessTranslator<tags::$current, $current, StorageAccess> {

            type ElectoralSystem = $current;
            type ElectionAccess<'a> = CompositeElectoralAccess<'a, tags::$current, $current, StorageAccess>
            where
                Self: 'a;

            fn translate_electoral_access<'a>(&'a self, storage_access: &'a mut StorageAccess) -> Self::ElectionAccess<'a> {
                Self::ElectionAccess::<'a>::new(storage_access)
            }
        }

        generate_electoral_system_tuple_impls!(@ $($previous,)* $current,; $($remaining,)*: $($electoral_system,)*);
    };
}

generate_electoral_system_tuple_impls!(tuple_6_impls: ((A, A0), (B, B0), (C, C0), (D, D0), (EE, E0), (FF, F0)));
