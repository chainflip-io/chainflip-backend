use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
    path::Path,
};

use super::KeyDB;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, IteratorMode, Options, WriteBatch, DB};
use slog::o;

use crate::{
    logging::COMPONENT_KEY,
    multisig::{client::KeygenResultInfo, KeyId},
};

use anyhow::{Context, Result};

/// This is the version of the data on this current branch
/// This version *must* be bumped, and appropriate migrations
/// written on any changes to the persistent application data format
const DATA_VERSION: u32 = 1;

/// Key used to store the `DATA_VERSION` value in the `METADATA_COLUMN`
pub const DATA_VERSION_KEY: &[u8; 12] = b"data_version";

/// Prefixes for the `DATA_COLUMN`
const PREFIX_SIZE: usize = 4;
const KEYGEN_DATA_PREFIX: &[u8; PREFIX_SIZE] = b"key_";

/// Column family names
// All data is stored in `DATA_COLUMN` with a prefix for key spaces
const DATA_COLUMN: &str = "data";
// This column is just for data version info. No prefix is used.
const METADATA_COLUMN: &str = "metadata";
const COLUMN_FAMILIES: &[&str] = &[DATA_COLUMN, METADATA_COLUMN];

/// Database for keys and persistent metadata
pub struct PersistentKeyDB {
    /// Rocksdb database instance
    db: DB,
    logger: slog::Logger,
}
impl PersistentKeyDB {
    pub fn new(path: &Path, logger: &slog::Logger) -> Result<Self> {
        let logger = logger.new(o!(COMPONENT_KEY => "PersistentKeyDB"));

        // Build a list of column families with default descriptors
        let mut cfs_check: HashSet<String> = HashSet::new();
        let mut cfs: HashMap<String, ColumnFamilyDescriptor> = COLUMN_FAMILIES
            .iter()
            .map(|cf_name| {
                (
                    cf_name.to_string(),
                    ColumnFamilyDescriptor::new((*cf_name).to_string(), Options::default()),
                )
            })
            .collect();

        let has_existing_db = path.exists();
        if has_existing_db {
            // Add the column families found in the existing db, they might be needed for migration.
            cfs = cfs
                .into_iter()
                .chain(
                    DB::list_cf(&Options::default(), path)
                        .expect("Should get list of column families")
                        .into_iter()
                        .filter_map(|cf_name| {
                            // Filter out the `default` column because we don't use it
                            if cf_name == "default" {
                                None
                            } else {
                                cfs_check.insert(cf_name.clone());
                                Some((
                                    cf_name.clone(),
                                    ColumnFamilyDescriptor::new(cf_name, Options::default()),
                                ))
                            }
                        }),
                )
                .collect();
        }

        // Override the default descriptor for the data column so it can use a prefix extractor
        let mut cfopts_for_prefix = Options::default();
        cfopts_for_prefix
            .set_prefix_extractor(rocksdb::SliceTransform::create_fixed_prefix(PREFIX_SIZE));
        cfs.insert(
            DATA_COLUMN.to_string(),
            ColumnFamilyDescriptor::new(DATA_COLUMN, cfopts_for_prefix),
        );

        let mut create_missing_db_and_cols_opts = Options::default();
        create_missing_db_and_cols_opts.create_missing_column_families(true);
        create_missing_db_and_cols_opts.create_if_missing(true);

        let cf_descriptors: Vec<ColumnFamilyDescriptor> =
            cfs.into_iter().map(|(_, cf_desc)| cf_desc).collect();

        // Open the db or create a new one if it doesn't exist
        let mut db =
            DB::open_cf_descriptors(&create_missing_db_and_cols_opts, &path, cf_descriptors)
                .map_err(anyhow::Error::msg)
                .context(format!("Failed to open database at: {}", path.display()))?;

        // We must check if the database is new or not, so we don't try and migrate from version 0.
        // Because version 0 had no metadata, we cant tell the difference between version 0 and a new db.
        let data_version = if has_existing_db {
            read_data_version(&db, &logger)
        } else {
            write_latest_data_version(&db)?
        };

        if data_version != DATA_VERSION {
            if data_version < DATA_VERSION {
                // TODO: backup the database before migrating it. #1182

                slog::info!(
                    logger,
                    "Database is migrating from version {} to {}",
                    data_version,
                    DATA_VERSION
                );
                // Preform migrations
                migrate_db_to_latest(&mut db, data_version)
                    .expect(&format!("Failed to migrate database at {}. Manual restoration of a backup or purging of the file is required.", path.display()));
            } else {
                // Automatic backwards migration is not supported
                return Err(anyhow::Error::msg(
                    format!("Database is at data version {} but needs to be {}. Manual backwards migration is required",
                    data_version,
                    DATA_VERSION)
                ));
            }
        } else {
            // Check for unused data columns
            let junk_columns: HashSet<String> = cfs_check
                .iter()
                .filter(|cf_name| !COLUMN_FAMILIES.iter().any(|s| s == cf_name))
                .cloned()
                .collect();
            if junk_columns.len() > 0 {
                // Just a warning for now. We can delete the columns if this becomes a problem in the future.
                slog::warn!(logger, "Unknown columns found in db: {:?}", junk_columns);
            }
        }

        Ok(PersistentKeyDB { db, logger })
    }
}

impl KeyDB for PersistentKeyDB {
    fn update_key(&mut self, key_id: &KeyId, keygen_result_info: &KeygenResultInfo) {
        // TODO: this error should be handled better
        let keygen_result_info_encoded =
            bincode::serialize(keygen_result_info).expect("Could not serialize keygen_result_info");

        let key_id_with_prefix = [KEYGEN_DATA_PREFIX.to_vec(), key_id.0.clone()].concat();

        self.db
            .put_cf(
                get_data_column_handle(&self.db),
                key_id_with_prefix,
                &keygen_result_info_encoded,
            )
            .unwrap_or_else(|_| {
                panic!(
                    "Could not write key share for key_id `{}` to database",
                    &key_id
                )
            });
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        self.db
            .prefix_iterator_cf(get_data_column_handle(&self.db), KEYGEN_DATA_PREFIX)
            .filter_map(|(key_id, key_info)| {
                // Strip the prefix off the key_id
                let key_id: KeyId = KeyId(key_id[PREFIX_SIZE..].into());

                // deserialize the `KeygenResultInfo`
                match bincode::deserialize::<KeygenResultInfo>(&*key_info) {
                    Ok(keygen_info) => {
                        slog::info!(
                            self.logger,
                            "Loaded key_info (key_id: {}) from database",
                            key_id
                        );
                        Some((key_id, keygen_info))
                    }
                    Err(err) => {
                        slog::error!(
                            self.logger,
                            "Could not deserialize key_info (key_id: {}) from database: {}",
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

fn get_metadata_column_handle(db: &DB) -> &ColumnFamily {
    get_column_handle(db, METADATA_COLUMN)
}

fn get_data_column_handle(db: &DB) -> &ColumnFamily {
    get_column_handle(db, DATA_COLUMN)
}

fn get_column_handle<'a>(db: &'a DB, column_name: &str) -> &'a ColumnFamily {
    db.cf_handle(column_name)
        .unwrap_or_else(|| panic!("Should get column family handle for {}", column_name))
}

/// Used is every migration to update the db data version in the same batch write as the migration operation
fn add_version_to_batch_write(db: &DB, data_version: u32, batch: &mut WriteBatch) {
    batch.put_cf(
        get_metadata_column_handle(db),
        DATA_VERSION_KEY,
        data_version.to_be_bytes(),
    );
}

/// Update the db data version to the latest DATA_VERSION
fn write_latest_data_version(db: &DB) -> Result<u32> {
    db.put_cf(
        get_metadata_column_handle(db),
        DATA_VERSION_KEY,
        DATA_VERSION.to_be_bytes(),
    )
    .map_err(|e| anyhow::Error::msg(format!("Failed to write data version: {}", e)))?;
    Ok(DATA_VERSION)
}

/// Get the data version from the metadata column in the db.
/// If no `DATA_VERSION_KEY` exists, it will return 0.
fn read_data_version(db: &DB, logger: &slog::Logger) -> u32 {
    match db
        .get_cf(get_metadata_column_handle(db), DATA_VERSION_KEY)
        .expect("Should querying for data_version")
    {
        Some(version) => {
            let version: [u8; 4] = version.try_into().expect("Version should be a u32");
            let version = u32::from_be_bytes(version);
            slog::info!(logger, "Found data_version of {}", version);
            version
        }
        // If we can't find a data_version, we assume it's the first one
        None => {
            slog::info!(
                logger,
                "Did not find data_version in existing database. Assuming data_version of 0"
            );
            0
        }
    }
}

/// Migrates the db forward one version migration at a time
fn migrate_db_to_latest(db: &mut DB, from_version: u32) -> Result<(), anyhow::Error> {
    assert!(from_version < DATA_VERSION, "Invalid migration");

    for version in (from_version + 1)..=DATA_VERSION {
        match version {
            1 => {
                migration_0_to_1(db)?;
            }
            _ => {
                return Err(anyhow::Error::msg(format!(
                    "Invalid migration to data version {}",
                    version
                )))
            }
        }
    }
    Ok(())
}

// Moving the keygen data from column `col0` to `data` and adding a prefix
// Also adding data version to the metadata column
fn migration_0_to_1(db: &mut DB) -> Result<(), anyhow::Error> {
    // Update version data
    let mut batch = WriteBatch::default();
    add_version_to_batch_write(db, 1, &mut batch);

    // Get column handles
    let old_cf_name = "col0";

    let old_cf = db
        .cf_handle(old_cf_name)
        .unwrap_or_else(|| panic!("Should get column {}", &old_cf_name));

    let new_cf = db.cf_handle("data").expect("Should get column data");

    // Read the data from the old column and add it to the new column via the batch write
    for (old_key, old_value) in db.iterator_cf(old_cf, IteratorMode::Start) {
        // Add the prefix to the key
        let key_with_prefix = [
            KEYGEN_DATA_PREFIX.iter().cloned().collect(),
            old_key.clone(),
        ]
        .concat();
        batch.put_cf(new_cf, &key_with_prefix, old_value);
        batch.delete_cf(old_cf, old_key);
    }

    // Write the batch
    db.write(batch).map_err(|e| {
        anyhow::Error::msg(format!("Failed to write to db during migration: {}", e))
    })?;

    // Delete the old column family
    db.drop_cf(old_cf_name)
        .unwrap_or_else(|_| panic!("Should drop old column family {}", old_cf_name));

    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::{
        logging::test_utils::new_test_logger, multisig::db::PersistentKeyDB, testing::assert_ok,
    };

    fn open_db_and_write_version_data(path: &Path, version_data: u32) {
        let mut opts = Options::default();
        opts.create_missing_column_families(true);
        opts.create_if_missing(true);
        let db = DB::open_cf(&opts, &path, COLUMN_FAMILIES).expect("Should open db file");

        // Write the version data
        db.put_cf(
            get_metadata_column_handle(&db),
            DATA_VERSION_KEY,
            version_data.to_be_bytes(),
        )
        .expect("Should write DATA_VERSION");
    }

    // Just a random key
    const TEST_KEY: [u8; 33] = [
        3, 3, 94, 73, 229, 219, 117, 193, 0, 143, 51, 247, 54, 138, 135, 255, 177, 63, 13, 132, 93,
        195, 249, 200, 151, 35, 228, 224, 122, 6, 111, 38, 103,
    ];

    // To generate this, you can use the test in engine/src/signing/client/client_inner/genesis.rs
    const KEYGEN_RESULT_INFO_HEX: &'static str = "21000000000000000356815a968986af7dd8f84c365429435fba940a8b854129e78739d6d5a5ba74222000000000000000a0687cf58d7838802724b5a0ce902b421605488990c2a1156833743c68cc792303000000000000002100000000000000027cf4fe1aabd5862729d8f96ab07cf175f058fc7b4f79f3fd4fc4f9fba399dbb42100000000000000030bf033482c62d78902ff482b625dd99f025fcd429689123495bd5c5c6224cfda210000000000000002ee6ff7fd3bad3942708e965e728d8923784d36eb57f09d23aa75d8743a27c59b030000000000000030000000000000003547653178463155334555674b6947596a4c43576d6763444858516e66474e45756a775859546a5368463647636d595a0300000000000000300000000000000035444a565645595044465a6a6a394a744a5245327647767065536e7a42415541373456585053706b474b684a5348624e010000000000000030000000000000003546396f664342574c4d46586f747970587462556e624c586b4d315a39417334374752684444464a4473784b6770427502000000000000000300000000000000300000000000000035444a565645595044465a6a6a394a744a5245327647767065536e7a42415541373456585053706b474b684a5348624e30000000000000003546396f664342574c4d46586f747970587462556e624c586b4d315a39417334374752684444464a4473784b6770427530000000000000003547653178463155334555674b6947596a4c43576d6763444858516e66474e45756a775859546a5368463647636d595a03000000000000000100000000000000";

    #[test]
    fn can_create_new_database() {
        let logger = new_test_logger();
        let db_path = Path::new("db_new");

        {
            assert_ok!(PersistentKeyDB::new(&db_path, &logger));
            assert!(db_path.exists());
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn new_db_creates_new_db_with_latest_version_when_db_does_not_exist() {
        let db_path = Path::new("db3");
        let logger = new_test_logger();

        // Create a fresh db. This will also write the version data
        let _ = std::fs::remove_dir_all(db_path);
        assert!(!db_path.exists());
        {
            assert_ok!(PersistentKeyDB::new(&db_path, &logger));
        }

        assert!(db_path.exists());
        {
            // Open the db file manually
            let db = DB::open_cf(&Options::default(), &db_path, COLUMN_FAMILIES)
                .expect("Should open db file");

            // Get version number
            let data_version = db
                .get_cf(get_metadata_column_handle(&db), DATA_VERSION_KEY)
                .expect("Should get from metadata column")
                .expect("No version data found");
            let data_version: [u8; 4] = data_version.try_into().expect("Version should be a u32");
            assert_eq!(u32::from_be_bytes(data_version), DATA_VERSION);
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn new_db_returns_db_when_db_data_version_is_latest() {
        let db_path = Path::new("db4");
        let _ = std::fs::remove_dir_all(db_path);

        {
            open_db_and_write_version_data(&db_path, DATA_VERSION);
            assert_ok!(PersistentKeyDB::new(&db_path, &new_test_logger()));
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn can_migrate_0_to_1() {
        let db_path = Path::new("migrate_0_1");
        let _ = std::fs::remove_dir_all(db_path);

        let bashful_secret = KEYGEN_RESULT_INFO_HEX.to_string();
        let bashful_secret_bin = hex::decode(bashful_secret).unwrap();
        assert_ok!(bincode::deserialize::<KeygenResultInfo>(
            bashful_secret_bin.as_ref()
        ));
        let logger = new_test_logger();

        let key_id = KeyId(TEST_KEY.into());

        // Create a db that is data version 0.
        // No metadata column, the keygen data column is named 'col0' and has no prefix
        {
            let mut opts = Options::default();
            opts.create_missing_column_families(true);
            opts.create_if_missing(true);
            let db = DB::open_cf(&opts, &db_path, vec!["col0"]).expect("Should open db file");

            let cf = db.cf_handle("col0").unwrap();

            db.put_cf(cf, &key_id.0.clone(), &bashful_secret_bin)
                .expect("Should write key share");
        }

        // Load the old db and see if the keygen data is migrated and data version is updated
        {
            let p_db = PersistentKeyDB::new(&db_path, &logger).unwrap();
            let keys = p_db.load_keys();
            let key = keys.get(&key_id).expect("Should have an entry for key");
            assert_eq!(key.params.threshold, 1);
            assert_eq!(read_data_version(&p_db.db, &logger), 1);
        }

        // Check that the old column family was deleted
        {
            let cfs = DB::list_cf(&Options::default(), &db_path)
                .expect("Should get list of column families");
            assert!(cfs.iter().find(|s| *s == &"col0".to_string()).is_none());
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn can_load_keys() {
        // a hex encoded secret share
        let bashful_secret = KEYGEN_RESULT_INFO_HEX.to_string();
        let bashful_secret_bin = hex::decode(bashful_secret).unwrap();

        assert_ok!(bincode::deserialize::<KeygenResultInfo>(
            bashful_secret_bin.as_ref()
        ));
        let logger = new_test_logger();

        let key_id = KeyId(TEST_KEY.into());
        let db_path = Path::new("db1");
        let _ = std::fs::remove_dir_all(db_path);
        {
            let p_db = PersistentKeyDB::new(&db_path, &logger).unwrap();
            let db = p_db.db;

            let key = [
                KEYGEN_DATA_PREFIX.iter().cloned().collect(),
                key_id.0.clone(),
            ]
            .concat();

            db.put_cf(get_data_column_handle(&db), &key, &bashful_secret_bin)
                .expect("Should write key share");
        }

        {
            let p_db = PersistentKeyDB::new(&db_path, &logger).unwrap();
            let keys = p_db.load_keys();
            let key = keys.get(&key_id).expect("Should have an entry for key");
            assert_eq!(key.params.threshold, 1);
        }
        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn can_update_key() {
        let logger = new_test_logger();
        let key_id = KeyId(vec![0; 33]);
        let db_path = Path::new("db2");
        let _ = std::fs::remove_dir_all(db_path);
        {
            let mut p_db = PersistentKeyDB::new(&db_path, &logger).unwrap();

            let keys_before = p_db.load_keys();
            // there should be no key [0; 33] yet
            assert!(keys_before.get(&key_id).is_none());

            let keygen_result_info = hex::decode(KEYGEN_RESULT_INFO_HEX)
                .expect("Should decode hex to valid KeygenResultInfo binary");
            let keygen_result_info = bincode::deserialize::<KeygenResultInfo>(&keygen_result_info)
                .expect("Should deserialize binary into KeygenResultInfo");
            p_db.update_key(&key_id, &keygen_result_info);

            let keys_before = p_db.load_keys();
            assert!(keys_before.get(&key_id).is_some());
        }
        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }
}
