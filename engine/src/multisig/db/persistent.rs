use std::{cmp::Ordering, collections::HashMap, fs, path::Path};

use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, DB};
use slog::o;

use crate::{
    logging::COMPONENT_KEY,
    multisig::{
        client::KeygenResultInfo,
        crypto::{CryptoScheme, CHAIN_TAG_SIZE},
        KeyId,
    },
};

use anyhow::{Context, Result};

/// This is the version of the data on this current branch
/// This version *must* be bumped, and appropriate migrations
/// written on any changes to the persistent application data format
pub const LATEST_SCHEMA_VERSION: u32 = 0;

/// Key used to store the `LATEST_SCHEMA_VERSION` value in the `METADATA_COLUMN`
pub const DB_SCHEMA_VERSION_KEY: &[u8; 17] = b"db_schema_version";

/// A static length prefix is used on the `DATA_COLUMN`
pub const PREFIX_SIZE: usize = 10;
/// Keygen data uses a prefix that is a combination of a keygen data prefix and the chain tag
const KEYGEN_DATA_PARTIAL_PREFIX: &[u8; PREFIX_SIZE - CHAIN_TAG_SIZE] = b"key_____";

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
impl PersistentKeyDB {
    /// Create a new persistent key database. If the database exists and the schema version
    /// is below the latest, it will attempt to migrate the data to the latest version
    pub fn new_and_migrate_to_latest(db_path: &Path, logger: &slog::Logger) -> Result<Self> {
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
            (
                DATA_COLUMN.to_string(),
                ColumnFamilyDescriptor::new(DATA_COLUMN, cfopts_for_prefix),
            ),
        ]);

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

        let cf_descriptors: Vec<ColumnFamilyDescriptor> =
            cfs.into_iter().map(|(_, cf_desc)| cf_desc).collect();

        // Open the db or create a new one if it doesn't exist
        let db =
            DB::open_cf_descriptors(&create_missing_db_and_cols_opts, &db_path, cf_descriptors)
                .map_err(anyhow::Error::msg)
                .context(format!("Failed to open database at: {}", db_path.display()))?;

        let p_kdb = if is_existing_db {
            // Perform migrations and update schema version
            migrate_db_to_latest(db, db_path, logger)
                    .with_context(|| format!("Failed to migrate database at {}. Manual restoration of a backup or purging of the file is required.", db_path.display()))?
        } else {
            // It's a new db, so just write the latest schema version
            PersistentKeyDB::new_from_db_and_set_schema_version_to_latest(db, logger)?
        };

        Ok(p_kdb)
    }

    fn new_from_db(db: DB, logger: &slog::Logger) -> Self {
        PersistentKeyDB {
            db,
            logger: logger.new(o!(COMPONENT_KEY => "PersistentKeyDB")),
        }
    }

    // Create a new `PersistentKeyDB` and write the latest schema version
    pub fn new_from_db_and_set_schema_version_to_latest(
        db: DB,
        logger: &slog::Logger,
    ) -> Result<Self, anyhow::Error> {
        // Update version data
        db.put_cf(
            get_metadata_column_handle(&db),
            DB_SCHEMA_VERSION_KEY,
            LATEST_SCHEMA_VERSION.to_be_bytes(),
        )
        .context("Failed to write schema version to db")?;

        Ok(PersistentKeyDB::new_from_db(db, logger))
    }

    /// Write the keyshare to the db, indexed by the key id
    pub fn update_key<C: CryptoScheme>(
        &self,
        key_id: &KeyId,
        keygen_result_info: &KeygenResultInfo<C::Point>,
    ) {
        let key_id_with_prefix = [
            KEYGEN_DATA_PARTIAL_PREFIX.to_vec(),
            C::CHAIN_TAG.to_vec(),
            key_id.0.clone(),
        ]
        .concat();

        self.db
            .put_cf(
                get_data_column_handle(&self.db),
                key_id_with_prefix,
                &bincode::serialize(keygen_result_info)
                    .expect("Couldn't serialize keygen result info"),
            )
            .unwrap_or_else(|e| panic!("Failed to update key {}. Error: {}", &key_id, e));
    }

    pub fn load_keys<C: CryptoScheme>(&self) -> HashMap<KeyId, KeygenResultInfo<C::Point>> {
        self.db
            .prefix_iterator_cf(
                get_data_column_handle(&self.db),
                [KEYGEN_DATA_PARTIAL_PREFIX.to_vec(), C::CHAIN_TAG.to_vec()].concat(),
            )
            .filter_map(|(key_id, key_info)| {
                // Strip the prefix off the key_id
                let key_id: KeyId = KeyId(key_id[PREFIX_SIZE..].into());

                // deserialize the `KeygenResultInfo`
                match bincode::deserialize::<KeygenResultInfo<C::Point>>(&*key_info) {
                    Ok(keygen_result_info) => {
                        slog::debug!(
                            self.logger,
                            "Loaded {} key_info (key_id: {}) from database",
                            C::NAME,
                            key_id
                        );
                        Some((key_id, keygen_result_info))
                    }
                    Err(err) => {
                        slog::error!(
                            self.logger,
                            "Could not deserialize {} key_info (key_id: {}) from database: {}",
                            C::NAME,
                            key_id,
                            err
                        );
                        None
                    }
                }
            })
            .collect()
    }
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
        fs::create_dir_all(&backups_path)
            .map_err(anyhow::Error::msg)
            .with_context(|| {
                format!(
                    "Failed to create backup directory {}",
                    &backups_path
                        .to_str()
                        .expect("Should get backup path as str")
                )
            })?;
    }

    // This db backup folder should not exist yet
    let backup_dir_path = backups_path.join(backup_dir_name);
    if backup_dir_path.exists() {
        return Err(anyhow::Error::msg(format!(
            "Backup directory already exists {}",
            backup_dir_path.display()
        )));
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
        .unwrap_or_else(|| panic!("Should get column family handle for {}", column_name))
}

/// Get the schema version from the metadata column in the db.
fn read_schema_version(db: &DB, logger: &slog::Logger) -> Result<u32> {
    db.get_cf(get_metadata_column_handle(db), DB_SCHEMA_VERSION_KEY)
        .context("Failed to get metadata column when trying to read the schema version")?
        .map(|version| {
            let version: [u8; 4] = version.try_into().expect("Version should be a u32");
            let version = u32::from_be_bytes(version);
            slog::info!(logger, "Found db_schema_version of {}", version);
            version
        })
        .ok_or_else(|| anyhow::Error::msg("Could not find db schema version"))
}

/// Reads the schema version and migrates the db to the latest schema version if required
fn migrate_db_to_latest(
    db: DB,
    path: &Path,
    logger: &slog::Logger,
) -> Result<PersistentKeyDB, anyhow::Error> {
    // Read the schema version from the db
    let version =
        read_schema_version(&db, logger).context("Failed to read schema version on existing db")?;

    // Check if the db version is up-to-date or we need to do migrations
    match version.cmp(&LATEST_SCHEMA_VERSION) {
        Ordering::Equal => {
            // The db is at the latest version, no action needed
            slog::info!(
                logger,
                "Database already at latest version of: {}",
                LATEST_SCHEMA_VERSION
            );
            Ok(PersistentKeyDB::new_from_db(db, logger))
        }
        Ordering::Greater => {
            // We do not support backwards migrations
            Err(anyhow::Error::msg(
                    format!("Database schema version {} is ahead of the current schema version {}. Is your Chainflip Engine up to date?",
                    version,
                    LATEST_SCHEMA_VERSION)
                ))
        }
        Ordering::Less => {
            slog::info!(
                logger,
                "Database is migrating from version {} to {}",
                version,
                LATEST_SCHEMA_VERSION
            );

            // Backup the database before migrating it
            slog::info!(
                logger,
                "Database backup created at {}",
                create_backup(path, version)
                    .map_err(anyhow::Error::msg)
                    .context("Failed to create database backup before migration")?
            );

            // Future migrations can be added here

            // No migrations exist yet so just panic
            panic!("Invalid migration from schema version {}", version);
        }
    }
}

#[cfg(test)]
mod tests {

    use std::path::PathBuf;

    use crate::multisig::crypto::polkadot::PolkadotSigning;
    use crate::multisig::{client::keygen::generate_key_data_until_compatible, crypto::Rng};
    use sp_runtime::AccountId32;
    use tempfile::TempDir;

    use super::*;

    use crate::multisig::crypto::eth::EthSigning;

    use crate::{
        logging::test_utils::new_test_logger,
        testing::{assert_ok, new_temp_directory_with_nonexistent_file},
    };

    fn generate_key_share_for_test<C: CryptoScheme>() -> KeygenResultInfo<C::Point> {
        use rand_legacy::FromEntropy;
        let rng = Rng::from_entropy();
        let account_id = AccountId32::new([0; 32]);
        let (_key_id, key_shares) =
            generate_key_data_until_compatible(&[account_id.clone()], 20, rng);
        key_shares[&account_id].clone()
    }

    const COLUMN_FAMILIES: &[&str] = &[DATA_COLUMN, METADATA_COLUMN];

    fn open_db_and_write_version_data(path: &Path, schema_version: u32) {
        let mut opts = Options::default();
        opts.create_missing_column_families(true);
        opts.create_if_missing(true);
        let db = DB::open_cf(&opts, &path, COLUMN_FAMILIES).expect("Should open db file");

        // Write the schema version
        db.put_cf(
            get_metadata_column_handle(&db),
            DB_SCHEMA_VERSION_KEY,
            schema_version.to_be_bytes(),
        )
        .expect("Should write DB_SCHEMA_VERSION");
    }

    fn find_backups(temp_dir: &TempDir, db_path: PathBuf) -> Result<Vec<PathBuf>, std::io::Error> {
        let backups_path = temp_dir.path().join(BACKUPS_DIRECTORY);

        let backups: Vec<PathBuf> = fs::read_dir(&backups_path)?
            .collect::<Result<Vec<std::fs::DirEntry>, std::io::Error>>()?
            .iter()
            .filter_map(|entry| {
                let file_path = entry.path();
                if file_path.is_dir() && file_path != *db_path {
                    Some(file_path)
                } else {
                    None
                }
            })
            .collect();

        Ok(backups)
    }

    // Just a random key
    const TEST_KEY: [u8; 33] = [
        3, 3, 94, 73, 229, 219, 117, 193, 0, 143, 51, 247, 54, 138, 135, 255, 177, 63, 13, 132, 93,
        195, 249, 200, 151, 35, 228, 224, 122, 6, 111, 38, 103,
    ];

    #[test]
    fn can_create_new_database() {
        let logger = new_test_logger();
        let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
        assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(
            &db_path, &logger
        ));
        assert!(db_path.exists());
    }

    #[test]
    fn new_db_is_created_with_latest_schema_version() {
        let logger = new_test_logger();
        let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
        // Create a fresh db. This will also write the schema version
        assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(
            &db_path, &logger
        ));

        assert!(db_path.exists());
        {
            // Open the db file manually
            let db = DB::open_cf(&Options::default(), &db_path, COLUMN_FAMILIES)
                .expect("Should open db file");

            // Get version number
            let db_schema_version = db
                .get_cf(get_metadata_column_handle(&db), DB_SCHEMA_VERSION_KEY)
                .expect("Should get from metadata column")
                .expect("No version data found");
            let db_schema_version: [u8; 4] = db_schema_version
                .try_into()
                .expect("Version should be a u32");
            assert_eq!(u32::from_be_bytes(db_schema_version), LATEST_SCHEMA_VERSION);
        }
    }

    #[test]
    fn new_db_returns_db_when_db_data_version_is_latest() {
        let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
        open_db_and_write_version_data(&db_path, LATEST_SCHEMA_VERSION);
        assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(
            &db_path,
            &new_test_logger()
        ));
    }

    #[test]
    fn should_not_migrate_backwards() {
        let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
        // Create a db with schema version + 1
        {
            open_db_and_write_version_data(&db_path, LATEST_SCHEMA_VERSION + 1);
        }

        // Open the db and make sure the migration errors
        {
            let db = DB::open_cf(&Options::default(), &db_path, COLUMN_FAMILIES)
                .expect("Should open db file");
            assert!(migrate_db_to_latest(db, &db_path, &new_test_logger()).is_err());
        }
    }

    #[test]
    fn can_load_keys_with_current_keygen_info() {
        type Scheme = EthSigning;

        let secret_share = generate_key_share_for_test::<Scheme>();
        let logger = new_test_logger();

        let key_id = KeyId(TEST_KEY.into());
        let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
        {
            let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, &logger).unwrap();

            p_db.update_key::<Scheme>(&key_id, &secret_share);
        }

        {
            let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, &logger).unwrap();
            let keys = p_db.load_keys::<Scheme>();
            let key = keys.get(&key_id).expect("Should have an entry for key");
            // single party keygen has a threshold of 0
            assert_eq!(key.params.threshold, 0);
        }
    }

    #[test]
    fn can_update_key() {
        type Scheme = EthSigning;

        let logger = new_test_logger();
        let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
        let key_id = KeyId(vec![0; 33]);

        let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, &logger).unwrap();

        let keys_before = p_db.load_keys::<Scheme>();
        // there should be no key [0; 33] yet
        assert!(keys_before.get(&key_id).is_none());

        let keygen_result_info = generate_key_share_for_test::<Scheme>();
        p_db.update_key::<Scheme>(&key_id, &keygen_result_info);

        let keys_before = p_db.load_keys::<Scheme>();
        assert!(keys_before.get(&key_id).is_some());
    }

    #[test]
    fn backup_should_fail_if_already_exists() {
        let logger = new_test_logger();
        let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
        // Create a normal db
        assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(
            &db_path, &logger
        ));

        // Backup up the db to a specified directory.
        // We cannot use the normal backup directory because it has a timestamp in it.
        let backup_dir_name = "test".to_string();
        assert_ok!(create_backup_with_directory_name(
            &db_path,
            backup_dir_name.clone()
        ));

        // Try and back it up again to the same directory and it should fail
        assert!(create_backup_with_directory_name(&db_path, backup_dir_name).is_err());
    }

    #[test]
    fn backup_should_fail_if_cant_copy_files() {
        let logger = new_test_logger();
        let (directory, db_path) = new_temp_directory_with_nonexistent_file();

        // Create a normal db
        assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(
            &db_path, &logger
        ));
        // Do a backup of the db,
        assert_ok!(create_backup(&db_path, LATEST_SCHEMA_VERSION));

        // Change the backups folder to readonly
        let backups_path = directory.path().join(BACKUPS_DIRECTORY);
        assert!(backups_path.exists());
        let mut permissions = backups_path.metadata().unwrap().permissions();
        permissions.set_readonly(true);
        assert_ok!(fs::set_permissions(&backups_path, permissions));
        assert!(
            backups_path.metadata().unwrap().permissions().readonly(),
            "Readonly permissions were not set"
        );

        // Try and backup the db again, it should fail with permissions denied due to readonly
        assert!(create_backup(&db_path, LATEST_SCHEMA_VERSION).is_err());
    }

    #[test]
    fn can_load_key_from_backup() {
        type Scheme = EthSigning;

        let logger = new_test_logger();
        let (directory, db_path) = new_temp_directory_with_nonexistent_file();
        let key_id = KeyId(vec![0; 33]);

        // Create a normal db and save a key in it
        {
            let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, &logger).unwrap();
            let keygen_result_info = generate_key_share_for_test::<Scheme>();
            p_db.update_key::<Scheme>(&key_id, &keygen_result_info);
        }

        // Do a backup of the db,
        assert_ok!(create_backup(&db_path, LATEST_SCHEMA_VERSION));

        // Try and open the backup to make sure it still works
        {
            let backups = find_backups(&directory, db_path).unwrap();
            assert!(
                backups.len() == 1,
                "Incorrect number of backups found in {}",
                BACKUPS_DIRECTORY
            );

            // Should be able to open the backup and load the key
            let p_db =
                PersistentKeyDB::new_and_migrate_to_latest(backups.first().unwrap(), &logger)
                    .unwrap();

            assert!(p_db.load_keys::<Scheme>().get(&key_id).is_some());
        }
    }

    #[test]
    fn can_use_multiple_crypto_schemes() {
        type Scheme1 = EthSigning;
        type Scheme2 = PolkadotSigning;

        let logger = new_test_logger();
        let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
        let scheme_1_key_id = KeyId(vec![0; 33]);
        let scheme_2_key_id = KeyId(vec![1; 33]);

        // Create a normal db and save multiple keys to it of different crypto schemes
        {
            let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, &logger).unwrap();

            p_db.update_key::<Scheme1>(&scheme_1_key_id, &generate_key_share_for_test::<Scheme1>());
            p_db.update_key::<Scheme2>(&scheme_2_key_id, &generate_key_share_for_test::<Scheme2>());
        }

        // Open the db and load the keys of both types
        {
            let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, &logger).unwrap();

            let scheme_1_keys = p_db.load_keys::<Scheme1>();
            assert_eq!(scheme_1_keys.len(), 1, "Incorrect number of keys loaded");
            assert!(
                scheme_1_keys.get(&scheme_1_key_id).is_some(),
                "Incorrect key id"
            );

            let scheme_2_keys = p_db.load_keys::<Scheme2>();
            assert_eq!(scheme_2_keys.len(), 1, "Incorrect number of keys loaded");
            assert!(
                scheme_2_keys.get(&scheme_2_key_id).is_some(),
                "Incorrect key id"
            );
        }
    }
}
