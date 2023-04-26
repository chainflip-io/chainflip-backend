use sp_runtime::AccountId32;
use std::{collections::BTreeSet, fs, path::PathBuf};
use tempfile::TempDir;

use crate::{
	db::persistent::rocksdb_kv::create_backup,
	multisig::{client::get_key_data_for_test, eth::EthSigning, polkadot::PolkadotSigning},
};

use super::{rocksdb_kv::BACKUPS_DIRECTORY, *};
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

	let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
	let scheme_1_key_id = KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes: vec![0; 33] };
	let scheme_2_key_id = KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes: vec![1; 33] };

	// Create a normal db and save multiple keys to it of different crypto schemes
	{
		let p_db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		p_db.update_key::<Scheme1>(&scheme_1_key_id, &get_single_key_data::<Scheme1>());
		p_db.update_key::<Scheme2>(&scheme_2_key_id, &get_single_key_data::<Scheme2>());
	}

	// Open the db and load the keys of both types
	{
		let p_db = PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap();

		let scheme_1_keys = p_db.load_keys::<Scheme1>();
		assert_eq!(scheme_1_keys.len(), 1, "Incorrect number of keys loaded");
		assert!(scheme_1_keys.get(&scheme_1_key_id).is_some(), "Incorrect key id");

		let scheme_2_keys = p_db.load_keys::<Scheme2>();
		assert_eq!(scheme_2_keys.len(), 1, "Incorrect number of keys loaded");
		assert!(scheme_2_keys.get(&scheme_2_key_id).is_some(), "Incorrect key id");
	}
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
