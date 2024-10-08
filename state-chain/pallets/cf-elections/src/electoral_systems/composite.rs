use crate::{
	electoral_system::{ElectoralSystem, ElectoralWriteAccess},
	electoral_system_runner::RunnerStorageAccessTrait,
};

/// Allows the composition of multiple ElectoralSystems while allowing the ability to configure the
/// `on_finalize` behaviour without exposing the internal composite types.
pub struct CompositeRunner<T, ValidatorId, StorageAccess, H = DefaultHooks<(), StorageAccess>> {
	storage_access: StorageAccess,
	_phantom: core::marker::PhantomData<(T, ValidatorId, H)>,
}

pub struct DefaultHooks<OnFinalizeContext, StorageAccess> {
	_phantom: core::marker::PhantomData<(OnFinalizeContext, StorageAccess)>,
}

/// Takes a generic storage access type and then can translate into an election access type for a
/// specific electoral system.
pub trait Translator {
	type StorageAccess: RunnerStorageAccessTrait;
	type ElectoralSystem: ElectoralSystem;
	type ElectionAccess<'a>: ElectoralWriteAccess<ElectoralSystem = Self::ElectoralSystem>
	where
		Self: 'a;

	fn translate_electoral_access<'a>(
		&'a self,
		storage_access: &'a mut Self::StorageAccess,
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
}

macro_rules! generate_electoral_system_tuple_impls {
    ($module:ident: ($(($electoral_system:ident, $electoral_system_alt_name_0:ident)),*$(,)?)) => {
        #[allow(dead_code)]
        // We use the type names as variable names.
        #[allow(non_snake_case)]
        // In the 1/identity case, no invalid combinations are possible, so error cases are unreachable.
        #[allow(unreachable_patterns)]
        // Macro expands tuples, but only uses 1 element in some cases.
        #[allow(unused_variables)]
        pub mod $module {
            use super::{
                Translator,
                CompositeRunner,
                DefaultHooks,
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
                    VotePropertiesOf, CompositeConsensusVotes, CompositeConsensusStatus, CompositeElectionIdentifierOf, CompositeConsensusVote},
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

                type StorageAccess: RunnerStorageAccessTrait;

                // we could pass in another generic, and have the translator take the storage, but why?
                fn on_finalize<$($electoral_system_alt_name_0: Translator<StorageAccess = Self::StorageAccess, ElectoralSystem = $electoral_system>),*>(
                    // What do we actually need this for?
                    generic_electoral_access: &mut Self::StorageAccess,
                    electoral_access_translators: ($($electoral_system_alt_name_0,)*),
                    election_identifiers: ($(Vec<ElectionIdentifierOf<$electoral_system>>,)*),
                ) -> Result<(), CorruptStorageError>;
            }

            impl<$($electoral_system: ElectoralSystem<OnFinalizeContext = ()>,)* StorageAccess: RunnerStorageAccessTrait> Hooks<$($electoral_system,)*> for DefaultHooks<(), StorageAccess> {

                type StorageAccess = StorageAccess;

                fn on_finalize<$($electoral_system_alt_name_0: Translator<StorageAccess = Self::StorageAccess, ElectoralSystem = $electoral_system>),*>(
                    generic_electoral_access: &mut Self::StorageAccess,
                    electoral_access_translators: ($($electoral_system_alt_name_0,)*),
                    election_identifiers: ($(Vec<ElectionIdentifier<$electoral_system::ElectionIdentifierExtra>>,)*),
                ) -> Result<(), CorruptStorageError> {
                    let ($($electoral_system,)*) = electoral_access_translators;
                    let ($($electoral_system_alt_name_0,)*) = election_identifiers;

                    $(
                        $electoral_system::on_finalize(&mut $electoral_system.translate_electoral_access(generic_electoral_access), $electoral_system_alt_name_0, &())?;
                    )*

                    Ok(())
                }
            }

            #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
            pub enum CompositeElectoralUnsynchronisedStateMapKey<$($electoral_system,)*> {
                // struct A is used here  in the wrapped version
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

            // We need to pass a storage access here.
            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StorageAccess: RunnerStorageAccessTrait + 'static, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static> CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H> {
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

            impl<$($electoral_system: ElectoralSystem<ValidatorId = ValidatorId>,)* ValidatorId: MaybeSerializeDeserialize + Parameter + Member, StorageAccess: RunnerStorageAccessTrait + 'static, H: Hooks<$($electoral_system),*, StorageAccess = StorageAccess> + 'static> ElectoralSystemRunner for CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, H> {
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

                fn is_vote_desired<SA: RunnerStorageAccessTrait>(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    storage_access: &SA,
                    current_vote: Option<(
                        VotePropertiesOf<Self>,
                        AuthorityVoteOf<Self>,
                    )>,
                ) -> Result<bool, CorruptStorageError> {
                    match *election_identifier.extra() {
                        $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                            <$electoral_system as ElectoralSystem>::is_vote_desired(
                                election_identifier.with_extra(extra),
                                &CompositeElectionAccess::<tags::$electoral_system, $electoral_system,  _, SA>::new(storage_access),
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


                fn is_vote_valid<ElectionAccess: RunnerStorageAccessTrait>(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    election_access: &ElectionAccess,
                    partial_vote: &<Self::Vote as VoteStorage>::PartialVote,
                ) -> Result<bool, CorruptStorageError> {
                    Ok(match (*election_identifier.extra(), partial_vote) {
                        $((CompositeElectionIdentifierExtra::$electoral_system(extra), CompositePartialVote::$electoral_system(partial_vote)) => <$electoral_system as ElectoralSystem>::is_vote_valid(
                            election_identifier.with_extra(extra),
                            &CompositeElectionAccess::<tags::$electoral_system, $electoral_system, _, ElectionAccess>::new(election_access),
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
                    // We call this *on* the CompositeRunner, and so Self is the CompositerRunner, but the translators are on the Storage.
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

                // TODO:
                fn check_consensus<SA: RunnerStorageAccessTrait>(
                    election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
                    election_access: &SA,
                    previous_consensus: Option<&Self::Consensus>,
                    consensus_votes: CompositeConsensusVotes<Self>,
                ) -> Result<Option<Self::Consensus>, CorruptStorageError> {
                    Ok(match *election_identifier.extra() {
                        $(CompositeElectionIdentifierExtra::$electoral_system(extra) => {
                            <$electoral_system as ElectoralSystem>::check_consensus(
                                election_identifier.with_extra(extra),
                                &CompositeElectionAccess::<tags::$electoral_system, _, _, SA>::new(election_access),
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

            // The election accessors, access the runner
            pub struct CompositeElectionAccess<Tag, ES, BorrowRunner, Runner> {
                // TOOD: Look at simplifying this
                runner: BorrowRunner,
                _phantom: core::marker::PhantomData<(Tag, ES, Runner)>,
            }

            // We want something that has read access to the composite. This is the runner.
            // This doesn't have to take a Read access, it just has to take something that can be translated into a specific acccessor - we effectively want a ReadAccess where the electoral system is actually a runner instead. The runner *is* a composite.
            // TODO: Rename
            // The runner should contain them in some way, but not as an access as it was done before.
            impl<Tag, ES: ElectoralSystem, BorrowRunner: Borrow<Runner>, Runner: RunnerStorageAccessTrait> CompositeElectionAccess<Tag, ES, BorrowRunner, Runner> {
                fn new(runner: BorrowRunner) -> Self {
                    Self {
                        runner,
                        _phantom: Default::default(),
                    }
                }
            }
            pub struct CompositeElectoralAccess<'a, Tag, ES, Runner> {
                runner: &'a mut Runner,
                _phantom: core::marker::PhantomData<(Tag, ES)>,
            }
            impl<'a, Tag, ES, Runner> CompositeElectoralAccess<'a, Tag, ES, Runner> {
                fn new(runner: &'a mut Runner) -> Self {
                    Self {
                        runner,
                        _phantom: Default::default(),
                    }
                }
            }

            // we need to store a storage accessor in the translator too? Why can't the runner do it? the runner is currently the CompositeRunner, but maybe it should
            // be something else??? - how do we pass that into here?
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

        // This is where the undeclared is coming from? We use a generic that is not initialised.
        impl<$current: ElectoralSystem, BorrowRunner: Borrow<Runner>, Runner: RunnerStorageAccessTrait> ElectionReadAccess for CompositeElectionAccess<tags::$current, $current, BorrowRunner, Runner> {
            type ElectoralSystem = $current;

            fn settings(&self) -> Result<$current::ElectoralSettings, CorruptStorageError> {


                let ($($previous,)* settings, $($remaining,)*) =self.runner.borrow().settings()?;
                Ok(settings)
            }
            fn properties(&self) -> Result<$current::ElectionProperties, CorruptStorageError> {
                todo!()
                // match self.runner.borrow().properties()? {
                //     CompositeElectionProperties::$current(properties) => {
                //         Ok(properties)
                //     },
                //     _ => Err(CorruptStorageError::new())
                // }
            }
            fn state(&self) -> Result<$current::ElectionState, CorruptStorageError> {
                todo!()
                // match self.runner.borrow().state()? {
                //     CompositeElectionState::$current(state) => {
                //         Ok(state)
                //     },
                //     _ => Err(CorruptStorageError::new())
                // }
            }
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

        impl<$current: ElectoralSystem, Runner: RunnerStorageAccessTrait> ElectionWriteAccess for CompositeElectionAccess<tags::$current, $current, Runner, Runner> {
            fn set_state(&mut self, state: $current::ElectionState) -> Result<(), CorruptStorageError> {
                // self.runner.set_state(CompositeElectionState::$current(state))
                todo!()
            }
            fn clear_votes(&mut self) {
                // self.runner.clear_votes()
            }
            fn delete(self) {
                // self.runner.delete();
            }
            fn refresh(
                &mut self,
                extra: $current::ElectionIdentifierExtra,
                properties: $current::ElectionProperties,
            ) -> Result<(), CorruptStorageError> {
                // self.runner.refresh(
                //     CompositeElectionIdentifierExtra::$current(extra),
                //     CompositeElectionProperties::$current(properties),
                // )
                todo!()
            }
            fn check_consensus(
                &mut self,
            ) -> Result<ConsensusStatus<$current::Consensus>, CorruptStorageError> {
                // self.runner.check_consensus().and_then(|consensus_status| {
                //     consensus_status.try_map(|consensus| {
                //         match consensus {
                //             CompositeConsensus::$current(consensus) => Ok(consensus),
                //             _ => Err(CorruptStorageError::new()),
                //         }
                //     })

                // })
                todo!()
            }
        }

        // TODO: Rename to ElectoralReadAccessForSpecificES or something similar...
        impl<'a, $current: ElectoralSystem, Runner: RunnerStorageAccessTrait> ElectoralReadAccess for CompositeElectoralAccess<'a, tags::$current, $current, Runner> {
            type ElectoralSystem = $current;
            type ElectionReadAccess<'b> = CompositeElectionAccess<tags::$current, $current, Runner, Runner>
            where
                Self: 'b;

            fn election(
                &self,
                id: ElectionIdentifier<<$current as ElectoralSystem>::ElectionIdentifierExtra>,
            ) -> Result<Self::ElectionReadAccess<'_>, CorruptStorageError> {
                // we need to generate the election generator somehow
                // self.runner.election(id.with_extra(CompositeElectionIdentifierExtra::$current(*id.extra()))).map(|election_access| {
                //     CompositeElectionAccess::<tags::$current, _, Runner>::new(election_access)
                // })
                todo!()
            }
            fn unsynchronised_settings(
                &self,
            ) -> Result<$current::ElectoralUnsynchronisedSettings, CorruptStorageError> {
                // let ($($previous,)* unsynchronised_settings, $($remaining,)*) = self.runner.unsynchronised_settings()?;
                // Ok(unsynchronised_settings)
                todo!()
            }
            fn unsynchronised_state(
                &self,
            ) -> Result<$current::ElectoralUnsynchronisedState, CorruptStorageError> {
                todo!()
                // let ($($previous,)* unsynchronised_state, $($remaining,)*) = self.runner.unsynchronised_state()?;
                // Ok(unsynchronised_state)
            }
            fn unsynchronised_state_map(
                &self,
                key: &$current::ElectoralUnsynchronisedStateMapKey,
            ) -> Result<Option<$current::ElectoralUnsynchronisedStateMapValue>, CorruptStorageError> {
                todo!()
                // match self.runner.unsynchronised_state_map(&CompositeElectoralUnsynchronisedStateMapKey::$current(key.clone()))? {
                //     Some(CompositeElectoralUnsynchronisedStateMapValue::$current(value)) => Ok(Some(value)),
                //     None => Ok(None),
                //     _ => Err(CorruptStorageError::new()),
                // }
            }
        }

        // TODO: Check if we even need the tags after this?
        impl<'a, $current: ElectoralSystem, Runner: RunnerStorageAccessTrait> ElectoralWriteAccess for CompositeElectoralAccess<'a, tags::$current, $current, Runner> {
            type ElectionWriteAccess<'b> = CompositeElectionAccess<tags::$current, $current, Runner, Runner>
            where
                Self: 'b;

            fn new_election(
                &mut self,
                extra: $current::ElectionIdentifierExtra,
                properties: $current::ElectionProperties,
                state: $current::ElectionState,
            ) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError> {
                // self.runner.new_election(CompositeElectionIdentifierExtra::$current(extra), CompositeElectionProperties::$current(properties), CompositeElectionState::$current(state)).map(|election_access| {
                //     CompositeElectionAccess::new(election_access)
                // })
                todo!()
            }
            fn election_mut(
                &mut self,
                id: ElectionIdentifier<$current::ElectionIdentifierExtra>,
            ) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError> {
                // self.runner.election_mut(id.with_extra(CompositeElectionIdentifierExtra::$current(*id.extra()))).map(|election_access| {
                //     CompositeElectionAccess::new(election_access)
                // })
                todo!()
            }
            fn set_unsynchronised_state(
                &mut self,
                unsynchronised_state: $current::ElectoralUnsynchronisedState,
            ) -> Result<(), CorruptStorageError> {
                // let ($($previous,)* _, $($remaining,)*) = self.runner.unsynchronised_state()?;
                // self.runner.set_unsynchronised_state(($($previous,)* unsynchronised_state, $($remaining,)*))
                todo!()
            }

            fn set_unsynchronised_state_map(
                &mut self,
                key: $current::ElectoralUnsynchronisedStateMapKey,
                value: Option<$current::ElectoralUnsynchronisedStateMapValue>,
            ) -> Result<(), CorruptStorageError> {
                // self.runner.set_unsynchronised_state_map(
                //     CompositeElectoralUnsynchronisedStateMapKey::$current(key),
                //     value.map(CompositeElectoralUnsynchronisedStateMapValue::$current),
                // )
                todo!()
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
                // let ($($previous,)* mut unsynchronised_state, $($remaining,)*) = self.runner.unsynchronised_state()?;
                // let t = f(self, &mut unsynchronised_state)?;
                // self.runner.set_unsynchronised_state(($($previous,)* unsynchronised_state, $($remaining,)*))?;
                // Ok(t)
                todo!()
            }
        }

        // We don't need a runner, we need a Storage access layer i.e. something that impls this trait. - should rename that here and above.
        impl<$current: ElectoralSystem, StorageAccess: RunnerStorageAccessTrait> Translator for ElectoralAccessTranslator<tags::$current, $current, StorageAccess> {

            type StorageAccess = StorageAccess;
            type ElectoralSystem = $current;
            type ElectionAccess<'a> = CompositeElectoralAccess<'a, tags::$current, $current, Self::StorageAccess>
            where
                Self: 'a;

            // What do we actually need to be able to t
            fn translate_electoral_access<'a>(&'a self, storage_access: &'a mut Self::StorageAccess) -> Self::ElectionAccess<'a> {
                Self::ElectionAccess::<'a>::new(storage_access)
            }
        }

        generate_electoral_system_tuple_impls!(@ $($previous,)* $current,; $($remaining,)*: $($electoral_system,)*);
    };
}

generate_electoral_system_tuple_impls!(tuple_5_impls: ((A, A0), (B, B0), (C, C0), (D, D0), (EE, E0)));
