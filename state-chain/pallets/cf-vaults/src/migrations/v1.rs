use super::*;
use frame_support::storage::migration::*;
#[cfg(feature = "try-runtime")]
use frame_support::traits::OnRuntimeUpgradeHelpersExt;

pub fn migrate_storage<T: Config<I>, I: 'static>() -> frame_support::weights::Weight {
	log::info!("🏯 migrate_storage to V1");
	// The pallet has been renamed.
	move_pallet(b"Vaults", b"EthereumVault");

	//
	// storage_iter("EthereumVault", "Vaults")
	// 	.

	releases::V1.put::<Pallet<T, I>>();
	0
}

#[cfg(feature = "try-runtime")]
pub fn pre_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	ensure!(StorageVersion::get::<Pallet<T, I>>() == releases::V0, "Expected storage version V0.");

	let pre_migration_id_counter: u64 =
		get_storage_value(b"Vaults", b"KeygenCeremonyIdCounter", b"").unwrap_or_else(|| {
			log::warn!("Couldn't extract old id counter, assuming default");
			Default::default()
		});

	Pallet::<T, I>::set_temp_storage(pre_migration_id_counter, "id_counter");

	Ok(())
}

#[cfg(feature = "try-runtime")]
pub fn post_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	ensure!(StorageVersion::get::<Pallet<T, I>>() == releases::V1, "Expected storage version V1.");

	let pre_migration_id_counter: u64 = Pallet::<T, I>::get_temp_storage("id_counter")
		.ok_or("No id_counter written during the pre-migration checks")?;

	let post_migration_id_counter = KeygenCeremonyIdCounter::<T, I>::get();

	log::info!(
		"🏯 KeygenCeremonyIdCounter checked; Pre-migration: {}, Post-migration: {}",
		pre_migration_id_counter,
		post_migration_id_counter
	);
	ensure!(
		pre_migration_id_counter == post_migration_id_counter,
		"CeremonyId counter has changed!"
	);
	Ok(())
}

mod v0_types {
	use std::convert::TryInto;

	use crate::*;

	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
	pub struct VaultV0<T: Config<I>, I: 'static = ()> {
		/// The vault's public key.
		pub public_key: Vec<u8>,
		/// The active window for this vault
		pub active_window: BlockHeightWindow,
		/// Marker.
		_phantom_data: PhantomData<(T, I)>,
	}

	impl<T: Config<I>, I: 'static> TryFrom<VaultV0<T, I>> for Vault<T::Chain> {
		type Error = &'static str;

		fn try_from(old: VaultV0<T, I>) -> Result<Self, Self::Error> {
			Ok(Self { public_key: old.public_key.try_into()?, active_window: old.active_window })
		}
	}

	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
	pub struct KeygenResponseStatusV0<T: Config<I>, I: 'static = ()> {
		/// The total number of candidates participating in the keygen ceremony.
		candidate_count: u32,
		/// The candidates that have yet to reply.
		remaining_candidates: BTreeSet<T::ValidatorId>,
		/// A map of new keys with the number of votes for each key.
		success_votes: BTreeMap<Vec<u8>, u32>,
		/// A map of the number of blame votes that each validator has received.
		blame_votes: BTreeMap<T::ValidatorId, u32>,
		/// Marker.
		_phantom_data: PhantomData<(T, I)>,
	}

	impl<T: Config<I>, I: 'static> TryFrom<KeygenResponseStatusV0<T, I>>
		for KeygenResponseStatus<T, I>
	// where
	// 	TLegacy: Config<()> + Chainflip<ValidatorId = <T as Chainflip>::ValidatorId>,
	{
		type Error = &'static str;

		fn try_from(old: KeygenResponseStatusV0<T, I>) -> Result<Self, Self::Error> {
			Ok(Self {
				candidate_count: old.candidate_count,
				remaining_candidates: old.remaining_candidates,
				success_votes: old
					.success_votes
					.into_iter()
					.map(|(key, votes)| key.try_into().map(|key| (key, votes)))
					.collect::<Result<_, _>>()?,
				blame_votes: old.blame_votes,
			})
		}
	}

	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
	pub enum VaultRotationStatusV0<T: Config<I>, I: 'static = ()> {
		AwaitingKeygen {
			keygen_ceremony_id: CeremonyId,
			response_status: KeygenResponseStatusV0<T, I>,
			_phantom: PhantomData<I>,
		},
		AwaitingRotation {
			new_public_key: Vec<u8>,
			_phantom: PhantomData<I>,
		},
		Complete {
			tx_hash: Vec<u8>,
			_phantom: PhantomData<I>,
		},
	}

	impl<T: Config<I>, I: 'static> TryFrom<VaultRotationStatusV0<T, I>> for VaultRotationStatus<T, I>
	where
		// TLegacy: Config<()> + Chainflip<ValidatorId = <T as Chainflip>::ValidatorId>,
		<T::Chain as ChainCrypto>::TransactionHash: TryFrom<Vec<u8>>,
	{
		type Error = &'static str;

		fn try_from(old: VaultRotationStatusV0<T, I>) -> Result<Self, Self::Error> {
			Ok(match old {
				VaultRotationStatusV0::AwaitingKeygen {
					keygen_ceremony_id,
					response_status,
					_phantom,
				} => Self::AwaitingKeygen {
					keygen_ceremony_id,
					response_status: response_status.try_into()?,
				},
				VaultRotationStatusV0::AwaitingRotation { new_public_key, _phantom } =>
					Self::AwaitingRotation { new_public_key: new_public_key.try_into()? },
				VaultRotationStatusV0::Complete { tx_hash, _phantom } => Self::Complete {
					tx_hash: tx_hash.try_into().map_err(|_| {
						"Unable to convert Vec<u8> bytes to a valid TransactionHash"
					})?,
				},
			})
		}
	}
}
