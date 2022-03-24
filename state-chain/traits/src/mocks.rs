#![cfg(feature = "std")]

use codec::{Decode, Encode};
use frame_support::{storage, StorageHasher, Twox64Concat};

// pub mod broadcaster;
pub mod ceremony_id_provider;
pub mod chainflip_account;
pub mod ensure_origin_mock;
pub mod ensure_witnessed;
pub mod epoch_info;
pub mod key_provider;
pub mod keygen_exclusion;
pub mod offence_reporting;
pub mod online;
pub mod signer_nomination;
pub mod stake_transfer;
pub mod threshold_signer;
pub mod time_source;
pub mod vault_rotation;
pub mod waived_fees_mock;
pub mod witnesser;

trait MockPallet {
	const PREFIX: &'static [u8];
}

trait MockPalletStorage {
	fn put_storage<K: Encode, V: Encode>(store: &[u8], k: K, v: V);
	fn get_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V>;
	fn take_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V>;
}

fn storage_key<K: Encode>(prefix: &[u8], store: &[u8], k: K) -> Vec<u8> {
	[prefix, store, &k.encode()].concat()
}

impl<T: MockPallet> MockPalletStorage for T {
	fn put_storage<K: Encode, V: Encode>(store: &[u8], k: K, v: V) {
		storage::hashed::put(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, k),
			&v.encode(),
		)
	}

	fn get_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V> {
		storage::hashed::get(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, k),
		)
	}

	fn take_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V> {
		storage::hashed::take(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, k),
		)
	}
}
