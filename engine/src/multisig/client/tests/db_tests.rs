use crate::{
    logging,
    multisig::{
        client::{
            key_store::KeyStore,
            keygen::KeygenOptions,
            tests::{new_nodes, ACCOUNT_IDS},
        },
        PersistentKeyDB,
    },
    testing::{assert_ok, new_temp_dir},
};

use super::helpers::run_keygen;

#[tokio::test]
async fn check_signing_db() {
    // Generate a key to use in this test
    let (key_id, stored_keygen_result_info) = {
        let (key_id, key_data, _, _) = run_keygen(
            new_nodes(ACCOUNT_IDS.clone()),
            1,
            KeygenOptions::allowing_high_pubkey(),
        )
        .await;
        (key_id, key_data.into_iter().next().unwrap().1)
    };

    let logger = logging::test_utils::new_test_logger();

    let (_dir, db_file) = new_temp_dir();
    let db = PersistentKeyDB::new(&db_file, &logger).expect("Failed to open database");

    let db_with_key = {
        let mut key_store = KeyStore::new(db);
        key_store.set_key(key_id.clone(), stored_keygen_result_info.clone());
        key_store.extract_db()
    };

    // Reload DB
    let key_store = KeyStore::new(db_with_key);

    let loaded_keygen_result_info = assert_ok!(key_store.get_key(&key_id));

    // Check the loaded and stored keys match
    assert_eq!(*loaded_keygen_result_info, stored_keygen_result_info);
}
