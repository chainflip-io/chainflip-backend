#[cfg(test)]
mod persistent_key_db_tests;

use std::{cmp::Ordering, collections::HashMap, fs, mem::size_of, path::Path};

use cf_primitives::KeyId;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, WriteBatch, DB};
use slog::o;

use crate::{
	logging::COMPONENT_KEY,
	multisig::{
		client::KeygenResultInfo,
		crypto::{CryptoScheme, CHAIN_TAG_SIZE},
		eth::EthSigning,
		polkadot::PolkadotSigning,
		ChainTag,
	},
	witnesser::checkpointing::WitnessedUntil,
};

use anyhow::{anyhow, bail, Context, Result};

/// This is the version of the data on this current branch
/// This version *must* be bumped, and appropriate migrations
/// written on any changes to the persistent application data format
pub const LATEST_SCHEMA_VERSION: u32 = 1;

/// Key used to store the `LATEST_SCHEMA_VERSION` value in the `METADATA_COLUMN`
pub const DB_SCHEMA_VERSION_KEY: &[u8; 17] = b"db_schema_version";
pub const GENESIS_HASH_KEY: &[u8; 12] = b"genesis_hash";

/// A static length prefix is used on the `DATA_COLUMN`
pub const PREFIX_SIZE: usize = 10;
const PARTIAL_PREFIX_SIZE: usize = PREFIX_SIZE - CHAIN_TAG_SIZE;
/// Keygen data uses a prefix that is a combination of a keygen data prefix and the chain tag
const KEYGEN_DATA_PARTIAL_PREFIX: &[u8; PARTIAL_PREFIX_SIZE] = b"key_____";
/// The Witnesser checkpoint uses a prefix that is a combination of a checkpoint prefix and the
/// chain tag
const WITNESSER_CHECKPOINT_PARTIAL_PREFIX: &[u8; PARTIAL_PREFIX_SIZE] = b"check___";

/// Column family names
// All data is stored in `DATA_COLUMN` with a prefix for key spaces
pub const DATA_COLUMN: &str = "data";
// This column is just for schema version info. No prefix is used.
pub const METADATA_COLUMN: &str = "metadata";
// The default column that rust_rocksdb uses (we ignore)
pub const DEFAULT_COLUMN_NAME: &str = "default";

/// Name of the directory that the backups will go into (only created before migrations)
const BACKUPS_DIRECTORY: &str = "backups";

/// Database for keys and persistent metadata
pub struct PersistentKeyDB {
	/// Rocksdb database instance
	db: DB,
	logger: slog::Logger,
}

fn write_schema_version_to_batch(db: &DB, batch: &mut WriteBatch, version: u32) {
	batch.put_cf(get_metadata_column_handle(db), DB_SCHEMA_VERSION_KEY, version.to_be_bytes());
}

impl PersistentKeyDB {
	fn new_version_0(
		db_path: &Path,
		genesis_hash: Option<state_chain_runtime::Hash>,
		logger: &slog::Logger,
	) -> Result<Self> {
		// Use a prefix extractor on the data column
		let mut cfopts_for_prefix = Options::default();
		cfopts_for_prefix
			.set_prefix_extractor(rocksdb::SliceTransform::create_fixed_prefix(PREFIX_SIZE));

		// Build a list of column families with descriptors
		let mut cfs: HashMap<String, ColumnFamilyDescriptor> = HashMap::from_iter([
			(
				METADATA_COLUMN.to_string(),
				ColumnFamilyDescriptor::new(METADATA_COLUMN, Options::default()),
			),
			(DATA_COLUMN.to_string(), ColumnFamilyDescriptor::new(DATA_COLUMN, cfopts_for_prefix)),
		]);

		// TODO: I don't think this is needed anymore, so let's remove it?
		let is_existing_db = db_path.exists();
		if is_existing_db {
			// Add the column families found in the existing db, they might be needed for migration.
			DB::list_cf(&Options::default(), db_path)
				.map_err(anyhow::Error::msg)
				.with_context(|| {
					format!(
						"Failed to read column families from existing database {}",
						db_path.display()
					)
				})?
				.into_iter()
				.for_each(|cf_name| {
					// Filter out the `default` column because we don't use it
					// and if we already have the cf, we don't want to add it again
					if !(cf_name == DEFAULT_COLUMN_NAME || cfs.contains_key(&cf_name)) {
						cfs.insert(
							cf_name.clone(),
							ColumnFamilyDescriptor::new(cf_name, Options::default()),
						);
					}
				});
		}

		let mut create_missing_db_and_cols_opts = Options::default();
		create_missing_db_and_cols_opts.create_missing_column_families(true);
		create_missing_db_and_cols_opts.create_if_missing(true);

		// Open the db or create a new one if it doesn't exist
		let db =
			DB::open_cf_descriptors(&create_missing_db_and_cols_opts, db_path, cfs.into_values())
				.map_err(anyhow::Error::msg)
				.context(format!("Failed to open database at: {}", db_path.display()))?;

		let mut batch = WriteBatch::default();

		write_schema_version_to_batch(&db, &mut batch, 0);

		if let Some(genesis_hash) = genesis_hash {
			batch.put_cf(get_metadata_column_handle(&db), GENESIS_HASH_KEY, genesis_hash);
		}

		db.write(batch).context("Failed to write metadata to new db")?;

		Ok(PersistentKeyDB::new_from_db(db, logger))
	}
	/// Create a new persistent key database. If the database exists and the schema version
	/// is below the latest, it will attempt to migrate the data to the latest version.
	pub fn new_and_migrate_to_latest(
		db_path: &Path,
		genesis_hash: Option<state_chain_runtime::Hash>,
		logger: &slog::Logger,
	) -> Result<Self> {
		let db = PersistentKeyDB::new_version_0(db_path, genesis_hash, logger)?;

		// An existing db, so perform migrations and update the metadata
		migrate_db_to_latest(db.db, db_path, genesis_hash, logger)
                    .with_context(|| format!("Failed to migrate database at {}. Manual restoration of a backup or purging of the file is required.", db_path.display()))
	}

	fn new_from_db(db: DB, logger: &slog::Logger) -> Self {
		PersistentKeyDB { db, logger: logger.new(o!(COMPONENT_KEY => "PersistentKeyDB")) }
	}

	/// Write the keyshare to the db, indexed by the key id
	pub fn update_key<C: CryptoScheme>(
		&self,
		key_id: &KeyId,
		keygen_result_info: &KeygenResultInfo<C>,
	) {
		let key_id_with_prefix =
			[get_keygen_data_prefix::<C>().as_slice(), &key_id.to_bytes()[..]].concat();

		self.db
			.put_cf(
				get_data_column_handle(&self.db),
				key_id_with_prefix,
				bincode::serialize(keygen_result_info)
					.expect("Couldn't serialize keygen result info"),
			)
			.unwrap_or_else(|e| panic!("Failed to update key {}. Error: {}", &key_id, e));
	}

	pub fn load_keys<C: CryptoScheme>(&self) -> HashMap<KeyId, KeygenResultInfo<C>> {
		let keys: HashMap<KeyId, KeygenResultInfo<C>> = self
			.db
			.prefix_iterator_cf(get_data_column_handle(&self.db), get_keygen_data_prefix::<C>())
			.filter_map(|result| match result {
				Ok(key) => Some(key),
				Err(err) => {
					slog::error!(self.logger, "Error getting prefix iterator: {err}");
					None
				},
			})
			.filter_map(|(key_id_with_prefix, key_info)| {
				let key_id = KeyId::from_bytes(&key_id_with_prefix[PREFIX_SIZE..]);

				match bincode::deserialize::<KeygenResultInfo<C>>(&key_info) {
					Ok(keygen_result_info) => Some((key_id, keygen_result_info)),
					Err(err) => {
						slog::error!(
							self.logger,
							"Could not deserialize {} key from database: {}",
							C::NAME,
							err;
							"key_id" => key_id.to_string()
						);
						None
					},
				}
			})
			.collect();
		if !keys.is_empty() {
			slog::debug!(self.logger, "Loaded {} {} keys from the database", keys.len(), C::NAME,);
		}
		keys
	}

	/// Write the witnesser checkpoint to the db
	pub fn update_checkpoint(&self, chain_tag: ChainTag, checkpoint: &WitnessedUntil) {
		self.db
			.put_cf(
				get_data_column_handle(&self.db),
				get_checkpoint_prefix(chain_tag),
				bincode::serialize(checkpoint).expect("Should serialize WitnessedUntil checkpoint"),
			)
			.unwrap_or_else(|e| {
				panic!("Failed to update {chain_tag} witnesser checkpoint. Error: {e}")
			});
	}

	pub fn load_checkpoint(&self, chain_tag: ChainTag) -> Result<Option<WitnessedUntil>> {
		self.db
			.get_cf(get_data_column_handle(&self.db), get_checkpoint_prefix(chain_tag))?
			.map(|data| {
				bincode::deserialize::<WitnessedUntil>(&data).map_err(|e| {
					anyhow!("Could not deserialize {chain_tag} WitnessedUntil checkpoint: {e}")
				})
			})
			.transpose()
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

pub fn get_data_column_handle(db: &DB) -> &ColumnFamily {
	get_column_handle(db, DATA_COLUMN)
}

fn get_column_handle<'a>(db: &'a DB, column_name: &str) -> &'a ColumnFamily {
	db.cf_handle(column_name)
		.unwrap_or_else(|| panic!("Should get column family handle for {column_name}"))
}

/// Get the schema version from the metadata column in the db.
fn read_schema_version(db: &DB, logger: &slog::Logger) -> Result<u32> {
	db.get_cf(get_metadata_column_handle(db), DB_SCHEMA_VERSION_KEY)
		.context("Failed to get metadata column")?
		.map(|version| {
			let version: [u8; 4] = version.try_into().expect("Version should be a u32");
			let version = u32::from_be_bytes(version);
			slog::info!(logger, "Found db_schema_version of {version}");
			version
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

/// Reads the schema version and migrates the db to the latest schema version if required
fn migrate_db_to_latest(
	db: DB,
	path: &Path,
	genesis_hash: Option<state_chain_runtime::Hash>,
	logger: &slog::Logger,
) -> Result<PersistentKeyDB, anyhow::Error> {
	let version = read_schema_version(&db, logger)
		.context("Failed to read schema version from existing db")?;

	if let Some(expected_genesis_hash) = genesis_hash {
		check_or_set_genesis_hash(&db, expected_genesis_hash)?;
	}

	let db = PersistentKeyDB::new_from_db(db, logger);

	// Check if the db version is up-to-date or we need to do migrations
	match version.cmp(&LATEST_SCHEMA_VERSION) {
		Ordering::Equal => {
			// The db is at the latest version, no action needed
			slog::info!(logger, "Database already at latest version of: {}", LATEST_SCHEMA_VERSION);
			Ok(db)
		},
		Ordering::Greater => {
			// We do not support backwards migrations
			Err(anyhow!("Database schema version {} is ahead of the current schema version {}. Is your Chainflip Engine up to date?",
                    version,
                    LATEST_SCHEMA_VERSION)
                )
		},
		Ordering::Less => {
			// Backup the database before migrating it
			slog::info!(
				logger,
				"Database backup created at {}",
				create_backup(path, version)
					.map_err(anyhow::Error::msg)
					.context("Failed to create database backup before migration")?
			);

			for version in version..LATEST_SCHEMA_VERSION {
				slog::info!(
					logger,
					"Database is migrating from version {} to {}",
					version,
					version + 1
				);

				match version {
					0 => {
						migrate_0_to_1(&db);
					},
					_ => {
						panic!("Unexpected migration from version {version}");
					},
				}
			}

			Ok(db)
		},
	}
}

fn migrate_0_to_1_for_scheme<C: CryptoScheme>(db: &DB, batch: &mut WriteBatch) {
	// We will need to do this for every crypto scheme!
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

fn migrate_0_to_1(db: &PersistentKeyDB) {
	let db = &db.db;

	let mut batch = WriteBatch::default();

	migrate_0_to_1_for_scheme::<EthSigning>(db, &mut batch);
	migrate_0_to_1_for_scheme::<PolkadotSigning>(db, &mut batch);

	write_schema_version_to_batch(db, &mut batch, 1);

	db.write(batch).unwrap();
}

#[test]
fn test_migration_to_v1() {
	use crate::multisig::{client::keygen, Rng};
	use cf_primitives::AccountId;
	use rand_legacy::FromEntropy;
	use std::collections::BTreeSet;

	let (_dir, db_file) = crate::testing::new_temp_directory_with_nonexistent_file();
	let logger = crate::logging::test_utils::new_test_logger();

	// create db with version 0
	let db = PersistentKeyDB::new_version_0(&db_file, None, &logger).unwrap();

	let account_ids: BTreeSet<_> = [1, 2, 3].iter().map(|i| AccountId::new([*i; 32])).collect();

	let (key_id, key_data) =
		keygen::generate_key_data::<EthSigning>(account_ids, &mut Rng::from_entropy());

	let key_info = key_data.values().next().unwrap();

	assert_eq!(key_id.len(), 33);

	// Insert the key manually, so it matches the way it was done in db version 0:
	{
		let key_id_with_prefix =
			[get_keygen_data_prefix::<EthSigning>().as_slice(), &key_id].concat();

		db.db
			.put_cf(
				get_data_column_handle(&db.db),
				key_id_with_prefix,
				bincode::serialize(key_info).expect("Couldn't serialize keygen result info"),
			)
			.unwrap();
	}

	// After migration, the we should be able to load the key using the new code
	migrate_0_to_1(&db);

	let keys = db.load_keys::<EthSigning>();

	assert_eq!(keys.len(), 1);

	let (key_id_loaded, key_info_loaded) = keys.into_iter().next().unwrap();

	assert_eq!(key_id_loaded.epoch_index, 0);
	assert_eq!(key_id_loaded.public_key_bytes, key_id);
	assert_eq!(key_info, &key_info_loaded);
}
