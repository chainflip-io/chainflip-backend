use sp_runtime::AccountId32;
use std::{collections::BTreeSet, fs, path::PathBuf};
use tempfile::TempDir;

use crate::multisig::{
	bitcoin::BtcSigning, client::get_key_data_for_test, eth::EthSigning, polkadot::PolkadotSigning,
};

use super::*;
use cf_primitives::GENESIS_EPOCH;
use utilities::{assert_ok, testing::new_temp_directory_with_nonexistent_file};

#[test]
fn should_save_and_load_checkpoint() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let test_checkpoint = WitnessedUntil { epoch_index: 69, block_number: 420 };

	let chain = ChainTag::Ethereum;
	// Open a fresh db and write the checkpoint to it
	{
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		assert!(db.load_checkpoint(chain).unwrap().is_none());

		db.update_checkpoint(chain, &test_checkpoint);
	}

	// Open the db file again and load the checkpoint
	{
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		assert_eq!(db.load_checkpoint(chain).unwrap(), Some(test_checkpoint));
	}
}

fn get_single_key_data<C: CryptoScheme>() -> KeygenResultInfo<C> {
	get_key_data_for_test::<C>(BTreeSet::from_iter([AccountId32::new([0; 32])]))
}

#[test]
fn can_use_multiple_crypto_schemes() {
	type Scheme1 = EthSigning;
	type Scheme2 = PolkadotSigning;
	type Scheme3 = BtcSigning;

	fn add_key_for_scheme<Scheme: CryptoScheme>(db: &PersistentKeyDB) -> KeyId {
		let key_id = KeyId {
			epoch_index: GENESIS_EPOCH,
			public_key_bytes: rand::random::<[u8; 32]>().into(),
		};
		db.update_key(&key_id, &get_single_key_data::<Scheme>());
		key_id
	}

	fn ensure_loaded_one_key<Scheme: CryptoScheme>(db: &PersistentKeyDB, expected_key: &KeyId) {
		let keys = db.load_keys::<Scheme>();
		assert_eq!(keys.len(), 1, "Incorrect number of keys loaded");
		assert!(keys.contains_key(expected_key), "Incorrect key id");
	}

	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	// Create a normal db and save multiple keys to it of different crypto schemes
	let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

	let key_1 = add_key_for_scheme::<Scheme1>(&db);
	let key_2 = add_key_for_scheme::<Scheme2>(&db);
	let key_3 = add_key_for_scheme::<Scheme3>(&db);

	drop(db);

	// Open the db and load the keys of all types
	let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

	ensure_loaded_one_key::<Scheme1>(&db, &key_1);
	ensure_loaded_one_key::<Scheme2>(&db, &key_2);
	ensure_loaded_one_key::<Scheme3>(&db, &key_3);
}

#[test]
fn can_load_keys_with_current_keygen_info() {
	type Scheme = EthSigning;

	// Just a random key
	const TEST_KEY: [u8; 33] = [
		3, 3, 94, 73, 229, 219, 117, 193, 0, 143, 51, 247, 54, 138, 135, 255, 177, 63, 13, 132, 93,
		195, 249, 200, 151, 35, 228, 224, 122, 6, 111, 38, 103,
	];

	let key_id = KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes: TEST_KEY.into() };
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	{
		let p_db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		p_db.update_key::<Scheme>(&key_id, &get_single_key_data::<Scheme>());
	}

	{
		let p_db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();
		let keys = p_db.load_keys::<Scheme>();
		let key = keys.get(&key_id).expect("Should have an entry for key");
		// single party keygen has a threshold of 0
		assert_eq!(key.params.threshold, 0);
	}
}

#[test]
fn can_update_key() {
	type Scheme = EthSigning;

	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let key_id = KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes: vec![0; 33] };

	let p_db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

	let keys_before = p_db.load_keys::<Scheme>();
	// there should be no key [0; 33] yet
	assert!(keys_before.get(&key_id).is_none());

	p_db.update_key::<Scheme>(&key_id, &get_single_key_data::<Scheme>());

	let keys_before = p_db.load_keys::<Scheme>();
	assert!(keys_before.get(&key_id).is_some());
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

#[test]
fn can_load_key_from_backup() {
	type Scheme = EthSigning;

	let (directory, db_path) = new_temp_directory_with_nonexistent_file();
	let key_id = KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes: vec![0; 33] };

	// Create a normal db and save a key in it
	{
		let p_db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

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
			PersistentKeyDB::open_and_migrate_to_latest(backups.first().unwrap(), None).unwrap();

		assert!(p_db.load_keys::<Scheme>().get(&key_id).is_some());
	}
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
	db.kv_db
		.put_data(keygen_data_prefix::<EthSigning>().as_slice(), &public_key_bytes, key_info)
		.unwrap();

	// After migration, we should be able to load the key using the new code
	migrate_0_to_1(&db);

	let keys = db.load_keys::<EthSigning>();

	assert_eq!(keys.len(), 1);

	let (key_id_loaded, key_info_loaded) = keys.into_iter().next().unwrap();

	assert_eq!(key_id_loaded.epoch_index, 0);
	assert_eq!(key_id_loaded.public_key_bytes, public_key_bytes);
	assert_eq!(key_info, &key_info_loaded);
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
fn should_not_migrate_backwards() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	// Create a db with schema version + 1
	{
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();
		db.put_schema_version(LATEST_SCHEMA_VERSION + 1).unwrap();
	}

	// Open the db and make sure the migration errors
	assert!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).is_err());
}

#[test]
fn can_create_new_database() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None));
	assert!(db_path.exists());
}

#[test]
fn new_db_returns_db_when_db_data_version_is_latest() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	{
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();
		db.put_schema_version(LATEST_SCHEMA_VERSION).unwrap();
	}
	assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None));
}

#[test]
fn new_db_is_created_with_correct_metadata() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let starting_genesis_hash: state_chain_runtime::Hash = sp_core::H256::random();

	// Create a fresh db. This will write the schema version and genesis hash
	assert_ok!(PersistentKeyDB::open_and_migrate_to_latest(&db_path, Some(starting_genesis_hash),));

	assert!(db_path.exists());

	// Open the db again, and check metadata
	let db =
		PersistentKeyDB::open_and_migrate_to_latest(&db_path, Some(starting_genesis_hash)).unwrap();

	// Check the schema version is at the latest

	assert_eq!(db.get_schema_version().expect("Should read schema version"), LATEST_SCHEMA_VERSION);

	// Check the genesis hash exists and matches the one we provided
	assert_eq!(
		db.get_genesis_hash()
			.expect("Should read genesis hash")
			.expect("Should find genesis hash"),
		starting_genesis_hash
	);
}

#[test]
fn should_add_genesis_hash_if_missing() {
	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let genesis_hash_added_later: state_chain_runtime::Hash = sp_core::H256::random();

	// Create a fresh db with no genesis hash
	{
		PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();
	}

	// Open the db normally, so the genesis hash will be added
	let db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, Some(genesis_hash_added_later))
		.unwrap();

	// Check that the genesis hash was added and is correct
	assert_eq!(
		db.get_genesis_hash()
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

		assert_eq!(db.get_schema_version().unwrap(), 0);
	}

	let db = PersistentKeyDB::open_and_migrate_to_latest(&db_file, None).unwrap();

	assert_eq!(db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);
}
