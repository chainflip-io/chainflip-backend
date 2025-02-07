use crate::*;
use frame_support::{pallet_prelude::Weight, storage::unhashed, traits::UncheckedOnRuntimeUpgrade};

use pallet_cf_elections::{
	self, BitmapComponents, ElectionConsensusHistory, ElectionConsensusHistoryUpToDate,
	ElectionProperties, ElectionState, ElectoralSettings, ElectoralSystemTypes,
	ElectoralUnsynchronisedState, ElectoralUnsynchronisedStateMap, IndividualComponents,
	SharedData, SharedDataReferenceCount,
};

use chainflip::solana_elections::SolanaElectoralSystemRunner;

use pallet_cf_elections::UniqueMonotonicIdentifier;

#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

mod old {
	use super::*;
	use cf_chains::sol::SolAmount;
	use chainflip::solana_elections::{
		SolanaBlockHeightTracking, SolanaEgressWitnessing, SolanaIngressTracking, SolanaLiveness,
		SolanaNonceTracking, SolanaVaultSwapTracking,
	};
	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};
	use pallet_cf_elections::{
		electoral_systems::composite::tuple_6_impls, ConsensusHistory, ElectionIdentifier,
	};
	use sp_runtime::FixedU128;

	#[derive(Debug, PartialEq, Eq, Encode, Decode, Clone)]
	pub struct SolanaFeeUnsynchronisedSettings {
		fee_multiplier: FixedU128,
	}

	#[frame_support::storage_alias]
	pub type ElectoralUnsynchronisedSettings = StorageValue<
		SolanaElections,
		((), SolanaFeeUnsynchronisedSettings, (), (), (), (), ()),
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type ElectoralUnsynchronisedState = StorageValue<
		SolanaElections,
		(
			<SolanaBlockHeightTracking as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
			SolAmount,
			(),
			<SolanaNonceTracking as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
			<SolanaEgressWitnessing as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
			<SolanaLiveness as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
			<SolanaVaultSwapTracking as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		),
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type ElectoralSettings = StorageMap<
		SolanaElections,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		(
			<SolanaBlockHeightTracking as ElectoralSystemTypes>::ElectoralSettings,
			(),
			<SolanaIngressTracking as ElectoralSystemTypes>::ElectoralSettings,
			<SolanaNonceTracking as ElectoralSystemTypes>::ElectoralSettings,
			<SolanaEgressWitnessing as ElectoralSystemTypes>::ElectoralSettings,
			<SolanaLiveness as ElectoralSystemTypes>::ElectoralSettings,
			<SolanaVaultSwapTracking as ElectoralSystemTypes>::ElectoralSettings,
		),
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type ElectionProperties = StorageMap<
		SolanaElections,
		Twox64Concat,
		// old election identifier
		ElectionIdentifier<CompositeElectionIdentifierExtra>,
		CompositeElectionProperties,
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type ElectionState = StorageMap<
		SolanaElections,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		CompositeElectionState,
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type ElectoralUnsynchronisedStateMap = StorageMap<
		SolanaElections,
		Twox64Concat,
		CompositeElectoralUnsynchronisedStateMapKey,
		CompositeElectoralUnsynchronisedStateMapValue,
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type ElectionConsensusHistory = StorageMap<
		SolanaElections,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		ConsensusHistory<CompositeConsensus>,
		OptionQuery,
	>;

	macro_rules! define_composite_enum {
		($name:ident, $type:ident, $BType:ty) => {
			#[derive(Debug, PartialEq, Eq, Encode, Decode, Clone)]
			pub enum $name {
				A(<SolanaBlockHeightTracking as ElectoralSystemTypes>::$type),
				// B was fee tracking
				B($BType),
				C(<SolanaIngressTracking as ElectoralSystemTypes>::$type),
				D(<SolanaNonceTracking as ElectoralSystemTypes>::$type),
				EE(<SolanaEgressWitnessing as ElectoralSystemTypes>::$type),
				FF(<SolanaLiveness as ElectoralSystemTypes>::$type),
				GG(<SolanaVaultSwapTracking as ElectoralSystemTypes>::$type),
			}

			paste::paste! {
				#[allow(non_snake_case)]
				pub fn [<translate_composite_enum_ $name>](
					enum_name: $name,
				) -> Option<<SolanaElectoralSystemRunner as ElectoralSystemTypes>::$type> {
					match enum_name {
						$name::A(a) => Some(tuple_6_impls::$name::A(a)),
						$name::B(_b) => None,
						$name::C(c) => Some(tuple_6_impls::$name::B(c)),
						$name::D(d) => Some(tuple_6_impls::$name::C(d)),
						$name::EE(ee) => Some(tuple_6_impls::$name::D(ee)),
						$name::FF(ff) => Some(tuple_6_impls::$name::EE(ff)),
						$name::GG(gg) => Some(tuple_6_impls::$name::FF(gg)),
					}
				}
			}
		};
	}

	define_composite_enum!(CompositeElectionState, ElectionState, ());
	define_composite_enum!(CompositeElectionProperties, ElectionProperties, ());
	define_composite_enum!(
		CompositeElectoralUnsynchronisedStateMapKey,
		ElectoralUnsynchronisedStateMapKey,
		()
	);
	define_composite_enum!(
		CompositeElectoralUnsynchronisedStateMapValue,
		ElectoralUnsynchronisedStateMapValue,
		()
	);
	define_composite_enum!(CompositeElectionIdentifierExtra, ElectionIdentifierExtra, ());
	define_composite_enum!(CompositeConsensus, Consensus, SolAmount);

	pub fn unwrap_composite_consensus_history(
		consensus_history: ConsensusHistory<CompositeConsensus>,
	) -> Option<ConsensusHistory<<SolanaElectoralSystemRunner as ElectoralSystemTypes>::Consensus>>
	{
		old::translate_composite_enum_CompositeConsensus(consensus_history.most_recent).map(
			|consensus| ConsensusHistory {
				most_recent: consensus,
				lost_since: consensus_history.lost_since,
			},
		)
	}
}

pub struct RemoveFeeTrackingMigration;

impl UncheckedOnRuntimeUpgrade for RemoveFeeTrackingMigration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let pre_upgrade_properties_len = old::ElectionProperties::iter().count() as u32;

		let pre_upgrade_election_state_len = old::ElectionState::iter().count() as u32;

		let pre_upgrade_map_len = old::ElectoralUnsynchronisedStateMap::iter().count() as u32;

		let mut pre_upgrade_data = Vec::new();
		pre_upgrade_data.extend(pre_upgrade_properties_len.encode());
		pre_upgrade_data.extend(pre_upgrade_election_state_len.encode());
		pre_upgrade_data.extend(pre_upgrade_map_len.encode());

		Ok(pre_upgrade_data)
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!("Starting remove fee tracking migration");

		// Only fee tracking had a value, so the rest can be deleted, and will default to ()
		old::ElectoralUnsynchronisedSettings::take();

		log::info!("Cleared electoral unsynchronised settings");

		// ElectoralUnsynchronisedState
		if let Some(state) = old::ElectoralUnsynchronisedState::take() {
			let (a, _deleted_b_fee, (), (), (), (), gg) = state;
			log::info!("Migrating electoral unsynchronised state {:?}", state);

			ElectoralUnsynchronisedState::<Runtime, SolanaInstance>::put((a, (), (), (), (), gg));
			log::info!("Inserted new electoral unsynchronised state");
		} else if let Some(raw_state) =
			unhashed::get_raw(&old::ElectoralUnsynchronisedState::hashed_key())
		{
			log::error!(
				"Unable to decode old electoral unsynchronised state: {}",
				hex::encode(raw_state)
			);
		}

		// ElectoralUnsynchronisedStateMap
		let mut new_state_map = Vec::new();
		for (key, value) in old::ElectoralUnsynchronisedStateMap::iter() {
			if let (Some(key), Some(value)) = (
				old::translate_composite_enum_CompositeElectoralUnsynchronisedStateMapKey(key),
				old::translate_composite_enum_CompositeElectoralUnsynchronisedStateMapValue(value),
			) {
				new_state_map.push((key, value))
			}
		}
		log::info!("Iterated over old state map");

		if !new_state_map.is_empty() {
			let _ = old::ElectoralUnsynchronisedStateMap::clear(u32::MAX, None);
			log::info!("Adding new state map with {:?} items", new_state_map.len());
			for (key, value) in new_state_map {
				ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>::insert(key, value);
			}
		}

		// ElectoralSettings
		let open_election_identifiers = old::ElectoralSettings::iter_keys();
		let mut new_settings = Vec::new();
		log::info!("Iterating over old settings");
		for electoral_settings_id in open_election_identifiers {
			if let Some((a, _b_deleted, c, d, ee, ff, gg)) =
				old::ElectoralSettings::get(electoral_settings_id)
			{
				new_settings.push((electoral_settings_id, (a, c, d, ee, ff, gg)));
			}
		}
		log::info!("Iterated over old settings");

		if !new_settings.is_empty() {
			log::info!("Adding new settings with {:?} items", new_settings.len());
			let _ = old::ElectoralSettings::clear(u32::MAX, None);
			log::info!("Cleared old electoral settings");
			for (electoral_settings_id, settings) in new_settings {
				ElectoralSettings::<Runtime, SolanaInstance>::insert(
					electoral_settings_id,
					settings,
				);
			}
		}

		// Properties
		let election_properties = old::ElectionProperties::iter().collect::<Vec<_>>();
		let mut new_election_properties = Vec::new();

		log::info!("Old election properties: {} elections.", election_properties.len());

		for (election_identifier, _props) in election_properties {
			let key = old::ElectionProperties::hashed_key_for(election_identifier.clone());

			let raw_storage_at_key =
				unhashed::get_raw(&key).expect("We just got the keys directly from the storage");

			let props =
				old::CompositeElectionProperties::decode(&mut &raw_storage_at_key[..]).unwrap();

			if let Some(props) = old::translate_composite_enum_CompositeElectionProperties(props) {
				let old_extra = election_identifier.extra();
				if let Some(new_extra) =
					old::translate_composite_enum_CompositeElectionIdentifierExtra(
						old_extra.clone(),
					) {
					new_election_properties
						.push((election_identifier.with_extra(new_extra), props));
				}
			}
		}

		if !new_election_properties.is_empty() {
			log::info!(
				"Adding new election properties with {} items",
				new_election_properties.len()
			);

			let _ = old::ElectionProperties::clear(u32::MAX, None);
			log::info!("Cleared old election properties");

			for (election_identifier, props) in new_election_properties {
				ElectionProperties::<Runtime, SolanaInstance>::insert(election_identifier, props);
			}
			log::info!("Inserted new election properties");
		}

		// Election state
		let election_state = old::ElectionState::iter().collect::<Vec<_>>();
		log::info!("Old election state had {} entries.", election_state.len());
		let mut new_election_state = Vec::new();
		for (election_identifier, _state) in election_state {
			let raw_storage_at_key =
				unhashed::get_raw(&old::ElectionState::hashed_key_for(election_identifier))
					.expect("We just got the keys directly from the storage");

			let state = old::CompositeElectionState::decode(&mut &raw_storage_at_key[..]).unwrap();

			if let Some(state) = old::translate_composite_enum_CompositeElectionState(state) {
				new_election_state.push((election_identifier, state));
			}
		}

		if !new_election_state.is_empty() {
			log::info!("Adding new election state with {:?} items", new_election_state.len());
			let _ = old::ElectionState::clear(u32::MAX, None);
			for (election_identifier, state) in new_election_state {
				ElectionState::<Runtime, SolanaInstance>::insert(election_identifier, state);
			}
		}

		// Composite consensus
		let election_consensus_history = old::ElectionConsensusHistory::iter().collect::<Vec<_>>();
		let mut new_election_consensus_history = Vec::new();
		for (election_identifier, consensus) in election_consensus_history {
			if let Some(consensus) = old::unwrap_composite_consensus_history(consensus) {
				new_election_consensus_history.push((election_identifier, consensus));
			}
		}

		if !new_election_consensus_history.is_empty() {
			log::info!(
				"Adding new election consensus history with {:?} items",
				new_election_consensus_history.len()
			);
			let _ = old::ElectionConsensusHistory::clear(u32::MAX, None);
			for (election_identifier, consensus) in new_election_consensus_history {
				ElectionConsensusHistory::<Runtime, SolanaInstance>::insert(
					election_identifier,
					consensus,
				);
			}
		}

		// The engines will re-fill these votes.
		let _ = SharedDataReferenceCount::<Runtime, SolanaInstance>::clear(u32::MAX, None);
		let _ = SharedData::<Runtime, SolanaInstance>::clear(u32::MAX, None);
		let _ = BitmapComponents::<Runtime, SolanaInstance>::clear(u32::MAX, None);
		let _ = IndividualComponents::<Runtime, SolanaInstance>::clear(u32::MAX, None);
		let _ = ElectionConsensusHistoryUpToDate::<Runtime, SolanaInstance>::clear(u32::MAX, None);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_upgrade_properties_len = u32::decode(&mut &state[..4]).unwrap();
		let pre_upgrade_election_state_len = u32::decode(&mut &state[4..8]).unwrap();
		let pre_upgrade_map_len = u32::decode(&mut &state[8..12]).unwrap();

		// Check lengths are correct:
		let properties_len = ElectionProperties::<Runtime, SolanaInstance>::iter().count() as u32;
		let election_state_len = ElectionState::<Runtime, SolanaInstance>::iter().count() as u32;
		let map_len =
			ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>::iter().count() as u32;

		assert!(
			// There's always going to be one fee election removed, so the properties length should
			// be one less than the pre-upgrade length.
			properties_len == pre_upgrade_properties_len.saturating_sub(1) ||
                // for idempotency
				properties_len == pre_upgrade_properties_len
		);
		assert!(
			// There's always going to be one fee election removed, so the election state length
			// should be one less than the pre-upgrade length.
			election_state_len == pre_upgrade_election_state_len.saturating_sub(1) ||
                // for idempotency
				election_state_len == pre_upgrade_election_state_len
		);
		assert_eq!(map_len, pre_upgrade_map_len);

		use pallet_cf_elections::ElectionIdentifierOf;

		let open_election_identifiers = ElectoralSettings::<Runtime, SolanaInstance>::iter_keys();
		for electoral_settings_id in open_election_identifiers {
			// should decode
			let settings: Option<
				<SolanaElectoralSystemRunner as ElectoralSystemTypes>::ElectoralSettings,
			> = ElectoralSettings::<Runtime, SolanaInstance>::get(electoral_settings_id);
			assert!(settings.is_some());
		}

		// check properties decoded correctly
		let _props: Vec<(
			ElectionIdentifierOf<SolanaElectoralSystemRunner>,
			<SolanaElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
		)> = ElectionProperties::<Runtime, SolanaInstance>::iter().collect::<Vec<_>>();

		assert!(SharedDataReferenceCount::<Runtime, SolanaInstance>::iter_keys()
			.next()
			.is_none());
		assert!(SharedData::<Runtime, SolanaInstance>::iter_keys().next().is_none());
		assert!(BitmapComponents::<Runtime, SolanaInstance>::iter_keys().next().is_none());
		assert!(IndividualComponents::<Runtime, SolanaInstance>::iter_keys().next().is_none());
		assert!(ElectionConsensusHistoryUpToDate::<Runtime, SolanaInstance>::iter_keys()
			.next()
			.is_none());

		Ok(())
	}
}
