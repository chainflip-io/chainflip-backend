use crate::electoral_system::{ElectoralSystem, ElectoralWriteAccess};

/// Allows the composition of multiple ElectoralSystems while allowing the ability to configure the
/// `on_finalize` behaviour without exposing the internal composite types.
pub struct Composite<T, ValidatorId, H = DefaultHooks<()>> {
	_phantom: core::marker::PhantomData<(T, ValidatorId, H)>,
}

pub struct DefaultHooks<OnFinalizeContext> {
	_phantom: core::marker::PhantomData<OnFinalizeContext>,
}

pub trait Translator<GenericElectoralAccess> {
	type ElectoralSystem: ElectoralSystem;
	type ElectionAccess<'a>: ElectoralWriteAccess<ElectoralSystem = Self::ElectoralSystem>
	where
		Self: 'a,
		GenericElectoralAccess: 'a;

	fn translate_electoral_access<'a>(
		&'a self,
		generic_electoral_access: &'a mut GenericElectoralAccess,
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
                Composite,
                DefaultHooks,
                tags,
            };

            use crate::{
                CorruptStorageError,
                electoral_system::{
                    ElectoralSystem,
                    ConsensusStatus,
                    ConsensusVote,
                    ElectionReadAccess,
                    ElectionWriteAccess,
                    ElectoralReadAccess,
                    ElectoralWriteAccess,
                    AuthorityVoteOf,
                    VotePropertiesOf,
                    ElectionIdentifierOf,
                    ConsensusVotes,
                },
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
            use core::borrow::Borrow;
            use sp_std::vec::Vec;

            /// This trait specifies the behaviour of the composite's `ElectoralSystem::on_finalize` without that code being exposed to the internals of the composite by using the Translator trait to obtain ElectoralAccess objects that abstract those details.
            pub trait Hooks<$($electoral_system: ElectoralSystem,)*> {
                /// The `OnFinalizeContext` of the composite's ElectoralSystem implementation.
                type OnFinalizeContext;

                /// The 'OnFinalizeReturn' of the composite's ElectoralSystem implementation.
                type OnFinalizeReturn;

                fn on_finalize<GenericElectoralAccess, $($electoral_system_alt_name_0: Translator<GenericElectoralAccess, ElectoralSystem = $electoral_system>),*>(
                    generic_electoral_access: &mut GenericElectoralAccess,
                    electoral_access_translators: ($($electoral_system_alt_name_0,)*),
                    election_identifiers: ($(Vec<ElectionIdentifierOf<$electoral_system>>,)*),
                    context: &Self::OnFinalizeContext,
                ) -> Result<Self::OnFinalizeReturn, CorruptStorageError>;
            }

            impl<OnFinalizeContext, $($electoral_system: ElectoralSystem<OnFinalizeContext = OnFinalizeContext>,)*> Hooks<$($electoral_system,)*> for DefaultHooks<OnFinalizeContext> {
                type OnFinalizeContext = OnFinalizeContext;
                type OnFinalizeReturn = ();

                fn on_finalize<GenericElectoralAccess, $($electoral_system_alt_name_0: Translator<GenericElectoralAccess, ElectoralSystem = $electoral_system>),*>(
                    generic_electoral_access: &mut GenericElectoralAccess,
                    electoral_access_translators: ($($electoral_system_alt_name_0,)*),
                    election_identifiers: ($(Vec<ElectionIdentifier<$electoral_system::ElectionIdentifierExtra>>,)*),
                    context: &Self::OnFinalizeContext,
                ) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
                    let ($($electoral_system,)*) = electoral_access_translators;
                    let ($($electoral_system_alt_name_0,)*) = election_identifiers;

                    $(
                        $electoral_system::on_finalize(&mut $electoral_system.translate_electoral_access(generic_electoral_access), $electoral_system_alt_name_0, &context)?;
                    )*

                    Ok(())
                }
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

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static> Composite<($($electoral_system,)*), ValidatorId, H> {
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

                pub fn with_access_translators<R, F: for<'a> FnOnce(
                    ($(
                        ElectoralAccessTranslator<tags::$electoral_system, Self>,
                    )*)
                ) -> R>(
                    f: F,
                ) -> R {
                    f((
                        $(ElectoralAccessTranslator::<tags::$electoral_system, Self>::new(),)*
                    ))
                }
            }

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static> ElectoralSystem for Composite<($($electoral_system,)*), ValidatorId, H> {
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

                type OnFinalizeContext = H::OnFinalizeContext;
                type OnFinalizeReturn = H::OnFinalizeReturn;

                fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    election_access: &ElectionAccess,
                    current_vote: Option<(
                        VotePropertiesOf<Self>,
                        AuthorityVoteOf<Self>,
                    )>,
                ) -> Result<bool, CorruptStorageError> {
                    match *election_identifier.extra() {
                        $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                            <$electoral_system as ElectoralSystem>::is_vote_desired(
                                election_identifier.with_extra(extra),
                                &CompositeElectionAccess::<tags::$electoral_system, _, ElectionAccess>::new(election_access),
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

                fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
                    electoral_access: &mut ElectoralAccess,
                    election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
                    context: &Self::OnFinalizeContext,
                ) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
                    Self::with_access_translators(|access_translators| {
                        Self::with_identifiers(election_identifiers, |election_identifiers| {
                            H::on_finalize(
                                electoral_access,
                                access_translators,
                                election_identifiers,
                                context
                            )
                        })
                    })
                }

                fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    election_access: &ElectionAccess,
                    previous_consensus: Option<&Self::Consensus>,
                    consensus_votes: ConsensusVotes<Self>,
                ) -> Result<Option<Self::Consensus>, CorruptStorageError> {
                    Ok(match *election_identifier.extra() {
                        $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                            <$electoral_system as ElectoralSystem>::check_consensus(
                                election_identifier.with_extra(extra),
                                &CompositeElectionAccess::<tags::$electoral_system, _, ElectionAccess>::new(election_access),
                                previous_consensus.map(|previous_consensus| {
                                    match previous_consensus {
                                        CompositeConsensus::$electoral_system(previous_consensus) => Ok(previous_consensus),
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

            pub struct CompositeElectionAccess<Tag, BorrowEA, EA> {
                ea: BorrowEA,
                _phantom: core::marker::PhantomData<(Tag, EA)>,
            }
            impl<Tag, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*>, BorrowEA: Borrow<EA>, EA: ElectionReadAccess<ElectoralSystem = Composite<($($electoral_system,)*), ValidatorId, H>>> CompositeElectionAccess<Tag, BorrowEA, EA> {
                fn new(ea: BorrowEA) -> Self {
                    Self {
                        ea,
                        _phantom: Default::default(),
                    }
                }
            }
            pub struct CompositeElectoralAccess<'a, Tag, EA> {
                ea: &'a mut EA,
                _phantom: core::marker::PhantomData<Tag>,
            }
            impl<'a, Tag, EA> CompositeElectoralAccess<'a, Tag, EA> {
                fn new(ea: &'a mut EA) -> Self {
                    Self {
                        ea,
                        _phantom: Default::default(),
                    }
                }
            }

            pub struct ElectoralAccessTranslator<Tag, EA> {
                _phantom: core::marker::PhantomData<(Tag, EA)>,
            }
            impl<Tag, EA> ElectoralAccessTranslator<Tag, EA> {
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
        impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static, BorrowEA: Borrow<EA>, EA: ElectionReadAccess<ElectoralSystem = Composite<($($electoral_system,)*), ValidatorId, H>>> ElectionReadAccess for CompositeElectionAccess<tags::$current, BorrowEA, EA> {
            type ElectoralSystem = $current;

            fn settings(&self) -> Result<$current::ElectoralSettings, CorruptStorageError> {
                let ($($previous,)* settings, $($remaining,)*) =self.ea.borrow().settings()?;
                Ok(settings)
            }
            fn properties(&self) -> Result<$current::ElectionProperties, CorruptStorageError> {
                match self.ea.borrow().properties()? {
                    CompositeElectionProperties::$current(properties) => {
                        Ok(properties)
                    },
                    _ => Err(CorruptStorageError::new())
                }
            }
            fn state(&self) -> Result<$current::ElectionState, CorruptStorageError> {
                match self.ea.borrow().state()? {
                    CompositeElectionState::$current(state) => {
                        Ok(state)
                    },
                    _ => Err(CorruptStorageError::new())
                }
            }

            #[cfg(test)]
            fn election_identifier(&self) -> Result<ElectionIdentifierOf<Self::ElectoralSystem>, CorruptStorageError> {
                let composite_identifier = self.ea.borrow().election_identifier()?;
                let extra = match composite_identifier.extra() {
                    CompositeElectionIdentifierExtra::$current(extra) => Ok(extra),
                    _ => Err(CorruptStorageError::new()),
                }?;
                Ok(composite_identifier.with_extra(*extra))
            }
        }
        impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static,  EA: ElectionWriteAccess<ElectoralSystem = Composite<($($electoral_system,)*), ValidatorId, H>>> ElectionWriteAccess for CompositeElectionAccess<tags::$current, EA, EA> {
            fn set_state(&mut self, state: $current::ElectionState) -> Result<(), CorruptStorageError> {
                self.ea.set_state(CompositeElectionState::$current(state))
            }
            fn clear_votes(&mut self) {
                self.ea.clear_votes()
            }
            fn delete(self) {
                self.ea.delete();
            }
            fn refresh(
                &mut self,
                extra: $current::ElectionIdentifierExtra,
                properties: $current::ElectionProperties,
            ) -> Result<(), CorruptStorageError> {
                self.ea.refresh(
                    CompositeElectionIdentifierExtra::$current(extra),
                    CompositeElectionProperties::$current(properties),
                )
            }
            fn check_consensus(
                &mut self,
            ) -> Result<ConsensusStatus<$current::Consensus>, CorruptStorageError> {
                self.ea.check_consensus().and_then(|consensus_status| {
                    consensus_status.try_map(|consensus| {
                        match consensus {
                            CompositeConsensus::$current(consensus) => Ok(consensus),
                            _ => Err(CorruptStorageError::new()),
                        }
                    })

                })
            }
        }
        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static, EA: ElectoralReadAccess<ElectoralSystem = Composite<($($electoral_system,)*), ValidatorId, H>>> ElectoralReadAccess for CompositeElectoralAccess<'a, tags::$current, EA> {
            type ElectoralSystem = $current;
            type ElectionReadAccess<'b> = CompositeElectionAccess<tags::$current, <EA as ElectoralReadAccess>::ElectionReadAccess<'b>, <EA as ElectoralReadAccess>::ElectionReadAccess<'b>>
            where
                Self: 'b;

            fn election(
                &self,
                id: ElectionIdentifier<<$current as ElectoralSystem>::ElectionIdentifierExtra>,
            ) -> Result<Self::ElectionReadAccess<'_>, CorruptStorageError> {
                self.ea.election(id.with_extra(CompositeElectionIdentifierExtra::$current(*id.extra()))).map(|election_access| {
                    CompositeElectionAccess::<tags::$current, _, <EA as ElectoralReadAccess>::ElectionReadAccess<'_>>::new(election_access)
                })
            }
            fn unsynchronised_settings(
                &self,
            ) -> Result<$current::ElectoralUnsynchronisedSettings, CorruptStorageError> {
                let ($($previous,)* unsynchronised_settings, $($remaining,)*) = self.ea.unsynchronised_settings()?;
                Ok(unsynchronised_settings)
            }
            fn unsynchronised_state(
                &self,
            ) -> Result<$current::ElectoralUnsynchronisedState, CorruptStorageError> {
                let ($($previous,)* unsynchronised_state, $($remaining,)*) = self.ea.unsynchronised_state()?;
                Ok(unsynchronised_state)
            }
            fn unsynchronised_state_map(
                &self,
                key: &$current::ElectoralUnsynchronisedStateMapKey,
            ) -> Result<Option<$current::ElectoralUnsynchronisedStateMapValue>, CorruptStorageError> {
                match self.ea.unsynchronised_state_map(&CompositeElectoralUnsynchronisedStateMapKey::$current(key.clone()))? {
                    Some(CompositeElectoralUnsynchronisedStateMapValue::$current(value)) => Ok(Some(value)),
                    None => Ok(None),
                    _ => Err(CorruptStorageError::new()),
                }
            }
        }

        impl<'a, $($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static, EA: ElectoralWriteAccess<ElectoralSystem = Composite<($($electoral_system,)*), ValidatorId, H>>> ElectoralWriteAccess for CompositeElectoralAccess<'a, tags::$current, EA> {
            type ElectionWriteAccess<'b> = CompositeElectionAccess<tags::$current, <EA as ElectoralWriteAccess>::ElectionWriteAccess<'b>, <EA as ElectoralWriteAccess>::ElectionWriteAccess<'b>>
            where
                Self: 'b;

            fn new_election(
                &mut self,
                extra: $current::ElectionIdentifierExtra,
                properties: $current::ElectionProperties,
                state: $current::ElectionState,
            ) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError> {
                self.ea.new_election(CompositeElectionIdentifierExtra::$current(extra), CompositeElectionProperties::$current(properties), CompositeElectionState::$current(state)).map(|election_access| {
                    CompositeElectionAccess::new(election_access)
                })
            }
            fn election_mut(
                &mut self,
                id: ElectionIdentifier<$current::ElectionIdentifierExtra>,
            ) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError> {
                self.ea.election_mut(id.with_extra(CompositeElectionIdentifierExtra::$current(*id.extra()))).map(|election_access| {
                    CompositeElectionAccess::new(election_access)
                })
            }
            fn set_unsynchronised_state(
                &mut self,
                unsynchronised_state: $current::ElectoralUnsynchronisedState,
            ) -> Result<(), CorruptStorageError> {
                let ($($previous,)* _, $($remaining,)*) = self.ea.unsynchronised_state()?;
                self.ea.set_unsynchronised_state(($($previous,)* unsynchronised_state, $($remaining,)*))
            }

            fn set_unsynchronised_state_map(
                &mut self,
                key: $current::ElectoralUnsynchronisedStateMapKey,
                value: Option<$current::ElectoralUnsynchronisedStateMapValue>,
            ) -> Result<(), CorruptStorageError> {
                self.ea.set_unsynchronised_state_map(
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
                let ($($previous,)* mut unsynchronised_state, $($remaining,)*) = self.ea.unsynchronised_state()?;
                let t = f(self, &mut unsynchronised_state)?;
                self.ea.set_unsynchronised_state(($($previous,)* unsynchronised_state, $($remaining,)*))?;
                Ok(t)
            }
        }

        impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, H: Hooks<$($electoral_system),*> + 'static, EA: ElectoralWriteAccess<ElectoralSystem = Composite<($($electoral_system,)*), ValidatorId, H>>> Translator<EA> for ElectoralAccessTranslator<tags::$current, Composite<($($electoral_system,)*), ValidatorId, H>> {
            type ElectoralSystem = $current;
            type ElectionAccess<'a> = CompositeElectoralAccess<'a, tags::$current, EA>
            where
                Self: 'a, EA: 'a;

            fn translate_electoral_access<'a>(&'a self, generic_electoral_access: &'a mut EA) -> Self::ElectionAccess<'a> {
                Self::ElectionAccess::<'a>::new(generic_electoral_access)
            }
        }

        generate_electoral_system_tuple_impls!(@ $($previous,)* $current,; $($remaining,)*: $($electoral_system,)*);
    };
}

generate_electoral_system_tuple_impls!(tuple_6_impls: ((A, A0), (B, B0), (C, C0), (D, D0), (EE, E0), (FF, F0)));
