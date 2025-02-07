use crate::{
	chainflip::solana_elections::{SolanaElectoralSystemRunner, SolanaIngressTracking},
	Runtime,
};
use cf_chains::instances::SolanaInstance;
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use pallet_cf_elections::{
	electoral_systems::composite::{self, tags::B},
	RunnerStorageAccess,
};
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
}
