use core::cmp::max;
use frame_support::{generate_storage_alias, traits::OnRuntimeUpgrade, weights::RuntimeDbWeight};
use pallet_cf_threshold_signature::CeremonyId;

use crate::Runtime;

/// Unify signing and ceremony ids in a single ceremony id. Uses the greater of the two
/// keygen/signing ceremony ids as the new unified id.
pub struct UnifyCeremonyIds;

generate_storage_alias!(
	EthereumThresholdSigner, SigningCeremonyIdCounter => Value<CeremonyId>
);

generate_storage_alias!(
	EthereumVault, KeygenCeremonyIdCounter => Value<CeremonyId>
);

impl OnRuntimeUpgrade for UnifyCeremonyIds {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let signing_ceremony_id_counter = SigningCeremonyIdCounter::take().unwrap_or_default();
		let keygen_ceremony_id_counter = KeygenCeremonyIdCounter::take().unwrap_or_default();

		pallet_cf_validator::CeremonyIdCounter::<Runtime>::put(max(
			signing_ceremony_id_counter,
			keygen_ceremony_id_counter,
		));

		RuntimeDbWeight::default().reads_writes(2, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		use frame_support::ensure;

		let signing_ceremony_id_counter = SigningCeremonyIdCounter::get();
		let keygen_ceremony_id_counter = KeygenCeremonyIdCounter::get();

		ensure!(signing_ceremony_id_counter.is_some(), "No entry for SigningCeremonyIdCounter.");
		ensure!(keygen_ceremony_id_counter.is_some(), "No entry for KeygenCeremonyIdCounter.");

		log::info!(
			"Merging ceremony id counters. Keygen: {:?}, Signing: {:?}.",
			keygen_ceremony_id_counter,
			signing_ceremony_id_counter
		);

		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		let ceremony_id_counter = crate::Validator::ceremony_id_counter();

		log::info!("Merged ceremony id counter: {:?}.", ceremony_id_counter);

		Ok(())
	}
}
