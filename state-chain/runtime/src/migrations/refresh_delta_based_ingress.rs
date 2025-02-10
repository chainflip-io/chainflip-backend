use crate::{
	chainflip::solana_elections::{SolanaElectoralSystemRunner, SolanaIngressTracking},
	Runtime,
};
use cf_chains::instances::SolanaInstance;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use pallet_cf_elections::{
	electoral_systems::composite::{self, tags::B},
	RunnerStorageAccess,
};
#[cfg(feature = "try-runtime")]
use sp_std::collections::btree_map::BTreeMap;
use sp_std::vec::Vec;

pub struct RefreshDeltaBasedIngress;

impl UncheckedOnRuntimeUpgrade for RefreshDeltaBasedIngress {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let mut dbi_elections = Vec::new();
		for (id, properties) in
			pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>::iter()
		{
			if let composite::tuple_6_impls::CompositeElectionProperties::B(_) = properties {
				dbi_elections.push(id);
			}
		}
		SolanaElectoralSystemRunner::with_identifiers(dbi_elections, |(_as, bs, ..)| {
			SolanaIngressTracking::migrate::<
				composite::tuple_6_impls::DerivedElectoralAccess<
					B,
					SolanaIngressTracking,
					RunnerStorageAccess<Runtime, SolanaInstance>,
				>,
			>(&bs)
			.unwrap_or_else(|_| {
				log::error!("Failed to migrate SolanaIngressTracking for election ids: {:?}", bs);
			});
		});
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let mut election_counts = BTreeMap::<u32, u32>::new();
		for (_id, properties) in
			pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>::iter()
		{
			*election_counts
				.entry(match properties {
					composite::tuple_6_impls::CompositeElectionProperties::A(_) => 0xa,
					composite::tuple_6_impls::CompositeElectionProperties::B(_) => 0xb,
					composite::tuple_6_impls::CompositeElectionProperties::C(_) => 0xc,
					composite::tuple_6_impls::CompositeElectionProperties::D(_) => 0xd,
					composite::tuple_6_impls::CompositeElectionProperties::EE(_) => 0xe,
					composite::tuple_6_impls::CompositeElectionProperties::FF(_) => 0xf,
				})
				.or_default() += 1
		}

		Ok(election_counts.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let pre_upgrade_election_counts = BTreeMap::<u32, u32>::decode(&mut &state[..])
			.map_err(|_| "Invalid post upgrade state.")?;

		let mut post_upgrade_election_counts = BTreeMap::<_, u32>::new();
		for (_id, properties) in
			pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>::iter()
		{
			*post_upgrade_election_counts
				.entry(match properties {
					composite::tuple_6_impls::CompositeElectionProperties::A(_) => 0xa,
					composite::tuple_6_impls::CompositeElectionProperties::B(_) => 0xb,
					composite::tuple_6_impls::CompositeElectionProperties::C(_) => 0xc,
					composite::tuple_6_impls::CompositeElectionProperties::D(_) => 0xd,
					composite::tuple_6_impls::CompositeElectionProperties::EE(_) => 0xe,
					composite::tuple_6_impls::CompositeElectionProperties::FF(_) => 0xf,
				})
				.or_default() += 1
		}

		assert_eq!(pre_upgrade_election_counts, post_upgrade_election_counts);

		Ok(())
	}
}
