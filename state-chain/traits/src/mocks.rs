#![cfg(feature = "std")]

use codec::{Decode, Encode, EncodeLike};
use frame_support::{storage, StorageHasher, Twox64Concat};

// pub mod broadcaster;
pub mod account_role_registry;
pub mod api_call;
pub mod bid_info;
pub mod ceremony_id_provider;
pub mod egress_handler;
pub mod ensure_origin_mock;
pub mod ensure_witnessed;
pub mod epoch_info;
pub mod eth_environment_provider;
pub mod eth_replay_protection_provider;
pub mod ingress_handler;
pub mod key_provider;
pub mod offence_reporting;
pub mod qualify_node;
pub mod reputation_resetter;
pub mod signer_nomination;
pub mod stake_transfer;
pub mod staking_info;
pub mod system_state_info;
pub mod threshold_signer;
pub mod time_source;
pub mod vault_rotator;
pub mod waived_fees_mock;

trait MockPallet {
	const PREFIX: &'static [u8];
}

trait MockPalletStorage {
	fn put_storage<K: Encode, V: Encode>(store: &[u8], k: K, v: V);
	fn get_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V>;
	fn take_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V>;
	fn put_value<V: Encode>(store: &[u8], v: V);
	fn get_value<V: Decode + Sized>(store: &[u8]) -> Option<V>;
	fn mutate_storage<
		K: Encode,
		E: EncodeLike<K>,
		V: Encode + Decode + Sized,
		R,
		F: FnOnce(&mut Option<V>) -> R,
	>(
		store: &[u8],
		k: &E,
		f: F,
	) -> R {
		let mut storage = Self::get_storage(store, k);
		let result = f(&mut storage);
		if let Some(v) = storage {
			Self::put_storage(store, k, v);
		}
		result
	}
	fn mutate_value<V: Encode + Decode + Sized, R, F: FnOnce(&mut Option<V>) -> R>(
		store: &[u8],
		f: F,
	) -> R {
		let mut storage = Self::get_value(store);
		let result = f(&mut storage);
		if let Some(v) = storage {
			Self::put_value(store, v);
		}
		result
	}
}

fn storage_key<K: Encode>(prefix: &[u8], store: &[u8], k: K) -> Vec<u8> {
	[prefix, store, &k.encode()].concat()
}

impl<T: MockPallet> MockPalletStorage for T {
	fn put_storage<K: Encode, V: Encode>(store: &[u8], k: K, v: V) {
		storage::hashed::put(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, k),
			&v,
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

	fn put_value<V: Encode>(store: &[u8], v: V) {
		storage::hashed::put(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, ()),
			&v,
		)
	}

	fn get_value<V: Decode + Sized>(store: &[u8]) -> Option<V> {
		storage::hashed::get(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, ()),
		)
	}
}
