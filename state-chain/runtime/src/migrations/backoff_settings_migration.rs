use crate::*;
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

use old::translate_composite_enum_CompositeElectionProperties;
use pallet_cf_elections::{
	self, electoral_systems::blockchain::delta_based_ingress::BackoffSettings, ElectionProperties,
	ElectoralSettings, ElectoralSystemTypes, UniqueMonotonicIdentifier,
};

use chainflip::solana_elections::SolanaElectoralSystemRunner;
use pallet_cf_elections::electoral_systems::composite::tuple_7_impls;

mod old {
	use crate::chainflip::solana_elections::SolanaIngressSettings;

	use super::*;

	use pallet_cf_elections::electoral_systems::blockchain::delta_based_ingress::{
		ChannelTotalIngressedFor, OpenChannelDetailsFor,
	};

	use cf_traits::IngressSink;

	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};

	use chainflip::solana_elections::{
		SolanaBlockHeightTracking, SolanaEgressWitnessing, SolanaLiveness, SolanaNonceTracking,
		SolanaVaultSwapTracking,
	};

	use pallet_cf_elections::ElectionIdentifierOf;

	macro_rules! define_composite_enum {
		($name:ident, $type:ident, $BOldType:ty) => {
			#[derive(Debug, PartialEq, Eq, Encode, Decode, Clone)]
			pub enum $name {
				A(<SolanaBlockHeightTracking as ElectoralSystemTypes>::$type),
				B($BOldType),
				C(<SolanaNonceTracking as ElectoralSystemTypes>::$type),
				D(<SolanaEgressWitnessing as ElectoralSystemTypes>::$type),
				EE(<SolanaLiveness as ElectoralSystemTypes>::$type),
				FF(<SolanaVaultSwapTracking as ElectoralSystemTypes>::$type),
			}

			paste::paste! {
				#[allow(non_snake_case)]
				pub fn [<translate_composite_enum_ $name>](
					enum_name: $name,
				) -> <SolanaElectoralSystemRunner as ElectoralSystemTypes>::$type {
					match enum_name {
						$name::A(a) => tuple_7_impls::$name::A(a),
						// Convert from the old into the new type
						$name::B(b) => translate_from_old(b),
						$name::C(c) => tuple_7_impls::$name::C(c),
						$name::D(d) => tuple_7_impls::$name::D(d),
						$name::EE(ee) => tuple_7_impls::$name::EE(ee),
						$name::FF(ff) => tuple_7_impls::$name::FF(ff),
					}
				}
			}
		};
	}

	pub type OldDeltaBasedElectionProperties = BTreeMap<
		<SolanaIngressEgress as IngressSink>::Account,
		(OpenChannelDetailsFor<SolanaIngressEgress>, ChannelTotalIngressedFor<SolanaIngressEgress>),
	>;

	#[frame_support::storage_alias]
	pub type ElectionProperties = StorageMap<
		SolanaElections,
		Twox64Concat,
		ElectionIdentifierOf<SolanaElectoralSystemRunner>,
		CompositeElectionProperties,
		OptionQuery,
	>;

	pub fn translate_from_old(
		old: OldDeltaBasedElectionProperties,
	) -> <SolanaElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties {
		// Setting them all to the latest block number means that all deltabased elections open at
		// time of migration will backoff in 1 hour.
		tuple_7_impls::CompositeElectionProperties::B((
			old,
			frame_system::Pallet::<Runtime>::block_number(),
		))
	}

	define_composite_enum!(
		CompositeElectionProperties,
		ElectionProperties,
		OldDeltaBasedElectionProperties
	);

	#[frame_support::storage_alias]
	pub type ElectoralSettings = StorageMap<
		SolanaElections,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		(
			<SolanaBlockHeightTracking as ElectoralSystemTypes>::ElectoralSettings,
			// The old settings were just the SolanaIngressSettings
			SolanaIngressSettings,
			<SolanaNonceTracking as ElectoralSystemTypes>::ElectoralSettings,
			<SolanaEgressWitnessing as ElectoralSystemTypes>::ElectoralSettings,
			<SolanaLiveness as ElectoralSystemTypes>::ElectoralSettings,
			<SolanaVaultSwapTracking as ElectoralSystemTypes>::ElectoralSettings,
		),
		OptionQuery,
	>;
}

pub struct SolanaBackoffSettingsMigration;

impl UncheckedOnRuntimeUpgrade for SolanaBackoffSettingsMigration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let old_settings_count = old::ElectoralSettings::iter().count() as u32;
		let old_properties_count = old::ElectionProperties::iter().count() as u32;

		let current_block_number = frame_system::Pallet::<Runtime>::block_number();

		let mut pre_upgrade_state = Vec::new();
		pre_upgrade_state.extend(old_settings_count.encode());
		pre_upgrade_state.extend(old_properties_count.encode());
		pre_upgrade_state.extend(current_block_number.encode());
		Ok(pre_upgrade_state)
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!("Starting remove fee tracking migration");

		let new_properties = old::ElectionProperties::iter()
			.map(|(id, old_property)| {
				(id, translate_composite_enum_CompositeElectionProperties(old_property))
			})
			.collect::<Vec<_>>();

		for (id, new_property) in new_properties {
			ElectionProperties::<Runtime, SolanaInstance>::insert(id, new_property);
		}

		let old_settings = old::ElectoralSettings::drain().collect::<Vec<_>>();

		for (id, (a, b, c, d, ee, ff)) in old_settings {
			ElectoralSettings::<Runtime, SolanaInstance>::insert(
				id,
				(
					a,
					// New Deltabased ingress requires backoff settings
					(b, BackoffSettings { backoff_after_blocks: 600, backoff_frequency: 100 }),
					c,
					d,
					ee,
					ff,
					(),
				),
			);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_upgrade_settings_count = u32::decode(&mut &state[..4]).unwrap();
		let pre_upgrade_properties_count = u32::decode(&mut &state[4..8]).unwrap();
		let pre_upgrade_block_number = u32::decode(&mut &state[8..12]).unwrap();

		let current_settings_count: u32 =
			ElectoralSettings::<Runtime, SolanaInstance>::iter().count() as u32;
		let current_properties_count =
			ElectionProperties::<Runtime, SolanaInstance>::iter().count() as u32;

		assert_eq!(pre_upgrade_settings_count, current_settings_count);
		assert_eq!(pre_upgrade_properties_count, current_properties_count);

		// Check we have backoff settings set correctly.
		for (_id, (_a, (_bb, backoff_settings), _c, _d, _ee, _ff, _gg)) in
			ElectoralSettings::<Runtime, SolanaInstance>::iter()
		{
			assert_eq!(
				backoff_settings,
				BackoffSettings { backoff_after_blocks: 600, backoff_frequency: 100 }
			);
		}

		// Check we have the correct properties set.
		for (_id, property) in ElectionProperties::<Runtime, SolanaInstance>::iter() {
			if let tuple_7_impls::CompositeElectionProperties::B(delta_properties) = property {
				{
					assert_eq!(delta_properties.1, pre_upgrade_block_number);
				}
			}
		}

		Ok(())
	}
}
