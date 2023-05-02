mod rocksdb_kv;
#[cfg(test)]
mod tests;

use std::{cmp::Ordering, collections::HashMap, path::Path};

use cf_primitives::KeyId;
use tracing::{debug, info, info_span};

use crate::witnesser::checkpointing::WitnessedUntil;
use multisig::{
	client::KeygenResultInfo, eth::EthSigning, polkadot::PolkadotSigning, ChainTag, CryptoScheme,
	CHAIN_TAG_SIZE,
};

use anyhow::{anyhow, bail, Context, Result};

use rocksdb_kv::{KVWriteBatch, RocksDBKeyValueStore, PREFIX_SIZE};

/// Name of the directory that the backups will go into (only created before migrations)
const BACKUPS_DIRECTORY: &str = "backups";

/// This is the version of the data on this current branch
/// This version *must* be bumped, and appropriate migrations
/// written on any changes to the persistent application data format
const LATEST_SCHEMA_VERSION: u32 = 1;

const PARTIAL_PREFIX_SIZE: usize = PREFIX_SIZE - CHAIN_TAG_SIZE;

/// Keygen data uses a prefix that is a combination of a keygen data prefix and the chain tag
const KEYGEN_DATA_PARTIAL_PREFIX: &[u8; PARTIAL_PREFIX_SIZE] = b"key_____";
/// The Witnesser checkpoint uses a prefix that is a combination of a checkpoint prefix and the
/// chain tag
const WITNESSER_CHECKPOINT_PARTIAL_PREFIX: &[u8; PARTIAL_PREFIX_SIZE] = b"check___";

/// Key used to store the `LATEST_SCHEMA_VERSION` value in the `METADATA_COLUMN`
const DB_SCHEMA_VERSION_KEY: &[u8; 17] = b"db_schema_version";
const GENESIS_HASH_KEY: &[u8; 12] = b"genesis_hash";

/// Used to specify whether a backup should be created, and if so,
/// the provided path is used to derive the name of the backup
enum BackupOption<'a> {
	NoBackup,
	CreateBackup(&'a Path),
}

/// Database for keys and persistent metadata
pub struct PersistentKeyDB {
	/// Underlying key-value database instance
	kv_db: RocksDBKeyValueStore,
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
		let is_existing_db = db_path.exists();

		let db = PersistentKeyDB { kv_db: RocksDBKeyValueStore::open(db_path)? };

		// Only create a backup if there is an existing db that we don't
		// want to accidentally corrupt
		let backup_option = if is_existing_db {
			BackupOption::CreateBackup(db_path)
		} else {
			let mut batch = db.kv_db.create_batch();

			batch.put_metadata(DB_SCHEMA_VERSION_KEY, 0u32.to_be_bytes());

			if let Some(genesis_hash) = genesis_hash {
				batch.put_metadata(GENESIS_HASH_KEY, genesis_hash);
			}

			batch.write().context("Failed to write metadata to new db")?;
			BackupOption::NoBackup
		};

		migrate_db_to_version(&db, backup_option, genesis_hash, version).with_context(|| {
			format!(
				"Failed to migrate database at {}. Manual restoration of a backup or purging of the file is required.",
				db_path.display()
			)
		})?;
		Ok(db)
	}

	/// Write the keyshare to the db, indexed by the key id
	pub fn update_key<C: CryptoScheme>(
		&self,
		key_id: &KeyId,
		keygen_result_info: &KeygenResultInfo<C>,
	) {
		self.kv_db
			.put_data(&keygen_data_prefix::<C>(), &key_id.to_bytes(), &keygen_result_info)
			.unwrap_or_else(|e| panic!("Failed to update key {}. Error: {}", &key_id, e));
	}

	pub fn load_keys<C: CryptoScheme>(&self) -> HashMap<KeyId, KeygenResultInfo<C>> {
		let span = info_span!("PersistentKeyDB");
		let _entered = span.enter();

		let keys: HashMap<_, _> = self
			.kv_db
			.get_data_for_prefix(&keygen_data_prefix::<C>())
			.map(|(key_id, key_bytes)| {
				(
					KeyId::from_bytes(&key_id),
					bincode::deserialize(&key_bytes).unwrap_or_else(|e| {
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
			.put_data(&checkpoint_prefix(chain_tag), &[], checkpoint)
			.unwrap_or_else(|e| {
				panic!("Failed to update {chain_tag} witnesser checkpoint. Error: {e}")
			});
	}

	pub fn load_checkpoint(&self, chain_tag: ChainTag) -> Result<Option<WitnessedUntil>> {
		self.kv_db
			.get_data(&checkpoint_prefix(chain_tag), &[])
			.context("Failed to load {chain_tag} checkpoint")
	}

	/// Get the genesis hash from the metadata column in the db.
	pub fn get_genesis_hash(&self) -> Result<Option<state_chain_runtime::Hash>> {
		match self.kv_db.get_metadata(GENESIS_HASH_KEY) {
			Some(hash) =>
				if hash.len() != std::mem::size_of::<state_chain_runtime::Hash>() {
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
		self.kv_db.put_metadata(GENESIS_HASH_KEY, genesis_hash)
	}

	#[cfg(test)]
	pub fn put_schema_version(&self, version: u32) -> Result<()> {
		self.kv_db.put_metadata(DB_SCHEMA_VERSION_KEY, version.to_be_bytes())
	}

	pub fn get_schema_version(&self) -> Result<u32> {
		self.kv_db
			.get_metadata(DB_SCHEMA_VERSION_KEY)
			.map(|version| {
				let version: [u8; 4] = version.try_into().expect("Version should be a u32");
				u32::from_be_bytes(version)
			})
			.ok_or_else(|| anyhow!("Could not find db schema version"))
	}
}

fn keygen_data_prefix<C: CryptoScheme>() -> Vec<u8> {
	[&KEYGEN_DATA_PARTIAL_PREFIX[..], &(C::CHAIN_TAG.to_bytes())[..]].concat()
}

fn checkpoint_prefix(chain_tag: ChainTag) -> Vec<u8> {
	[WITNESSER_CHECKPOINT_PARTIAL_PREFIX, &(chain_tag.to_bytes())[..]].concat()
}

/// Reads the schema version and migrates the db to the latest schema version if required
fn migrate_db_to_version(
	db: &PersistentKeyDB,
	backup_option: BackupOption,
	genesis_hash: Option<state_chain_runtime::Hash>,
	target_version: u32,
) -> Result<(), anyhow::Error> {
	let current_version = db
		.get_schema_version()
		.context("Failed to read schema version from existing db")?;

	info!("Found db_schema_version of {current_version}");

	if let Some(provided_genesis_hash) = genesis_hash {
		// Check that the genesis in the db file matches the one provided.
		// If None is found, it will be added to the db.
		let existing_hash = db.get_genesis_hash()?;

		match existing_hash {
			Some(existing_hash) =>
				if existing_hash != provided_genesis_hash {
					bail!("Genesis hash mismatch. Have you changed Chainflip network?",)
				},
			None => db.put_genesis_hash(provided_genesis_hash)?,
		}
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

fn migrate_0_to_1_for_scheme<C: CryptoScheme>(db: &PersistentKeyDB, batch: &mut KVWriteBatch) {
	for (legacy_key_id, key_info_bytes) in db.kv_db.get_data_for_prefix(&keygen_data_prefix::<C>())
	{
		let new_key_id = KeyId { epoch_index: 0, public_key_bytes: legacy_key_id.to_vec() };
		let key_id_with_prefix =
			[keygen_data_prefix::<C>().as_slice(), &new_key_id.to_bytes()].concat();

		batch.put_value(&key_id_with_prefix, &key_info_bytes);

		let legacy_key_id_with_prefix =
			[keygen_data_prefix::<C>().as_slice(), &legacy_key_id].concat();

		batch.delete_value(&legacy_key_id_with_prefix);
	}
}

fn migrate_0_to_1(db: &PersistentKeyDB) {
	let mut batch = db.kv_db.create_batch();

	// Do the migration for every scheme that we supported
	// until schema version 1:
	migrate_0_to_1_for_scheme::<EthSigning>(db, &mut batch);
	migrate_0_to_1_for_scheme::<PolkadotSigning>(db, &mut batch);

	batch.put_metadata(DB_SCHEMA_VERSION_KEY, 1u32.to_be_bytes());

	batch.write().expect("batch write should not fail");
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
		std::fs::create_dir_all(&backups_path)
			.map_err(anyhow::Error::msg)
			.with_context(|| {
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
