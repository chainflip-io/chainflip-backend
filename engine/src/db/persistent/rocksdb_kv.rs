#[cfg(test)]
mod rocksdb_kv_tests;

use std::{collections::HashMap, mem::size_of, path::Path};

use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, WriteBatch, DB};
use serde::{de::DeserializeOwned, Serialize};

// TODO: move everything to do with migration out of this module,
// then we could remove all dependencies on the business logic
use crate::multisig::CHAIN_TAG_SIZE;

use anyhow::{anyhow, Context, Result};

/// Key used to store the `LATEST_SCHEMA_VERSION` value in the `METADATA_COLUMN`
pub const DB_SCHEMA_VERSION_KEY: &[u8; 17] = b"db_schema_version";
const GENESIS_HASH_KEY: &[u8; 12] = b"genesis_hash";

/// A static length prefix is used on the `DATA_COLUMN`
pub const PREFIX_SIZE: usize = 10;
pub const PARTIAL_PREFIX_SIZE: usize = PREFIX_SIZE - CHAIN_TAG_SIZE;

/// Column family names
// All data is stored in `DATA_COLUMN` with a prefix for key spaces
pub const DATA_COLUMN: &str = "data";
// This column is just for schema version info. No prefix is used.
pub const METADATA_COLUMN: &str = "metadata";

pub struct RocksDBKeyValueStore {
	/// Rocksdb database instance
	pub db: DB,
}

/// Used to specify whether a backup should be created, and if so,
/// the provided path is used to derive the name of the backup
pub enum BackupOption<'a> {
	NoBackup,
	CreateBackup(&'a Path),
}

impl RocksDBKeyValueStore {
	pub fn open(
		db_path: &Path,
		genesis_hash: Option<state_chain_runtime::Hash>,
		version: u32,
	) -> Result<Self> {
		let is_existing_db = db_path.exists();

		// Use a prefix extractor on the data column
		let mut cfopts_for_prefix = Options::default();
		cfopts_for_prefix
			.set_prefix_extractor(rocksdb::SliceTransform::create_fixed_prefix(PREFIX_SIZE));

		// Build a list of column families with descriptors
		let cfs: HashMap<String, ColumnFamilyDescriptor> = HashMap::from_iter([
			(
				METADATA_COLUMN.to_string(),
				ColumnFamilyDescriptor::new(METADATA_COLUMN, Options::default()),
			),
			(DATA_COLUMN.to_string(), ColumnFamilyDescriptor::new(DATA_COLUMN, cfopts_for_prefix)),
		]);

		let mut create_missing_db_and_cols_opts = Options::default();
		create_missing_db_and_cols_opts.create_missing_column_families(true);
		create_missing_db_and_cols_opts.create_if_missing(true);

		// Open the db or create a new one if it doesn't exist
		let db =
			DB::open_cf_descriptors(&create_missing_db_and_cols_opts, db_path, cfs.into_values())
				.map_err(anyhow::Error::msg)
				.context(format!("Failed to open database at: {}", db_path.display()))?;

		// Only create a backup if there is an existing db that we don't
		// want to accidentally corrupt
		let backup_option = if is_existing_db {
			BackupOption::CreateBackup(db_path)
		} else {
			let mut batch = WriteBatch::default();

			put_schema_version_to_batch(&db, &mut batch, 0);

			if let Some(genesis_hash) = genesis_hash {
				batch.put_cf(get_metadata_column_handle(&db), GENESIS_HASH_KEY, genesis_hash);
			}

			db.write(batch).context("Failed to write metadata to new db")?;
			BackupOption::NoBackup
		};

		Ok(RocksDBKeyValueStore { db })
	}

	pub fn put_data<T: Serialize>(&self, prefix: &[u8], key: &[u8], value: &T) -> Result<()> {
		println!("saving key with prefix: {}", hex::encode(prefix));
		println!("  the key: {}", hex::encode(key));
		let key_with_prefix = [prefix, key].concat();
		self.db
			.put_cf(
				get_data_column_handle(&self.db),
				key_with_prefix,
				bincode::serialize(value).expect("Serialization is not expected to fail"),
			)
			.map_err(|e| {
				anyhow::anyhow!("Failed to write data to database. Error: {}", e.to_string())
			})
	}

	pub fn get_data<T: DeserializeOwned>(&self, prefix: &[u8], key: &[u8]) -> Result<Option<T>> {
		let key_with_prefix = [prefix, key].concat();

		self.db
			.get_cf(get_data_column_handle(&self.db), key_with_prefix)?
			.map(|data| {
				bincode::deserialize(&data).map_err(|e| anyhow!("Deserialization failure: {}", e))
			})
			.transpose()
	}

	pub fn get_data_for_prefix<'a, T: DeserializeOwned>(
		&'a self,
		prefix: &[u8],
	) -> impl Iterator<Item = (Vec<u8>, Result<T>)> + 'a {
		self.db
			.prefix_iterator_cf(get_data_column_handle(&self.db), prefix)
			.map(|result| result.expect("prefix iterator should not fail"))
			.map(|(key, value)| {
				(
					Vec::from(&key[PREFIX_SIZE..]),
					bincode::deserialize(&value)
						.map_err(|e| anyhow!("Deserialization failure: {}", e)),
				)
			})
	}

	pub fn get_data_for_prefix_bytes<'a>(
		&'a self,
		prefix: &[u8],
	) -> impl Iterator<Item = (Vec<u8>, Box<[u8]>)> + 'a {
		self.db
			.prefix_iterator_cf(get_data_column_handle(&self.db), prefix)
			.map(|result| result.expect("prefix iterator should not fail"))
			.map(|(key, value)| (Vec::from(&key[PREFIX_SIZE..]), value))
	}

	pub fn get_schema_version(&self) -> Result<u32> {
		self.db
			.get_cf(get_metadata_column_handle(&self.db), DB_SCHEMA_VERSION_KEY)
			.context("Failed to get metadata column")?
			.map(|version| {
				let version: [u8; 4] = version.try_into().expect("Version should be a u32");
				u32::from_be_bytes(version)
			})
			.ok_or_else(|| anyhow!("Could not find db schema version"))
	}

	pub fn put_schema_version(&self, version: u32) -> Result<()> {
		self.db
			.put_cf(
				get_metadata_column_handle(&self.db),
				DB_SCHEMA_VERSION_KEY,
				version.to_be_bytes(),
			)
			.map_err(|e| {
				anyhow::anyhow!("Failed to write data to database. Error: {}", e.to_string())
			})
	}

	/// Check that the genesis in the db file matches the one provided.
	/// If None is found, it will be added to the db.
	pub fn check_or_set_genesis_hash(&self, genesis_hash: state_chain_runtime::Hash) -> Result<()> {
		let existing_hash = self.get_genesis_hash()?;

		match existing_hash {
			Some(existing_hash) =>
				if existing_hash == genesis_hash {
					Ok(())
				} else {
					Err(anyhow!("Genesis hash mismatch. Have you changed Chainflip network?",))
				},
			None => {
				// TODO: add put_metadata method?
				self.db
					.put_cf(get_metadata_column_handle(&self.db), GENESIS_HASH_KEY, genesis_hash)
					.context("Failed to write genesis hash to db")?;

				Ok(())
			},
		}
	}

	/// Get the genesis hash from the metadata column in the db.
	pub fn get_genesis_hash(&self) -> Result<Option<state_chain_runtime::Hash>> {
		match self
			.db
			.get_cf(get_metadata_column_handle(&self.db), GENESIS_HASH_KEY)
			.context("Failed to get metadata column")?
		{
			Some(hash) =>
				if hash.len() != size_of::<state_chain_runtime::Hash>() {
					Err(anyhow!("Incorrect length of Genesis hash"))
				} else {
					Ok(Some(sp_core::H256::from_slice(&hash[..])))
				},
			// None is expected because the genesis hash is not known during the generate genesis
			// keys process, so the genesis databases will not have the genesis hash,
			// it will be added during `check_or_set_genesis_hash` on first time startup.
			None => Ok(None),
		}
	}

	pub fn create_batch<'a>(&'a self) -> KVWriteBatch<'a> {
		KVWriteBatch { db: &self.db, batch: WriteBatch::default() }
	}
}

pub struct KVWriteBatch<'a> {
	db: &'a DB,
	batch: WriteBatch,
}

impl<'a> KVWriteBatch<'a> {
	pub fn put_value<T: Serialize>(&mut self, key: &[u8], value: &T) {
		self.batch.put_cf(
			get_data_column_handle(&self.db),
			key,
			bincode::serialize(value).expect("Serialization is not expected to fail"),
		);
	}

	pub fn put_bytes(&mut self, key: &[u8], value: &[u8]) {
		self.batch.put_cf(get_data_column_handle(&self.db), key, value);
	}

	pub fn delete_value(&mut self, key: &[u8]) {
		self.batch.delete_cf(get_data_column_handle(&self.db), key);
	}

	pub fn put_schema_version(&mut self, version: u32) {
		self.batch.put_cf(
			get_metadata_column_handle(&self.db),
			DB_SCHEMA_VERSION_KEY,
			version.to_be_bytes(),
		);
	}

	pub fn write(self) -> anyhow::Result<()> {
		// TODO: handle the error
		Ok(self.db.write(self.batch).unwrap())
	}
}

fn put_schema_version_to_batch(db: &DB, batch: &mut WriteBatch, version: u32) {
	batch.put_cf(get_metadata_column_handle(db), DB_SCHEMA_VERSION_KEY, version.to_be_bytes());
}

fn get_data_column_handle(db: &DB) -> &ColumnFamily {
	get_column_handle(db, DATA_COLUMN)
}

fn get_metadata_column_handle(db: &DB) -> &ColumnFamily {
	get_column_handle(db, METADATA_COLUMN)
}

fn get_column_handle<'a>(db: &'a DB, column_name: &str) -> &'a ColumnFamily {
	db.cf_handle(column_name)
		.unwrap_or_else(|| panic!("Should get column family handle for {column_name}"))
}
