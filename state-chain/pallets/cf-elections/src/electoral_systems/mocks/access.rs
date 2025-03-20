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

use crate::{
	electoral_system::{
		ConsensusStatus, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralReadAccess, ElectoralSystem, ElectoralSystemTypes,
		ElectoralWriteAccess,
	},
	CorruptStorageError, ElectionIdentifier, UniqueMonotonicIdentifier,
};
use codec::{Decode, Encode};
use core::{cell::RefCell, cmp::Ord};
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};
use std::collections::BTreeMap;

pub struct MockReadAccess<ES: ElectoralSystem> {
	election_identifier: ElectionIdentifierOf<ES>,
}

pub struct MockWriteAccess<ES: ElectoralSystem> {
	election_identifier: ElectionIdentifierOf<ES>,
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound)]
pub struct MockAccess<ES: ElectoralSystem> {
	_phantom: core::marker::PhantomData<ES>,
}

macro_rules! impl_read_access {
	( $t:ty ) => {
		impl<ES: ElectoralSystem> ElectionReadAccess for $t {
			type ElectoralSystem = ES;

			fn settings(
				&self,
			) -> Result<
				<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralSettings,
				CorruptStorageError,
			> {
				Ok(MockStorageAccess::electoral_settings_for_election::<ES>(self.identifier()))
			}

			fn properties(
				&self,
			) -> Result<
				<Self::ElectoralSystem as ElectoralSystemTypes>::ElectionProperties,
				CorruptStorageError,
			> {
				Ok(MockStorageAccess::election_properties::<ES>(self.identifier()))
			}

			fn state(
				&self,
			) -> Result<
				<Self::ElectoralSystem as ElectoralSystemTypes>::ElectionState,
				CorruptStorageError,
			> {
				Ok(MockStorageAccess::election_state::<ES>(self.identifier()))
			}

			fn election_identifier(&self) -> ElectionIdentifierOf<Self::ElectoralSystem> {
				self.identifier()
			}
		}

		impl<ES: ElectoralSystem> $t {
			pub fn identifier(&self) -> ElectionIdentifierOf<ES> {
				self.election_identifier
			}
			pub fn check_consensus(
				&self,
				previous_consensus: Option<&ES::Consensus>,
				votes: ConsensusVotes<ES>,
			) -> Result<Option<ES::Consensus>, CorruptStorageError> {
				ES::check_consensus(self, previous_consensus, votes)
			}
		}
	};
}

impl_read_access!(MockReadAccess<ES>);
impl_read_access!(MockWriteAccess<ES>);

thread_local! {
	pub static ELECTION_STATE: RefCell<BTreeMap<Vec<u8>, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
	pub static ELECTION_PROPERTIES: RefCell<BTreeMap<Vec<u8>, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
	pub static ELECTORAL_SETTINGS: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
	// The electoral settings for a particular election
	pub static ELECTION_SETTINGS: RefCell<BTreeMap<Vec<u8>, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
	pub static ELECTORAL_UNSYNCHRONISED_SETTINGS: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
	pub static ELECTORAL_UNSYNCHRONISED_STATE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
	pub static ELECTORAL_UNSYNCHRONISED_STATE_MAP: RefCell<BTreeMap<Vec<u8>, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
	pub static CONSENSUS_STATUS: RefCell<BTreeMap<Vec<u8>, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
	pub static NEXT_ELECTION_ID: RefCell<UniqueMonotonicIdentifier> = const { RefCell::new(UniqueMonotonicIdentifier::from_u64(0)) };
}

impl<ES: ElectoralSystem> ElectionWriteAccess for MockWriteAccess<ES> {
	fn set_state(
		&self,
		state: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionState,
	) -> Result<(), CorruptStorageError> {
		MockStorageAccess::set_state::<ES>(self.identifier(), state);
		Ok(())
	}
	fn clear_votes(&self) {
		// nothing
	}
	fn delete(self) {
		MockStorageAccess::delete_election::<ES>(self.identifier());
	}
	fn refresh(
		&mut self,
		new_extra: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionIdentifierExtra,
		properties: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<(), CorruptStorageError> {
		// Remove the old properties and replace it with the new one.
		MockStorageAccess::set_election_properties::<ES>(self.identifier(), None);

		self.election_identifier = self.election_identifier.with_extra(new_extra);

		MockStorageAccess::set_election_properties::<ES>(self.identifier(), Some(properties));
		Ok(())
	}

	fn check_consensus(
		&self,
	) -> Result<
		ConsensusStatus<<Self::ElectoralSystem as ElectoralSystemTypes>::Consensus>,
		CorruptStorageError,
	> {
		Ok(MockStorageAccess::consensus_status::<ES>(self.identifier()))
	}
}

impl<ES: ElectoralSystem> ElectoralReadAccess for MockAccess<ES> {
	type ElectoralSystem = ES;
	type ElectionReadAccess = MockReadAccess<ES>;

	fn election(id: ElectionIdentifierOf<Self::ElectoralSystem>) -> Self::ElectionReadAccess {
		MockReadAccess { election_identifier: id }
	}
	fn unsynchronised_settings() -> Result<
		<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		CorruptStorageError,
	> {
		Ok(MockStorageAccess::unsynchronised_settings::<ES>())
	}
	fn unsynchronised_state() -> Result<
		<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		CorruptStorageError,
	> {
		Ok(MockStorageAccess::unsynchronised_state::<ES>())
	}
	fn unsynchronised_state_map(
		key: &<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
	) -> Result<
		Option<
			<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
		>,
		CorruptStorageError,
	> {
		Ok(MockStorageAccess::unsynchronised_state_map::<ES>(key))
	}
}

impl<ES: ElectoralSystem> ElectoralWriteAccess for MockAccess<ES> {
	type ElectionWriteAccess = MockWriteAccess<ES>;

	fn new_election(
		extra: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionIdentifierExtra,
		properties: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionProperties,
		state: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionState,
	) -> Result<Self::ElectionWriteAccess, CorruptStorageError> {
		Ok(Self::election_mut(MockStorageAccess::new_election::<ES>(extra, properties, state)))
	}
	fn election_mut(id: ElectionIdentifierOf<Self::ElectoralSystem>) -> Self::ElectionWriteAccess {
		MockWriteAccess { election_identifier: id }
	}
	fn set_unsynchronised_state(
		unsynchronised_state: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
	) -> Result<(), CorruptStorageError> {
		MockStorageAccess::set_unsynchronised_state::<ES>(unsynchronised_state);
		Ok(())
	}

	/// Inserts or removes a value from the unsynchronised state map of the electoral system.
	fn set_unsynchronised_state_map(
		key: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
		value: Option<
			<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
		>,
	) {
		MockStorageAccess::set_unsynchronised_state_map::<ES>(key, value);
	}
}

pub struct MockStorageAccess;

impl MockStorageAccess {
	pub fn clear_storage() {
		ELECTION_STATE.with(|state| {
			let mut state_ref = state.borrow_mut();
			state_ref.clear();
		});
		ELECTION_PROPERTIES.with(|properties| {
			let mut properties_ref = properties.borrow_mut();
			properties_ref.clear();
		});
		ELECTORAL_SETTINGS.with(|settings| {
			let mut settings_ref = settings.borrow_mut();
			settings_ref.clear();
		});
		ELECTION_SETTINGS.with(|settings| {
			let mut settings_ref = settings.borrow_mut();
			settings_ref.clear();
		});
		ELECTORAL_UNSYNCHRONISED_SETTINGS.with(|settings| {
			let mut settings_ref = settings.borrow_mut();
			settings_ref.clear();
		});
		ELECTORAL_UNSYNCHRONISED_STATE.with(|state| {
			let mut state_ref = state.borrow_mut();
			state_ref.clear();
		});
		ELECTORAL_UNSYNCHRONISED_STATE_MAP.with(|state_map| {
			let mut state_map_ref = state_map.borrow_mut();
			state_map_ref.clear();
		});
		CONSENSUS_STATUS.with(|consensus| {
			let mut consensus_ref = consensus.borrow_mut();
			consensus_ref.clear();
		});
		NEXT_ELECTION_ID.with(|next_id| {
			let mut next_id_ref = next_id.borrow_mut();
			*next_id_ref = UniqueMonotonicIdentifier::from_u64(0);
		});
	}

	pub fn next_umi() -> UniqueMonotonicIdentifier {
		NEXT_ELECTION_ID.with(|next_id| *next_id.borrow())
	}

	pub fn increment_next_umi() {
		NEXT_ELECTION_ID.with(|next_id| {
			let mut next_id_ref = next_id.borrow_mut();
			*next_id_ref = next_id_ref.next_identifier().unwrap();
		});
	}

	pub fn set_electoral_settings<ES: ElectoralSystem>(
		settings: <ES as ElectoralSystemTypes>::ElectoralSettings,
	) {
		ELECTORAL_SETTINGS.with(|old_settings| {
			let mut settings_ref = old_settings.borrow_mut();
			*settings_ref = settings.encode();
		});
	}

	pub fn electoral_settings<ES: ElectoralSystem>() -> ES::ElectoralSettings {
		ELECTORAL_SETTINGS.with(|settings| {
			let settings_ref = settings.borrow();
			ES::ElectoralSettings::decode(&mut &settings_ref[..]).unwrap()
		})
	}

	pub fn set_electoral_settings_for_election<ES: ElectoralSystem>(
		identifier: ElectionIdentifierOf<ES>,
		settings: <ES as ElectoralSystemTypes>::ElectoralSettings,
	) {
		ELECTION_SETTINGS.with(|old_settings| {
			let mut settings_ref = old_settings.borrow_mut();
			settings_ref.insert(identifier.unique_monotonic().encode(), settings.encode());
		});
	}

	pub fn electoral_settings_for_election<ES: ElectoralSystem>(
		identifier: ElectionIdentifierOf<ES>,
	) -> <ES as ElectoralSystemTypes>::ElectoralSettings {
		ELECTION_SETTINGS.with(|settings| {
			let settings_ref = settings.borrow();
			settings_ref
				.get(&identifier.unique_monotonic().encode())
				.map(|v| ES::ElectoralSettings::decode(&mut &v[..]).unwrap())
				.unwrap()
		})
	}

	pub fn delete_election<ES: ElectoralSystem>(identifier: ElectionIdentifierOf<ES>) {
		ELECTION_PROPERTIES.with(|properties| {
			let mut properties_ref = properties.borrow_mut();
			properties_ref.remove(&identifier.encode());
		});
		ELECTION_STATE.with(|state| {
			let mut state_ref = state.borrow_mut();
			state_ref.remove(&identifier.unique_monotonic().encode());
		});
	}

	pub fn set_state<ES: ElectoralSystem>(
		identifier: ElectionIdentifierOf<ES>,
		state: ES::ElectionState,
	) {
		println!("Setting election state for identifier: {:?}", identifier);
		ELECTION_STATE.with(|old_state| {
			let mut state_ref = old_state.borrow_mut();
			state_ref.insert(identifier.unique_monotonic().encode(), state.encode());
		});
	}

	pub fn election_state<ES: ElectoralSystem>(
		identifier: ElectionIdentifierOf<ES>,
	) -> ES::ElectionState {
		ELECTION_STATE.with(|old_state| {
			let state_ref = old_state.borrow();
			state_ref
				.get(&identifier.unique_monotonic().encode())
				.map(|v| ES::ElectionState::decode(&mut &v[..]).unwrap())
				.unwrap()
		})
	}

	pub fn election_state_all<ES: ElectoralSystem>(
	) -> BTreeMap<UniqueMonotonicIdentifier, ES::ElectionState> {
		ELECTION_STATE.with(|old_state| {
			let state_ref = old_state.borrow();
			state_ref
				.clone()
				.into_iter()
				.map(|(k, v)| {
					(
						UniqueMonotonicIdentifier::decode(&mut &k[..]).unwrap(),
						ES::ElectionState::decode(&mut &v[..]).unwrap(),
					)
				})
				.collect::<BTreeMap<_, _>>()
		})
	}

	pub fn set_election_properties<ES: ElectoralSystem>(
		identifier: ElectionIdentifierOf<ES>,
		properties: Option<ES::ElectionProperties>,
	) {
		ELECTION_PROPERTIES.with(|old_properties| {
			let mut properties_ref = old_properties.borrow_mut();
			match properties {
				Some(properties) => properties_ref.insert(identifier.encode(), properties.encode()),
				None => properties_ref.remove(&identifier.encode()),
			};
		});
	}

	pub fn election_properties<ES: ElectoralSystem>(
		identifier: ElectionIdentifierOf<ES>,
	) -> ES::ElectionProperties {
		ELECTION_PROPERTIES.with(|old_properties| {
			let properties_ref = old_properties.borrow();
			properties_ref
				.get(&identifier.encode())
				.map(|v| ES::ElectionProperties::decode(&mut &v[..]).unwrap())
				.unwrap()
		})
	}

	pub fn set_unsynchronised_state<ES: ElectoralSystem>(
		unsynchronised_state: ES::ElectoralUnsynchronisedState,
	) {
		ELECTORAL_UNSYNCHRONISED_STATE.with(|old_state| {
			let mut state_ref = old_state.borrow_mut();
			state_ref.clear();
			state_ref.extend_from_slice(&unsynchronised_state.encode());
		});
	}

	pub fn set_unsynchronised_settings<ES: ElectoralSystem>(
		unsynchronised_settings: ES::ElectoralUnsynchronisedSettings,
	) {
		ELECTORAL_UNSYNCHRONISED_SETTINGS.with(|old_settings| {
			let mut settings_ref = old_settings.borrow_mut();
			settings_ref.clear();
			settings_ref.extend_from_slice(&unsynchronised_settings.encode());
		});
	}

	pub fn unsynchronised_settings<ES: ElectoralSystem>() -> ES::ElectoralUnsynchronisedSettings {
		ELECTORAL_UNSYNCHRONISED_SETTINGS.with(|old_settings| {
			let settings_ref = old_settings.borrow();
			ES::ElectoralUnsynchronisedSettings::decode(&mut &settings_ref[..]).unwrap()
		})
	}

	pub fn unsynchronised_state<ES: ElectoralSystem>() -> ES::ElectoralUnsynchronisedState {
		ELECTORAL_UNSYNCHRONISED_STATE.with(|old_state| {
			let state_ref = old_state.borrow();
			ES::ElectoralUnsynchronisedState::decode(&mut &state_ref[..]).unwrap()
		})
	}
	pub fn unsynchronised_state_map<ES: ElectoralSystem>(
		key: &ES::ElectoralUnsynchronisedStateMapKey,
	) -> Option<ES::ElectoralUnsynchronisedStateMapValue> {
		ELECTORAL_UNSYNCHRONISED_STATE_MAP.with(|old_state_map| {
			let state_map_ref = old_state_map.borrow();
			let value = state_map_ref.get(&key.encode()).cloned();

			value.map(|v| ES::ElectoralUnsynchronisedStateMapValue::decode(&mut &v[..]).unwrap())
		})
	}

	pub fn unsynchronised_state_map_all<ES: ElectoralSystem>(
	) -> BTreeMap<ES::ElectoralUnsynchronisedStateMapKey, ES::ElectoralUnsynchronisedStateMapValue>
	where
		ES::ElectoralUnsynchronisedStateMapKey: Ord,
	{
		ELECTORAL_UNSYNCHRONISED_STATE_MAP.with(|old_state_map| {
			let state_map_ref = old_state_map.borrow();
			BTreeMap::from_iter(state_map_ref.clone().into_iter().map(|(key, value)| {
				(
					ES::ElectoralUnsynchronisedStateMapKey::decode(&mut &key[..]).unwrap(),
					ES::ElectoralUnsynchronisedStateMapValue::decode(&mut &value[..]).unwrap(),
				)
			}))
		})
	}

	pub fn set_unsynchronised_state_map<ES: ElectoralSystem>(
		key: ES::ElectoralUnsynchronisedStateMapKey,
		value: Option<ES::ElectoralUnsynchronisedStateMapValue>,
	) {
		ELECTORAL_UNSYNCHRONISED_STATE_MAP.with(|old_state_map| {
			let mut state_map_ref = old_state_map.borrow_mut();
			match value {
				Some(v) => state_map_ref.insert(key.encode(), v.encode()),
				None => state_map_ref.remove(&key.encode()),
			};
		});
	}

	pub fn election_identifiers<ES: ElectoralSystem>() -> Vec<ElectionIdentifierOf<ES>> {
		ELECTION_PROPERTIES.with(|properties| {
			let properties_ref = properties.borrow();
			properties_ref
				.keys()
				.map(|k| ElectionIdentifierOf::<ES>::decode(&mut &k[..]).unwrap())
				.collect()
		})
	}

	pub fn set_consensus_status<ES: ElectoralSystem>(
		identifier: ElectionIdentifierOf<ES>,
		status: ConsensusStatus<ES::Consensus>,
	) {
		println!("Setting consensus status to {:?} for {:?}", status, identifier);
		CONSENSUS_STATUS.with(|old_consensus| {
			let mut consensus_ref = old_consensus.borrow_mut();
			consensus_ref.insert(identifier.encode(), status.encode());
		});
	}

	pub fn consensus_status<ES: ElectoralSystem>(
		identifier: ElectionIdentifierOf<ES>,
	) -> ConsensusStatus<ES::Consensus> {
		CONSENSUS_STATUS
			.with(|old_consensus| {
				let consensus_ref = old_consensus.borrow();
				consensus_ref
					.get(&identifier.encode())
					.map(|v| ConsensusStatus::<ES::Consensus>::decode(&mut &v[..]).unwrap())
			})
			.unwrap_or(ConsensusStatus::None)
	}

	pub fn new_election<ES: ElectoralSystem>(
		extra: <ES as ElectoralSystemTypes>::ElectionIdentifierExtra,
		properties: <ES as ElectoralSystemTypes>::ElectionProperties,
		state: <ES as ElectoralSystemTypes>::ElectionState,
	) -> ElectionIdentifierOf<ES> {
		let next_umi = Self::next_umi();
		let election_identifier = ElectionIdentifier::new(next_umi, extra);
		Self::increment_next_umi();

		Self::set_election_properties::<ES>(election_identifier, Some(properties));
		Self::set_state::<ES>(election_identifier, state);
		// These are normally stored once and synchronised by election identifier. In the tests we
		// simplify this by just storing the electoral settings (that would be fetched by
		// resolving the synchronisation) alongside the election.
		Self::set_electoral_settings_for_election::<ES>(
			election_identifier,
			Self::electoral_settings::<ES>(),
		);

		election_identifier
	}
}
