use std::{
	collections::BTreeSet,
	fs,
	path::{Path, PathBuf},
};

use super::*;

use crate::{
	logging::test_utils::new_test_logger,
	multisig::{
		client::get_key_data_for_test,
		crypto::polkadot::PolkadotSigning,
		db::persistent::{
			create_backup, create_backup_with_directory_name, migrate_db_to_latest,
			LATEST_SCHEMA_VERSION,
		},
		eth::EthSigning,
		KeyId, PersistentKeyDB,
	},
	testing::new_temp_directory_with_nonexistent_file,
};
use rocksdb::{Options, DB};
use sp_runtime::AccountId32;
use tempfile::TempDir;
use utilities::assert_ok;

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

fn find_backups(temp_dir: &TempDir, db_path: PathBuf) -> Result<Vec<PathBuf>, std::io::Error> {
	let backups_path = temp_dir.path().join(BACKUPS_DIRECTORY);

	let backups: Vec<PathBuf> = fs::read_dir(backups_path)?
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

fn get_single_key_data<C: CryptoScheme>() -> KeygenResultInfo<C> {
	get_key_data_for_test::<C>(BTreeSet::from_iter([AccountId32::new([0; 32])]))
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
	assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger));
	assert!(db_path.exists());
}

#[test]
fn new_db_is_created_with_correct_metadata() {
	let logger = new_test_logger();
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let starting_genesis_hash: state_chain_runtime::Hash = sp_core::H256::random();

	// Create a fresh db. This will write the schema version and genesis hash
	assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(
		&db_path,
		Some(starting_genesis_hash),
		&logger
	));

	assert!(db_path.exists());
	{
		// Open the db file manually
		let db = DB::open_cf(&Options::default(), &db_path, COLUMN_FAMILIES)
			.expect("Should open db file");

		// Check the schema version is at the latest
		assert_eq!(
			read_schema_version(&db, &logger).expect("Should read schema version"),
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
	assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &new_test_logger()));
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
		assert!(migrate_db_to_latest(db, &db_path, None, &new_test_logger()).is_err());
	}
}

#[test]
fn can_load_keys_with_current_keygen_info() {
	type Scheme = EthSigning;

	let logger = new_test_logger();

	let key_id = KeyId(TEST_KEY.into());
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	{
		let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

		p_db.update_key::<Scheme>(&key_id, &get_single_key_data::<Scheme>());
	}

	{
		let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();
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

	let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

	let keys_before = p_db.load_keys::<Scheme>();
	// there should be no key [0; 33] yet
	assert!(keys_before.get(&key_id).is_none());

	p_db.update_key::<Scheme>(&key_id, &get_single_key_data::<Scheme>());

	let keys_before = p_db.load_keys::<Scheme>();
	assert!(keys_before.get(&key_id).is_some());
}

#[test]
fn backup_should_fail_if_already_exists() {
	let logger = new_test_logger();
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	// Create a normal db
	assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger));

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
	let logger = new_test_logger();
	let (directory, db_path) = new_temp_directory_with_nonexistent_file();

	// Create a normal db
	assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger));
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
		let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

		p_db.update_key::<Scheme>(&key_id, &get_single_key_data::<Scheme>());
	}

	// Do a backup of the db,
	assert_ok!(create_backup(&db_path, LATEST_SCHEMA_VERSION));

	// Try and open the backup to make sure it still works
	{
		let backups = find_backups(&directory, db_path).unwrap();
		assert!(backups.len() == 1, "Incorrect number of backups found in {BACKUPS_DIRECTORY}");

		// Should be able to open the backup and load the key
		let p_db =
			PersistentKeyDB::new_and_migrate_to_latest(backups.first().unwrap(), None, &logger)
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
		let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

		p_db.update_key::<Scheme1>(&scheme_1_key_id, &get_single_key_data::<Scheme1>());
		p_db.update_key::<Scheme2>(&scheme_2_key_id, &get_single_key_data::<Scheme2>());
	}

	// Open the db and load the keys of both types
	{
		let p_db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

		let scheme_1_keys = p_db.load_keys::<Scheme1>();
		assert_eq!(scheme_1_keys.len(), 1, "Incorrect number of keys loaded");
		assert!(scheme_1_keys.get(&scheme_1_key_id).is_some(), "Incorrect key id");

		let scheme_2_keys = p_db.load_keys::<Scheme2>();
		assert_eq!(scheme_2_keys.len(), 1, "Incorrect number of keys loaded");
		assert!(scheme_2_keys.get(&scheme_2_key_id).is_some(), "Incorrect key id");
	}
}

#[test]
fn should_add_genesis_hash_if_missing() {
	let logger = new_test_logger();
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let genesis_hash_added_later: state_chain_runtime::Hash = sp_core::H256::random();

	// Create a fresh db with no genesis hash
	open_db_and_write_version_data(&db_path, LATEST_SCHEMA_VERSION);

	// Open the db normally, so the genesis hash will be added
	{
		assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(
			&db_path,
			Some(genesis_hash_added_later),
			&logger
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
	let logger = new_test_logger();
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let genesis_hash_1 = sp_core::H256::from_low_u64_be(1);
	let genesis_hash_2 = sp_core::H256::from_low_u64_be(2);
	assert_ne!(genesis_hash_1, genesis_hash_2);

	// Open the db, so hash 1 is written
	{
		assert_ok!(PersistentKeyDB::new_and_migrate_to_latest(
			&db_path,
			Some(genesis_hash_1),
			&logger
		));
	}

	// Open the db again, but with hash 2, so it should compare them and return an error
	{
		assert!(PersistentKeyDB::new_and_migrate_to_latest(
			&db_path,
			Some(genesis_hash_2),
			&logger
		)
		.is_err());
	}
}

#[test]
fn should_save_and_load_checkpoint() {
	let logger = new_test_logger();

	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let test_checkpoint = WitnessedUntil { epoch_index: 69, block_number: 420 };

	let chain = ChainTag::Ethereum;
	// Open a fresh db and write the checkpoint to it
	{
		let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

		assert!(db.load_checkpoint(chain).unwrap().is_none());

		db.update_checkpoint(chain, &test_checkpoint);
	}

	// Open the db file again and load the checkpoint
	{
		let db = PersistentKeyDB::new_and_migrate_to_latest(&db_path, None, &logger).unwrap();

		assert_eq!(db.load_checkpoint(chain).unwrap(), Some(test_checkpoint));
	}
}
