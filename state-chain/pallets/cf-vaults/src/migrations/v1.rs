use crate::*;

use frame_support::{storage::migration::*, StorageHasher};
#[cfg(feature = "try-runtime")]
use frame_support::{traits::OnRuntimeUpgradeHelpersExt, Hashable};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::convert::{TryFrom, TryInto};
use v0_types::{VaultRotationStatusV0, VaultV0};

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		migrate_storage::<T, I>()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		pre_migration_checks::<T, I>()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		post_migration_checks::<T, I>()
	}
}

const PALLET_NAME_V0: &[u8; 6] = b"Vaults";

const PALLET_NAME_V1: &[u8; 13] = b"EthereumVault";

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
enum ChainId {
	Ethereum,
}

/// V1 Storage migration.
///
/// It *should* work during a rotation, but we should try to avoid it.
pub fn migrate_storage<T: Config<I>, I: 'static>() -> frame_support::weights::Weight {
	log::info!("🏯 migrate_storage to V1");
	// The pallet has been renamed.
	move_pallet(PALLET_NAME_V0, PALLET_NAME_V1);

	// The old vaults were indexed by ChainId - we need to construct the Storage suffix by
	// hashing the Ethereum ChainId and then write the data back using the new storage
	// accessors.
	// If the conversion between old and new fails (it shouldn't!), we print an error and
	// continue.
	for epoch in 0..=CurrentEpochIndex::<T>::get() {
		let hash = [
			epoch.using_encoded(<Blake2_128Concat as StorageHasher>::hash),
			ChainId::Ethereum.using_encoded(<Blake2_128Concat as StorageHasher>::hash),
		]
		.concat();

		if let Some(old_vault) =
			take_storage_value::<VaultV0<T, I>>(PALLET_NAME_V1, b"Vaults", &hash[..])
		{
			old_vault
				.try_into()
				.map(|new_vault: Vault<T::Chain>| {
					Vaults::<T, I>::insert(epoch, new_vault);
				})
				.unwrap_or_else(|e| {
					log::error!("Unable to convert Vault from V0 to V1: {:?}", e);
				});
		} else {
			log::info!("No vault for epoch {:?}, skipping storage migration.", epoch);
		}
	}

	// The Ethereum ChainNonces value has been migrated to a single global nonce in the environment
	// pallet.
	let nonce = take_storage_item::<_, u64, Blake2_128Concat>(
		PALLET_NAME_V1,
		b"ChainNonces",
		ChainId::Ethereum,
	)
	.unwrap_or_else(|| {
		log::warn!("🏯 No nonce value to migrate.");
		0
	});
	put_storage_value(b"Environment", b"GlobalSignatureNonce", b"", nonce);

	// If possible we should avoid upgrading during a rotation, but just in case...
	if let Some(status_v0) = take_storage_item::<_, VaultRotationStatusV0<T, I>, Blake2_128Concat>(
		PALLET_NAME_V1,
		b"PendingVaultRotations",
		ChainId::Ethereum,
	) {
		match VaultRotationStatus::<T, I>::try_from(status_v0) {
			Ok(status) => PendingVaultRotation::<T, I>::set(Some(status)),
			Err(e) => log::error!("Failed to convert VaultRotationStatus from V0 to V1: {:?}", e),
		}
	} else {
		log::info!("🏯 No pending vault rotations to migrate.");
	}

	if let Some(resolution_pending) = take_storage_item::<
		_,
		Vec<(ChainId, BlockNumberFor<T>)>,
		Identity,
	>(PALLET_NAME_V1, b"KeygenResolutionPending", ())
	{
		if let Some((ChainId::Ethereum, block_number)) = resolution_pending.first() {
			KeygenResolutionPendingSince::<T, I>::put(block_number);
		}
	} else {
		log::info!("🏯 No pending vault rotations to migrate.");
	}

	0
}

#[cfg(feature = "try-runtime")]
pub fn pre_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	ensure!(
		have_storage_value(
			PALLET_NAME_V0,
			b"Vaults",
			[100u32.blake2_128_concat(), ChainId::Ethereum.blake2_128_concat()]
				.concat()
				.as_slice()
		),
		"🏯 Can't find Ethereum vault at hash {:?}."
	);
	ensure!(
		have_storage_value(
			PALLET_NAME_V0,
			b"ChainNonces",
			ChainId::Ethereum.blake2_128_concat().as_slice()
		),
		"🏯 Can't find Ethereum nonce."
	);
	let pre_migration_id_counter: u64 =
		get_storage_value(b"Vaults", b"KeygenCeremonyIdCounter", b"").unwrap_or_else(|| {
			log::warn!("🏯 Couldn't extract old id counter, assuming default");
			Default::default()
		});

	Pallet::<T, I>::set_temp_storage(pre_migration_id_counter, "id_counter");

	Ok(())
}

#[cfg(feature = "try-runtime")]
pub fn post_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	ensure!(
		Vaults::<T, I>::contains_key(CurrentEpochIndex::<T>::get()),
		"💥 No vault for current epoch!"
	);

	let pre_migration_id_counter: u64 = Pallet::<T, I>::get_temp_storage("id_counter")
		.ok_or("No id_counter written during the pre-migration checks")?;

	let next = T::CeremonyIdProvider::next_ceremony_id();
	ensure!(pre_migration_id_counter + 1 <= next, {
		log::error!(
			"KeygenCeremonyIdCounter pre-migration: {:?} / next: {:?}.",
			pre_migration_id_counter,
			next
		);
		"🏯 KeygenCeremonyIdCounter pre/post migration inconsistency."
	});

	log::info!(
		"🏯 KeygenCeremonyIdCounter checked; Pre-migration ceremony Id: {}",
		pre_migration_id_counter,
	);
	Ok(())
}

mod v0_types {

	use cf_traits::AuthorityCount;

	use super::*;

	#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, Default)]
	pub struct BlockHeightWindowV0 {
		pub from: u64,
		pub to: Option<u64>,
	}

	#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
	pub struct VaultV0<T: Config<I>, I: 'static = ()> {
		/// The vault's public key.
		pub public_key: Vec<u8>,
		/// The active window for this vault
		pub active_window: BlockHeightWindowV0,
		/// Marker.
		_phantom_data: PhantomData<(T, I)>,
	}

	impl<T: Config<I>, I: 'static> TryFrom<VaultV0<T, I>> for Vault<T::Chain> {
		type Error = &'static str;

		fn try_from(old: VaultV0<T, I>) -> Result<Self, Self::Error> {
			Ok(Self {
				public_key: old
					.public_key
					.try_into()
					.map_err(|_| "Unable to convert Vec<u8> public key to AggKey format.")?,
				active_from_block: old.active_window.from.into(),
			})
		}
	}

	#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
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
				candidate_count: old.candidate_count as AuthorityCount,
				remaining_candidates: old.remaining_candidates,
				success_votes: old
					.success_votes
					.into_iter()
					.map(|(key, votes)| {
						key.try_into()
							.map_err(|_| "Unable to convert Vec<u8> public key to AggKey format.")
							.map(|key| (key, votes as AuthorityCount))
					})
					.collect::<Result<_, _>>()?,
				blame_votes: old
					.blame_votes
					.into_iter()
					.map(|(key, votes)| (key, votes as AuthorityCount))
					.collect(),
			})
		}
	}

	#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
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

	impl<T: Config<I>, I: 'static> TryFrom<VaultRotationStatusV0<T, I>> for VaultRotationStatus<T, I> {
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
					Self::AwaitingRotation {
						new_public_key: new_public_key.try_into().map_err(|_| {
							"Unable to convert Vec<u8> public key to AggKey format."
						})?,
					},
				VaultRotationStatusV0::Complete { tx_hash, _phantom } =>
					Self::Complete { tx_hash: vec_to_hash::<T::Chain>(tx_hash)? },
			})
		}
	}

	/// This is a bit of a hack. It abuses the fact that we know the V0 transaction hash was
	/// always 32 bytes. We can't convert directly, so we use Encode/Decode to get the bytes
	/// into the correct format.
	///
	/// If the provided vec is too large, it is truncated. If it's too small, it zero-pads.
	fn vec_to_hash<T: ChainCrypto>(mut v: Vec<u8>) -> Result<T::TransactionHash, &'static str> {
		let mut hash = [0u8; 32];
		if v.len() < 32 {
			let padding = [0u8].repeat(32 - v.len());
			v.extend_from_slice(&padding[..]);
		}
		hash.copy_from_slice(&v[..32]);
		let encoded_hash = hash.encode();
		<T::TransactionHash as Decode>::decode(&mut &encoded_hash[..])
			.map_err(|_| "Unable to convert Vec<u8> bytes to a valid TransactionHash")
	}

	#[cfg(test)]
	mod test_super {
		use cf_chains::Ethereum;

		use super::*;

		#[test]
		fn vec_to_hash_conversion_exact() {
			let v: Vec<u8> = [[0xcf; 16], [0x42; 16]].concat();
			let h: [u8; 32] = v.clone().try_into().unwrap();

			assert_eq!(cf_chains::eth::H256::from(h), vec_to_hash::<Ethereum>(v).unwrap());
		}

		#[test]
		fn vec_to_hash_conversion_smaller() {
			let v: Vec<u8> = vec![0xcf; 16];
			let h: [u8; 32] = [&v[..], &[0u8; 16][..]].concat().try_into().unwrap();

			assert_eq!(cf_chains::eth::H256::from(h), vec_to_hash::<Ethereum>(v).unwrap());
		}

		#[test]
		fn vec_to_hash_conversion_larger() {
			let mut v: Vec<u8> = [[0xcf; 16], [0x42; 16]].concat();
			let h: [u8; 32] = v.clone().try_into().unwrap();
			v.extend_from_slice(b"hello");

			assert_eq!(cf_chains::eth::H256::from(h), vec_to_hash::<Ethereum>(v).unwrap());
		}
	}
}
