#[cfg(test)]
mod persistent_key_db_tests;

use std::{cmp::Ordering, collections::HashMap, fs, mem::size_of, path::Path};

use cf_primitives::KeyId;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, WriteBatch, DB};
use serde::{de::DeserializeOwned, Serialize};
use tracing::{debug, info, info_span};

use crate::{
	multisig::{
		client::KeygenResultInfo, eth::EthSigning, polkadot::PolkadotSigning, ChainTag,
		CryptoScheme, CHAIN_TAG_SIZE,
	},
	witnesser::checkpointing::WitnessedUntil,
};

use anyhow::{anyhow, bail, Context, Result};

/// This is the version of the data on this current branch
/// This version *must* be bumped, and appropriate migrations
/// written on any changes to the persistent application data format
const LATEST_SCHEMA_VERSION: u32 = 1;

/// Key used to store the `LATEST_SCHEMA_VERSION` value in the `METADATA_COLUMN`
const DB_SCHEMA_VERSION_KEY: &[u8; 17] = b"db_schema_version";
const GENESIS_HASH_KEY: &[u8; 12] = b"genesis_hash";

/// A static length prefix is used on the `DATA_COLUMN`
const PREFIX_SIZE: usize = 10;
const PARTIAL_PREFIX_SIZE: usize = PREFIX_SIZE - CHAIN_TAG_SIZE;
/// Keygen data uses a prefix that is a combination of a keygen data prefix and the chain tag
const KEYGEN_DATA_PARTIAL_PREFIX: &[u8; PARTIAL_PREFIX_SIZE] = b"key_____";
/// The Witnesser checkpoint uses a prefix that is a combination of a checkpoint prefix and the
/// chain tag
const WITNESSER_CHECKPOINT_PARTIAL_PREFIX: &[u8; PARTIAL_PREFIX_SIZE] = b"check___";

/// Column family names
// All data is stored in `DATA_COLUMN` with a prefix for key spaces
const DATA_COLUMN: &str = "data";
// This column is just for schema version info. No prefix is used.
const METADATA_COLUMN: &str = "metadata";

/// Name of the directory that the backups will go into (only created before migrations)
const BACKUPS_DIRECTORY: &str = "backups";

struct RocksDBKeyValueStore {
	/// Rocksdb database instance
	db: DB,
}
/// Database for keys and persistent metadata
pub struct PersistentKeyDB {
	/// Underlying key-value database instance
	kv_db: RocksDBKeyValueStore,
}

fn put_schema_version_to_batch(db: &DB, batch: &mut WriteBatch, version: u32) {
	batch.put_cf(get_metadata_column_handle(db), DB_SCHEMA_VERSION_KEY, version.to_be_bytes());
}

impl RocksDBKeyValueStore {
	fn open_and_migrate_to_version(
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

		migrate_db_to_version(&db, backup_option, genesis_hash, version)
                    .with_context(|| format!("Failed to migrate database at {}. Manual restoration of a backup or purging of the file is required.", db_path.display()))?;

		Ok(RocksDBKeyValueStore { db })
	}

	fn put_data<T: Serialize>(&self, prefix: &[u8], key: &[u8], value: &T) -> Result<()> {
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

	fn get_data<T: DeserializeOwned>(&self, prefix: &[u8], key: &[u8]) -> Result<Option<T>> {
		let key_with_prefix = [prefix, key].concat();

		self.db
			.get_cf(get_data_column_handle(&self.db), key_with_prefix)?
			.map(|data| {
				bincode::deserialize(&data).map_err(|e| anyhow!("Deserialization failure: {}", e))
			})
			.transpose()
	}

	fn get_data_for_prefix<'a, T: DeserializeOwned>(
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

// Creates a backup of the database folder to BACKUPS_DIRECTORY/backup_vx_xx_xx
fn create_backup(path: &Path, schema_version: u32) -> Result<String, anyhow::Error> {
	// Build the name for the new backup using the schema version and a timestamp
	let backup_dir_name = format!(
		"backup_v{}_{}_{}",
		schema_version,
		chrono::Utc::now().to_rfc3339(),
		&path
			.file_name()
			.expect("Should have file name")
			.to_os_string()
			.into_string()
			.expect("Should get string from filename"),
	);

	create_backup_with_directory_name(path, backup_dir_name)
}

fn create_backup_with_directory_name(
	path: &Path,
	backup_dir_name: String,
) -> Result<String, anyhow::Error> {
	// Create the BACKUPS_DIRECTORY if it doesn't exist
	let backups_path = path.parent().expect("Should have parent");
	let backups_path = backups_path.join(BACKUPS_DIRECTORY);
	if !backups_path.exists() {
		fs::create_dir_all(&backups_path).map_err(anyhow::Error::msg).with_context(|| {
			format!(
				"Failed to create backup directory {}",
				&backups_path.to_str().expect("Should get backup path as str")
			)
		})?;
	}

	// This db backup folder should not exist yet
	let backup_dir_path = backups_path.join(backup_dir_name);
	if backup_dir_path.exists() {
		bail!("Backup directory already exists {}", backup_dir_path.display());
	}

	// Copy the files
	let mut copy_options = fs_extra::dir::CopyOptions::new();
	copy_options.copy_inside = true;
	fs_extra::dir::copy(path, &backup_dir_path, &copy_options)
		.map_err(anyhow::Error::msg)
		.context("Failed to copy db files for backup")?;

	Ok(backup_dir_path
		.into_os_string()
		.into_string()
		.expect("Should get backup path as string"))
}

fn get_metadata_column_handle(db: &DB) -> &ColumnFamily {
	get_column_handle(db, METADATA_COLUMN)
}

fn get_data_column_handle(db: &DB) -> &ColumnFamily {
	get_column_handle(db, DATA_COLUMN)
}

fn get_column_handle<'a>(db: &'a DB, column_name: &str) -> &'a ColumnFamily {
	db.cf_handle(column_name)
		.unwrap_or_else(|| panic!("Should get column family handle for {column_name}"))
}

/// Get the schema version from the metadata column in the db.
fn read_schema_version(db: &DB) -> Result<u32> {
	db.get_cf(get_metadata_column_handle(db), DB_SCHEMA_VERSION_KEY)
		.context("Failed to get metadata column")?
		.map(|version| {
			let version: [u8; 4] = version.try_into().expect("Version should be a u32");
			u32::from_be_bytes(version)
		})
		.ok_or_else(|| anyhow!("Could not find db schema version"))
}

/// Get the genesis hash from the metadata column in the db.
fn read_genesis_hash(db: &DB) -> Result<Option<state_chain_runtime::Hash>> {
	match db
		.get_cf(get_metadata_column_handle(db), GENESIS_HASH_KEY)
		.context("Failed to get metadata column")?
	{
		Some(hash) =>
			if hash.len() != size_of::<state_chain_runtime::Hash>() {
				Err(anyhow!("Incorrect length of Genesis hash"))
			} else {
				Ok(Some(sp_core::H256::from_slice(&hash[..])))
			},
		// None is expected because the genesis hash is not known during the generate genesis keys
		// process, so the genesis databases will not have the genesis hash,
		// it will be added during `check_or_set_genesis_hash` on first time startup.
		None => Ok(None),
	}
}

/// Check that the genesis in the db file matches the one provided.
/// If None is found, it will be added to the db.
fn check_or_set_genesis_hash(db: &DB, genesis_hash: state_chain_runtime::Hash) -> Result<()> {
	let existing_hash = read_genesis_hash(db)?;

	match existing_hash {
		Some(existing_hash) =>
			if existing_hash == genesis_hash {
				Ok(())
			} else {
				Err(anyhow!("Genesis hash mismatch. Have you changed Chainflip network?",))
			},
		None => {
			db.put_cf(get_metadata_column_handle(db), GENESIS_HASH_KEY, genesis_hash)
				.context("Failed to write genesis hash to db")?;

			Ok(())
		},
	}
}

/// Used to specify whether a backup should be created, and if so,
/// the provided path is used to derive the name of the backup
enum BackupOption<'a> {
	NoBackup,
	CreateBackup(&'a Path),
}

/// Reads the schema version and migrates the db to the latest schema version if required
fn migrate_db_to_version(
	db: &DB,
	backup_option: BackupOption,
	genesis_hash: Option<state_chain_runtime::Hash>,
	target_version: u32,
) -> Result<(), anyhow::Error> {
	let current_version =
		read_schema_version(db).context("Failed to read schema version from existing db")?;

	info!("Found db_schema_version of {current_version}");

	if let Some(expected_genesis_hash) = genesis_hash {
		check_or_set_genesis_hash(db, expected_genesis_hash)?;
	}

	// Check if the db version is up-to-date or we need to do migrations
	match current_version.cmp(&target_version) {
		Ordering::Equal => {
			info!("Database already at target version of: {target_version}");
			Ok(())
		},
		Ordering::Greater => {
			// We do not support backwards migrations
			Err(anyhow!("Database schema version {} is ahead of the current schema version {}. Is your Chainflip Engine up to date?",
                    current_version,
                    target_version)
                )
		},
		Ordering::Less => {
			// If requested, backup the database before migrating it
			if let BackupOption::CreateBackup(path) = backup_option {
				info!(
					"Database backup created at {}",
					create_backup(path, current_version)
						.map_err(anyhow::Error::msg)
						.context("Failed to create database backup before migration")?
				);
			}

			for version in current_version..target_version {
				info!("Database is migrating from version {version} to {}", version + 1);

				match version {
					0 => {
						migrate_0_to_1(db);
					},
					_ => {
						panic!("Unexpected migration from version {version}");
					},
				}
			}

			Ok(())
		},
	}
}

fn migrate_0_to_1_for_scheme<C: CryptoScheme>(db: &DB, batch: &mut WriteBatch) {
	for (legacy_key_id_with_prefix, key_info_bytes) in db
		.prefix_iterator_cf(get_data_column_handle(db), get_keygen_data_prefix::<C>())
		.map(|result| result.expect("should successfully read all items"))
	{
		let new_key_id = KeyId {
			epoch_index: 0,
			public_key_bytes: legacy_key_id_with_prefix[PREFIX_SIZE..].to_vec(),
		};
		let key_id_with_prefix =
			[get_keygen_data_prefix::<C>().as_slice(), &new_key_id.to_bytes()].concat();
		batch.put_cf(get_data_column_handle(db), key_id_with_prefix, key_info_bytes);

		batch.delete_cf(get_data_column_handle(db), legacy_key_id_with_prefix);
	}
}

fn migrate_0_to_1(db: &DB) {
	let mut batch = WriteBatch::default();

	// Do the migration for every scheme that we supported
	// until schema version 1:
	migrate_0_to_1_for_scheme::<EthSigning>(db, &mut batch);
	migrate_0_to_1_for_scheme::<PolkadotSigning>(db, &mut batch);

	put_schema_version_to_batch(db, &mut batch, 1);

	db.write(batch).unwrap();
}
