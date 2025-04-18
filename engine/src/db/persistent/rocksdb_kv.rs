// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, WriteBatch, DB};
use serde::{de::DeserializeOwned, Serialize};

use anyhow::{Context, Result};

/// A static length prefix is used on the `DATA_COLUMN`
pub const PREFIX_SIZE: usize = 10;

/// Column family names
// All data is stored in `DATA_COLUMN` with a prefix for key spaces
const DATA_COLUMN: &str = "data";
// This column is for various metadata. No prefix is used.
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
			.with_context(|| format!("Failed to open database at: {}", db_path.display()))?;

		Ok(RocksDBKeyValueStore { db })
	}

	pub fn put_data<T: Serialize, K: Serialize>(
		&self,
		prefix: &[u8],
		key: &K,
		value: &T,
	) -> Result<()> {
		let key_with_prefix =
			[prefix, &bincode::serialize(key).expect("Serialization is not expected to fail.")]
				.concat();
		self.db
			.put_cf(
				get_data_column_handle(&self.db),
				key_with_prefix,
				bincode::serialize(value).expect("Serialization is not expected to fail"),
			)
			.context("Failed to write data to database.")
	}

	pub fn get_data<K: Serialize, T: DeserializeOwned>(
		&self,
		prefix: &[u8],
		key: &K,
	) -> Result<Option<T>> {
		let key_with_prefix =
			[prefix, &bincode::serialize(key).expect("Serialization is not expected to fail.")]
				.concat();

		self.db
			.get_cf(get_data_column_handle(&self.db), key_with_prefix)?
			.map(|data| bincode::deserialize(&data).context("Deserialization failed"))
			.transpose()
	}

	pub fn get_data_for_prefix<'a, K: DeserializeOwned, V: DeserializeOwned>(
		&'a self,
		prefix: &[u8],
	) -> impl Iterator<Item = (K, V)> + 'a {
		self.db
			.prefix_iterator_cf(get_data_column_handle(&self.db), prefix)
			.map(|result| result.expect("prefix iterator should not fail"))
			.map(|(key, value)| (Vec::from(&key[PREFIX_SIZE..]), value))
			.map(|(key, value)| {
				(
					bincode::deserialize(&key).expect("Deserialization is not expected to fail"),
					bincode::deserialize(&value).expect("Deserialization is not expected to fail"),
				)
			})
	}

	pub fn put_metadata<V>(&self, key: &[u8], value: V) -> Result<()>
	where
		V: AsRef<[u8]>,
	{
		self.db
			.put_cf(get_metadata_column_handle(&self.db), key, value)
			.context("Failed to write metadata to database.")
	}

	pub fn get_metadata(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.db
			.get_cf(get_metadata_column_handle(&self.db), key)
			.expect("metadata column must exist")
	}

	pub fn create_batch(&self) -> KVWriteBatch<'_> {
		KVWriteBatch { db: &self.db, batch: WriteBatch::default() }
	}
}

pub struct KVWriteBatch<'a> {
	db: &'a DB,
	batch: WriteBatch,
}

impl KVWriteBatch<'_> {
	#[allow(dead_code)]
	pub fn put_value(&mut self, key: &[u8], value: &[u8]) {
		self.batch.put_cf(get_data_column_handle(self.db), key, value);
	}

	#[allow(dead_code)]
	pub fn delete_value(&mut self, key: &[u8]) {
		self.batch.delete_cf(get_data_column_handle(self.db), key);
	}

	pub fn put_metadata<V>(&mut self, key: &[u8], value: V)
	where
		V: AsRef<[u8]>,
	{
		self.batch.put_cf(get_metadata_column_handle(self.db), key, value);
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
