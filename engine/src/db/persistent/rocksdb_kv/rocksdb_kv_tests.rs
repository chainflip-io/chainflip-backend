use std::{fs, path::Path};

use super::*;
use crate::{
	db::persistent::LATEST_SCHEMA_VERSION,
	multisig::{eth::EthSigning, PersistentKeyDB},
};
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
fn new_db_is_created_with_correct_metadata() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let starting_genesis_hash: state_chain_runtime::Hash = sp_core::H256::random();

	// Create a fresh db. This will write the schema version and genesis hash
	assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, Some(starting_genesis_hash),));

	assert!(db_path.exists());
	{
		// Open the db file manually
		let db = DB::open_cf(&Options::default(), &db_path, COLUMN_FAMILIES)
			.expect("Should open db file");

		// Check the schema version is at the latest
		assert_eq!(
			read_schema_version(&db).expect("Should read schema version"),
			LATEST_SCHEMA_VERSION
		);

		// Check the genesis hash exists and matches the one we provided
		assert_eq!(
			read_genesis_hash(&db)
				.expect("Should read genesis hash")
				.expect("Should find genesis hash"),
			starting_genesis_hash
		);
	}
}

#[test]
fn new_db_returns_db_when_db_data_version_is_latest() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	open_db_and_write_version_data(&db_path, LATEST_SCHEMA_VERSION);
	assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None));
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
		assert!(migrate_db_to_version(&db, BackupOption::NoBackup, None, LATEST_SCHEMA_VERSION,)
			.is_err());
	}
}

#[test]
fn backup_should_fail_if_already_exists() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	// Create a normal db
	assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None));

	// Backup up the db to a specified directory.
	// We cannot use the normal backup directory because it has a timestamp in it.
	let backup_dir_name = "test".to_string();
	assert_ok!(create_backup_with_directory_name(&db_path, backup_dir_name.clone()));

	// Try and back it up again to the same directory and it should fail
	assert!(create_backup_with_directory_name(&db_path, backup_dir_name).is_err());
}

#[test]
// TODO: Re-enable this test for linux. We currently do this because Github Actions must run with
// root user. And so the readonly permissions will be ignored.
#[cfg(not(target_os = "linux"))]
fn backup_should_fail_if_cant_copy_files() {
	let (directory, db_path) = new_temp_directory_with_nonexistent_file();

	// Create a normal db
	assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None));
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
fn should_add_genesis_hash_if_missing() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let genesis_hash_added_later: state_chain_runtime::Hash = sp_core::H256::random();

	// Create a fresh db with no genesis hash
	open_db_and_write_version_data(&db_path, LATEST_SCHEMA_VERSION);

	// Open the db normally, so the genesis hash will be added
	{
		assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(
			&db_path,
			Some(genesis_hash_added_later),
		));
	}

	assert!(db_path.exists());
	{
		// Open the db file manually
		let db = DB::open_cf(&Options::default(), &db_path, COLUMN_FAMILIES)
			.expect("Should open db file");

		// Check that the genesis hash was added and is correct
		assert_eq!(
			read_genesis_hash(&db)
				.expect("Should read genesis hash")
				.expect("Should find genesis hash"),
			genesis_hash_added_later
		);
	}
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

		assert_eq!(read_schema_version(&db.kv_db.db).unwrap(), 0);
	}

	let db = PersistentKeyDB::open_and_migrate_to_latest(&db_file, None).unwrap();

	assert_eq!(read_schema_version(&db.kv_db.db).unwrap(), LATEST_SCHEMA_VERSION);
}

#[test]
fn test_migration_to_v1() {
	use crate::multisig::{client::keygen, Rng};
	use cf_primitives::AccountId;
	use rand_legacy::FromEntropy;
	use std::collections::BTreeSet;

	let (_dir, db_file) = utilities::testing::new_temp_directory_with_nonexistent_file();

	// create db with version 0
	let db = PersistentKeyDB::open_and_migrate_to_version(&db_file, None, 0).unwrap();

	let account_ids: BTreeSet<_> = [1, 2, 3].iter().map(|i| AccountId::new([*i; 32])).collect();

	let (public_key_bytes, key_data) =
		keygen::generate_key_data::<EthSigning>(account_ids, &mut Rng::from_entropy());

	let key_info = key_data.values().next().unwrap();

	// Sanity check: the key should not include the epoch index
	assert_eq!(public_key_bytes.len(), 33);

	// Insert the key manually, so it matches the way it was done in db version 0:
	{
		let key_id_with_prefix =
			[get_keygen_data_prefix::<EthSigning>().as_slice(), &public_key_bytes].concat();

		db.kv_db
			.db
			.put_cf(
				get_data_column_handle(&db.kv_db.db),
				key_id_with_prefix,
				bincode::serialize(key_info).expect("Couldn't serialize keygen result info"),
			)
			.unwrap();
	}

	// After migration, the we should be able to load the key using the new code
	migrate_0_to_1(&db.kv_db.db);

	let keys = db.load_keys::<EthSigning>();

	assert_eq!(keys.len(), 1);

	let (key_id_loaded, key_info_loaded) = keys.into_iter().next().unwrap();

	assert_eq!(key_id_loaded.epoch_index, 0);
	assert_eq!(key_id_loaded.public_key_bytes, public_key_bytes);
	assert_eq!(key_info, &key_info_loaded);
}
