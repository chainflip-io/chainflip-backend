#[cfg(test)]
mod persistent_key_db_tests;
mod rocksdb_kv;

use rocksdb_kv::PARTIAL_PREFIX_SIZE;
use std::{collections::HashMap, path::Path};

use cf_primitives::KeyId;
use tracing::{debug, info_span};

use crate::{
	multisig::{client::KeygenResultInfo, ChainTag, CryptoScheme},
	witnesser::checkpointing::WitnessedUntil,
};

use anyhow::{Context, Result};

use rocksdb_kv::RocksDBKeyValueStore;

/// This is the version of the data on this current branch
/// This version *must* be bumped, and appropriate migrations
/// written on any changes to the persistent application data format
const LATEST_SCHEMA_VERSION: u32 = 1;

/// Keygen data uses a prefix that is a combination of a keygen data prefix and the chain tag
const KEYGEN_DATA_PARTIAL_PREFIX: &[u8; PARTIAL_PREFIX_SIZE] = b"key_____";
/// The Witnesser checkpoint uses a prefix that is a combination of a checkpoint prefix and the
/// chain tag
const WITNESSER_CHECKPOINT_PARTIAL_PREFIX: &[u8; PARTIAL_PREFIX_SIZE] = b"check___";

/// Database for keys and persistent metadata
pub struct PersistentKeyDB {
	/// Underlying key-value database instance
	pub kv_db: RocksDBKeyValueStore,
}

impl PersistentKeyDB {
	/// Open a key database or create one if it doesn't exist. If the schema version of the
	/// existing database is below the latest, it will attempt to migrate to the latest version.
	pub fn open_and_migrate_to_latest(
		db_path: &Path,
		genesis_hash: Option<state_chain_runtime::Hash>,
	) -> Result<Self> {
		let span = info_span!("PersistentKeyDB");
		let _entered = span.enter();

		Self::open_and_migrate_to_version(db_path, genesis_hash, LATEST_SCHEMA_VERSION)
	}

	/// As [Self::open_and_migrate_to_latest], but allows specifying a specific version
	/// to migrate to (useful for testing migrations)
	fn open_and_migrate_to_version(
		db_path: &Path,
		genesis_hash: Option<state_chain_runtime::Hash>,
		version: u32,
	) -> Result<Self> {
		Ok(PersistentKeyDB {
			kv_db: RocksDBKeyValueStore::open_and_migrate_to_version(
				db_path,
				genesis_hash,
				version,
			)?,
		})
	}

	/// Write the keyshare to the db, indexed by the key id
	pub fn update_key<C: CryptoScheme>(
		&self,
		key_id: &KeyId,
		keygen_result_info: &KeygenResultInfo<C>,
	) {
		self.kv_db
			.put_data(&get_keygen_data_prefix::<C>(), &key_id.to_bytes(), &keygen_result_info)
			.unwrap_or_else(|e| panic!("Failed to update key {}. Error: {}", &key_id, e));
	}

	pub fn load_keys<C: CryptoScheme>(&self) -> HashMap<KeyId, KeygenResultInfo<C>> {
		let span = info_span!("PersistentKeyDB");
		let _entered = span.enter();

		let keys: HashMap<_, _> = self
			.kv_db
			.get_data_for_prefix::<KeygenResultInfo<C>>(&get_keygen_data_prefix::<C>())
			.map(|(key_id, key_info_result)| {
				(
					KeyId::from_bytes(&key_id),
					key_info_result.unwrap_or_else(|e| {
						panic!("Failed to deserialize {} key from database: {}", C::NAME, e)
					}),
				)
			})
			.collect();
		if !keys.is_empty() {
			debug!("Loaded {} {} keys from the database", keys.len(), C::NAME);
		}
		keys
	}

	/// Write the witnesser checkpoint to the db
	pub fn update_checkpoint(&self, chain_tag: ChainTag, checkpoint: &WitnessedUntil) {
		self.kv_db
			.put_data(&get_checkpoint_prefix(chain_tag), &[], checkpoint)
			.unwrap_or_else(|e| {
				panic!("Failed to update {chain_tag} witnesser checkpoint. Error: {e}")
			});
	}

	pub fn load_checkpoint(&self, chain_tag: ChainTag) -> Result<Option<WitnessedUntil>> {
		self.kv_db
			.get_data(&get_checkpoint_prefix(chain_tag), &[])
			.context("Failed to load {chain_tag} checkpoint")
	}
}

fn get_keygen_data_prefix<C: CryptoScheme>() -> Vec<u8> {
	[&KEYGEN_DATA_PARTIAL_PREFIX[..], &(C::CHAIN_TAG.to_bytes())[..]].concat()
}

fn get_checkpoint_prefix(chain_tag: ChainTag) -> Vec<u8> {
	[WITNESSER_CHECKPOINT_PARTIAL_PREFIX, &(chain_tag.to_bytes())[..]].concat()
}
