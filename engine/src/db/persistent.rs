#[cfg(test)]
mod persistent_key_db_tests;
mod rocksdb_kv;

use rocksdb_kv::PARTIAL_PREFIX_SIZE;
use std::{cmp::Ordering, collections::HashMap, path::Path};

use cf_primitives::KeyId;
use tracing::{debug, info, info_span};

use crate::{
	multisig::{
		client::KeygenResultInfo, eth::EthSigning, polkadot::PolkadotSigning, ChainTag,
		CryptoScheme,
	},
	witnesser::checkpointing::WitnessedUntil,
};

use anyhow::{anyhow, bail, Context, Result};

use rocksdb_kv::RocksDBKeyValueStore;

use self::rocksdb_kv::{BackupOption, KVWriteBatch};

/// Name of the directory that the backups will go into (only created before migrations)
pub const BACKUPS_DIRECTORY: &str = "backups";

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
		let db =
			PersistentKeyDB { kv_db: RocksDBKeyValueStore::open(db_path, genesis_hash, version)? };

		// TODO: use the correct `backup_option`
		let backup_option = BackupOption::CreateBackup(db_path);
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

// ----------- migrations -----------

/// Reads the schema version and migrates the db to the latest schema version if required
pub fn migrate_db_to_version(
	db: &PersistentKeyDB,
	backup_option: BackupOption,
	genesis_hash: Option<state_chain_runtime::Hash>,
	target_version: u32,
) -> Result<(), anyhow::Error> {
	let current_version = db
		.kv_db
		.get_schema_version()
		.context("Failed to read schema version from existing db")?;

	info!("Found db_schema_version of {current_version}");

	if let Some(expected_genesis_hash) = genesis_hash {
		db.kv_db.check_or_set_genesis_hash(expected_genesis_hash)?;
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
	for (legacy_key_id, key_info_bytes) in db
		.kv_db
		// TODO: this should just return [u8] always instead of trying to deserialize
		.get_data_for_prefix_bytes(&get_keygen_data_prefix::<C>())
	{
		let new_key_id = KeyId { epoch_index: 0, public_key_bytes: legacy_key_id.to_vec() };
		let key_id_with_prefix =
			[get_keygen_data_prefix::<C>().as_slice(), &new_key_id.to_bytes()].concat();

		batch.put_bytes(&key_id_with_prefix, &key_info_bytes);

		let legacy_key_id_with_prefix =
			[get_keygen_data_prefix::<C>().as_slice(), &legacy_key_id].concat();

		batch.delete_value(&legacy_key_id_with_prefix);
	}
}

pub fn migrate_0_to_1(db: &PersistentKeyDB) {
	let mut batch = db.kv_db.create_batch();

	// Do the migration for every scheme that we supported
	// until schema version 1:
	migrate_0_to_1_for_scheme::<EthSigning>(db, &mut batch);
	migrate_0_to_1_for_scheme::<PolkadotSigning>(db, &mut batch);

	batch.put_schema_version(1);

	batch.write().expect("batch write should not fail");
}

// Creates a backup of the database folder to BACKUPS_DIRECTORY/backup_vx_xx_xx
pub fn create_backup(path: &Path, schema_version: u32) -> Result<String, anyhow::Error> {
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
