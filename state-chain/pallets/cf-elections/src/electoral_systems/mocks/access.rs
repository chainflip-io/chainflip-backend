use crate::{
	electoral_system::{
		ConsensusStatus, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralReadAccess, ElectoralSystem, ElectoralWriteAccess,
	},
	vote_storage::VoteStorage,
	CorruptStorageError, ElectionIdentifier, UniqueMonotonicIdentifier,
};
use codec::Encode;
use frame_support::{
	ensure, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound, StorageHasher, Twox64Concat,
};
use std::collections::BTreeMap;

pub struct MockReadAccess<'es, ES: ElectoralSystem> {
	election_identifier: ElectionIdentifierOf<ES>,
	electoral_system: &'es MockAccess<ES>,
}
pub struct MockWriteAccess<'es, ES: ElectoralSystem> {
	election_identifier: ElectionIdentifierOf<ES>,
	electoral_system: &'es mut MockAccess<ES>,
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound)]
pub struct MockElection<ES: ElectoralSystem> {
	properties: ES::ElectionProperties,
	state: ES::ElectionState,
	settings: ES::ElectoralSettings,
	consensus_status: ConsensusStatus<ES::Consensus>,
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound)]
pub struct MockAccess<ES: ElectoralSystem> {
	electoral_settings: ES::ElectoralSettings,
	unsynchronised_state: ES::ElectoralUnsynchronisedState,
	unsynchronised_state_map: BTreeMap<Vec<u8>, Option<ES::ElectoralUnsynchronisedStateMapValue>>,
	elections: BTreeMap<ElectionIdentifierOf<ES>, MockElection<ES>>,
	unsynchronised_settings: ES::ElectoralUnsynchronisedSettings,
	next_election_id: UniqueMonotonicIdentifier,
}

impl<ES: ElectoralSystem> MockAccess<ES> {
	fn election_read_access(
		&self,
		id: ElectionIdentifierOf<ES>,
	) -> Result<MockReadAccess<'_, ES>, CorruptStorageError> {
		ensure!(self.elections.contains_key(&id), CorruptStorageError::new());
		Ok(MockReadAccess { election_identifier: id, electoral_system: self })
	}

	fn election_write_access(
		&mut self,
		id: ElectionIdentifierOf<ES>,
	) -> Result<MockWriteAccess<'_, ES>, CorruptStorageError> {
		ensure!(self.elections.contains_key(&id), CorruptStorageError::new());
		Ok(MockWriteAccess { election_identifier: id, electoral_system: self })
	}
}

macro_rules! impl_read_access {
	( $t:ty ) => {
		impl<ES: ElectoralSystem> ElectionReadAccess for $t {
			type ElectoralSystem = ES;

			fn settings(
				&self,
			) -> Result<
				<Self::ElectoralSystem as ElectoralSystem>::ElectoralSettings,
				CorruptStorageError,
			> {
				self.with_election(|e| e.settings.clone())
			}

			fn properties(
				&self,
			) -> Result<
				<Self::ElectoralSystem as ElectoralSystem>::ElectionProperties,
				CorruptStorageError,
			> {
				self.with_election(|e| e.properties.clone())
			}

			fn state(
				&self,
			) -> Result<
				<Self::ElectoralSystem as ElectoralSystem>::ElectionState,
				CorruptStorageError,
			> {
				self.with_election(|e| e.state.clone())
			}

			fn election_identifier(
				&self,
			) -> Result<ElectionIdentifierOf<Self::ElectoralSystem>, CorruptStorageError> {
				Ok(self.identifier())
			}
		}

		impl<ES: ElectoralSystem> $t {
			fn with_election<F: FnOnce(&MockElection<ES>) -> R, R>(
				&self,
				f: F,
			) -> Result<R, CorruptStorageError> {
				self.electoral_system
					.elections
					.get(&self.identifier())
					.map(f)
					.ok_or_else(CorruptStorageError::new)
			}
			pub fn identifier(&self) -> ElectionIdentifierOf<ES> {
				self.election_identifier
			}
			pub fn check_consensus(
				&self,
				previous_consensus: Option<&ES::Consensus>,
				votes: ConsensusVotes<ES>,
			) -> Result<Option<ES::Consensus>, CorruptStorageError> {
				ES::check_consensus(self.identifier(), self, previous_consensus, votes)
			}
		}
	};
}

impl_read_access!(MockReadAccess<'_, ES>);
impl_read_access!(MockWriteAccess<'_, ES>);

impl<ES: ElectoralSystem> MockWriteAccess<'_, ES> {
	fn with_election_mut<F: FnOnce(&mut MockElection<ES>) -> R, R>(
		&mut self,
		f: F,
	) -> Result<R, CorruptStorageError> {
		self.electoral_system
			.elections
			.get_mut(&self.identifier())
			.map(f)
			.ok_or_else(CorruptStorageError::new)
	}
	pub fn set_consensus_status(&mut self, consensus_status: ConsensusStatus<ES::Consensus>) {
		self.with_election_mut(|e| e.consensus_status = consensus_status)
			.expect("Cannot set consensus status for non-existent election");
	}
}

impl<ES: ElectoralSystem> ElectionWriteAccess for MockWriteAccess<'_, ES> {
	fn set_state(
		&mut self,
		state: <Self::ElectoralSystem as ElectoralSystem>::ElectionState,
	) -> Result<(), CorruptStorageError> {
		self.with_election_mut(|e| e.state = state)?;
		Ok(())
	}
	fn clear_votes(&mut self) {
		// nothing
	}
	fn delete(self) {
		self.electoral_system.elections.remove(&self.identifier());
	}
	fn refresh(
		&mut self,
		_extra: <Self::ElectoralSystem as ElectoralSystem>::ElectionIdentifierExtra,
		properties: <Self::ElectoralSystem as ElectoralSystem>::ElectionProperties,
	) -> Result<(), CorruptStorageError> {
		self.with_election_mut(|e| e.properties = properties)?;
		Ok(())
	}

	fn check_consensus(
		&mut self,
	) -> Result<
		ConsensusStatus<<Self::ElectoralSystem as ElectoralSystem>::Consensus>,
		CorruptStorageError,
	> {
		self.with_election_mut(|e| e.consensus_status.clone())
	}
}

impl<ES: ElectoralSystem> MockAccess<ES> {
	pub fn new(
		unsynchronised_state: ES::ElectoralUnsynchronisedState,
		unsynchronised_settings: ES::ElectoralUnsynchronisedSettings,
		electoral_settings: ES::ElectoralSettings,
	) -> Self {
		Self {
			electoral_settings,
			unsynchronised_state,
			unsynchronised_settings,
			unsynchronised_state_map: Default::default(),
			elections: Default::default(),
			next_election_id: Default::default(),
		}
	}

	pub fn finalize_elections(
		&mut self,
		context: &ES::OnFinalizeContext,
	) -> Result<ES::OnFinalizeReturn, CorruptStorageError> {
		ES::on_finalize(self, self.elections.keys().cloned().collect(), context)
	}

	pub fn election_identifiers(&self) -> Vec<ElectionIdentifierOf<ES>> {
		self.elections.keys().cloned().collect()
	}

	pub fn next_umi(&self) -> UniqueMonotonicIdentifier {
		self.next_election_id
	}

	pub fn is_vote_valid(
		&self,
		election_identifier: ElectionIdentifierOf<ES>,
		partial_vote: &<ES::Vote as VoteStorage>::PartialVote,
	) -> Result<bool, CorruptStorageError> {
		ES::is_vote_valid(election_identifier, &self.election(election_identifier)?, partial_vote)
	}
}

impl<ES: ElectoralSystem> ElectoralReadAccess for MockAccess<ES> {
	type ElectoralSystem = ES;
	type ElectionReadAccess<'es> = MockReadAccess<'es, ES>;

	fn election(
		&self,
		id: ElectionIdentifierOf<Self::ElectoralSystem>,
	) -> Result<Self::ElectionReadAccess<'_>, CorruptStorageError> {
		self.election_read_access(id)
	}
	fn unsynchronised_settings(
		&self,
	) -> Result<
		<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedSettings,
		CorruptStorageError,
	> {
		Ok(self.unsynchronised_settings.clone())
	}
	fn unsynchronised_state(
		&self,
	) -> Result<
		<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedState,
		CorruptStorageError,
	> {
		Ok(self.unsynchronised_state.clone())
	}
	fn unsynchronised_state_map(
		&self,
		key: &<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedStateMapKey,
	) -> Result<
		Option<<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedStateMapValue>,
		CorruptStorageError,
	> {
		self.unsynchronised_state_map
			.get(&key.using_encoded(Twox64Concat::hash))
			.ok_or_else(CorruptStorageError::new)
			.cloned()
	}
}

impl<ES: ElectoralSystem> ElectoralWriteAccess for MockAccess<ES> {
	type ElectionWriteAccess<'a> = MockWriteAccess<'a, ES>;

	fn new_election(
		&mut self,
		extra: <Self::ElectoralSystem as ElectoralSystem>::ElectionIdentifierExtra,
		properties: <Self::ElectoralSystem as ElectoralSystem>::ElectionProperties,
		state: <Self::ElectoralSystem as ElectoralSystem>::ElectionState,
	) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError> {
		let election_identifier = ElectionIdentifier::new(self.next_election_id, extra);
		self.next_election_id = self.next_election_id.next_identifier().unwrap();
		self.elections.insert(
			election_identifier,
			MockElection {
				properties,
				state,
				settings: self.electoral_settings.clone(),
				consensus_status: ConsensusStatus::None,
			},
		);
		self.election_write_access(election_identifier)
	}
	fn election_mut(
		&mut self,
		id: ElectionIdentifierOf<Self::ElectoralSystem>,
	) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError> {
		self.election_write_access(id)
	}
	fn set_unsynchronised_state(
		&mut self,
		unsynchronised_state: <Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedState,
	) -> Result<(), CorruptStorageError> {
		self.unsynchronised_state = unsynchronised_state;
		Ok(())
	}

	/// Inserts or removes a value from the unsynchronised state map of the electoral system.
	fn set_unsynchronised_state_map(
		&mut self,
		key: <Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedStateMapKey,
		value: Option<
			<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedStateMapValue,
		>,
	) -> Result<(), CorruptStorageError> {
		self.unsynchronised_state_map
			.insert(key.using_encoded(Twox64Concat::hash), value);
		Ok(())
	}
}
