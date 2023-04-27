use std::{mem::size_of, path::Path};

use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, WriteBatch, DB};
use serde::{de::DeserializeOwned, Serialize};

use anyhow::{anyhow, Context, Result};

/// Key used to store the `LATEST_SCHEMA_VERSION` value in the `METADATA_COLUMN`
const DB_SCHEMA_VERSION_KEY: &[u8; 17] = b"db_schema_version";
const GENESIS_HASH_KEY: &[u8; 12] = b"genesis_hash";

/// A static length prefix is used on the `DATA_COLUMN`
pub const PREFIX_SIZE: usize = 10;

/// Column family names
// All data is stored in `DATA_COLUMN` with a prefix for key spaces
const DATA_COLUMN: &str = "data";
// This column is just for schema version info. No prefix is used.
const METADATA_COLUMN: &str = "metadata";

pub struct RocksDBKeyValueStore {
	/// Rocksdb database instance
	pub db: DB,
}

impl RocksDBKeyValueStore {
	pub fn open(db_path: &Path) -> Result<Self> {
		let column_families = {
			// Use a prefix extractor on the data column
			let mut cfopts_for_prefix = Options::default();
			cfopts_for_prefix
				.set_prefix_extractor(rocksdb::SliceTransform::create_fixed_prefix(PREFIX_SIZE));

			// Build a list of column families with descriptors
			[
				ColumnFamilyDescriptor::new(METADATA_COLUMN, Options::default()),
				ColumnFamilyDescriptor::new(DATA_COLUMN, cfopts_for_prefix),
			]
		};

		let open_options = {
			let mut options = Options::default();
			options.create_missing_column_families(true);
			options.create_if_missing(true);
			options
		};

		// Open the db or create a new one if it doesn't exist
		let db = DB::open_cf_descriptors(&open_options, db_path, column_families)
			.map_err(anyhow::Error::msg)
			.context(format!("Failed to open database at: {}", db_path.display()))?;

		Ok(RocksDBKeyValueStore { db })
	}

	pub fn put_data<T: Serialize>(&self, prefix: &[u8], key: &[u8], value: &T) -> Result<()> {
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

	pub fn get_data_for_prefix<'a>(
		&'a self,
		prefix: &[u8],
	) -> impl Iterator<Item = (Vec<u8>, Box<[u8]>)> + 'a {
		self.db
			.prefix_iterator_cf(get_data_column_handle(&self.db), prefix)
			.map(|result| result.expect("prefix iterator should not fail"))
			.map(|(key, value)| (Vec::from(&key[PREFIX_SIZE..]), value))
	}

	pub fn put_metadata<V>(&self, key: &[u8], value: V) -> Result<()>
	where
		V: AsRef<[u8]>,
	{
		self.db.put_cf(get_metadata_column_handle(&self.db), key, value).map_err(|e| {
			anyhow::anyhow!("Failed to write metadata to database. Error: {}", e.to_string())
		})
	}

	pub fn get_metadata(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.db
			.get_cf(get_metadata_column_handle(&self.db), key)
			.expect("metadata column must exist")
	}

	pub fn get_schema_version(&self) -> Result<u32> {
		self.get_metadata(DB_SCHEMA_VERSION_KEY)
			.map(|version| {
				let version: [u8; 4] = version.try_into().expect("Version should be a u32");
				u32::from_be_bytes(version)
			})
			.ok_or_else(|| anyhow!("Could not find db schema version"))
	}

	#[cfg(test)]
	pub fn put_schema_version(&self, version: u32) -> Result<()> {
		self.put_metadata(DB_SCHEMA_VERSION_KEY, &version.to_be_bytes())
	}

	/// Get the genesis hash from the metadata column in the db.
	pub fn get_genesis_hash(&self) -> Result<Option<state_chain_runtime::Hash>> {
		match self.get_metadata(GENESIS_HASH_KEY) {
			Some(hash) =>
				if hash.len() != size_of::<state_chain_runtime::Hash>() {
					Err(anyhow!("Incorrect length of Genesis hash"))
				} else {
					Ok(Some(sp_core::H256::from_slice(&hash[..])))
				},
			// None is expected because the genesis hash is not known during the generate genesis
			// keys process, so the genesis databases will not have the genesis hash,
			// it will be added on first time startup.
			None => Ok(None),
		}
	}

	pub fn put_genesis_hash(&self, genesis_hash: state_chain_runtime::Hash) -> Result<()> {
		self.put_metadata(GENESIS_HASH_KEY, genesis_hash)
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
	pub fn put_value(&mut self, key: &[u8], value: &[u8]) {
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

	pub fn put_genesis_hash(&mut self, genesis_hash: state_chain_runtime::Hash) {
		self.batch
			.put_cf(get_metadata_column_handle(&self.db), GENESIS_HASH_KEY, genesis_hash)
	}

	pub fn write(self) -> anyhow::Result<()> {
		self.db.write(self.batch).context("failed to write batch")
	}
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
