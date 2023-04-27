use std::path::Path;

use super::*;
use crate::{db::persistent::LATEST_SCHEMA_VERSION, multisig::PersistentKeyDB};
use rocksdb::{Options, DB};

use utilities::{assert_ok, testing::new_temp_directory_with_nonexistent_file};

const COLUMN_FAMILIES: &[&str] = &[DATA_COLUMN, METADATA_COLUMN];

fn open_db_and_write_version_data(path: &Path, schema_version: u32) {
	let mut opts = Options::default();
	opts.create_missing_column_families(true);
	opts.create_if_missing(true);
	let db = DB::open_cf(&opts, path, COLUMN_FAMILIES).expect("Should open db file");

	// Write the schema version
	db.put_cf(get_metadata_column_handle(&db), DB_SCHEMA_VERSION_KEY, schema_version.to_be_bytes())
		.expect("Should write DB_SCHEMA_VERSION");
}

#[test]
fn can_create_new_database() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None));
	assert!(db_path.exists());
}

#[test]
fn should_add_genesis_hash_if_missing() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let genesis_hash_added_later: state_chain_runtime::Hash = sp_core::H256::random();

	// Create a fresh db with no genesis hash
	open_db_and_write_version_data(&db_path, LATEST_SCHEMA_VERSION);

	// Open the db normally, so the genesis hash will be added
	let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, Some(genesis_hash_added_later))
		.unwrap();

	// Check that the genesis hash was added and is correct
	assert_eq!(
		db.kv_db
			.get_genesis_hash()
			.expect("Should read genesis hash")
			.expect("Should find genesis hash"),
		genesis_hash_added_later
	);
}

#[test]
fn should_error_if_genesis_hash_is_different() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let genesis_hash_1 = sp_core::H256::from_low_u64_be(1);
	let genesis_hash_2 = sp_core::H256::from_low_u64_be(2);
	assert_ne!(genesis_hash_1, genesis_hash_2);

	// Open the db, so hash 1 is written
	{
		assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, Some(genesis_hash_1),));
	}

	// Open the db again, but with hash 2, so it should compare them and return an error
	{
		assert!(
			PersistentKeyDB::open_and_migrate_to_latest(&db_path, Some(genesis_hash_2),).is_err()
		);
	}
}

#[test]
fn test_migration_to_latest_from_0() {
	let (_dir, db_file) = utilities::testing::new_temp_directory_with_nonexistent_file();

	{
		let db = PersistentKeyDB::open_and_migrate_to_version(&db_file, None, 0).unwrap();

		assert_eq!(db.kv_db.get_schema_version().unwrap(), 0);
	}

	let db = PersistentKeyDB::open_and_migrate_to_latest(&db_file, None).unwrap();

	assert_eq!(db.kv_db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);
}
