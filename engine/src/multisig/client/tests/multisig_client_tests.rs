use super::*;
use crate::{
    logging::{self},
    multisig::{
        client::{self, key_store::KeyStore},
        eth::{EthSigning, Point as EthPoint},
        KeyId, MessageHash, PersistentKeyDB,
    },
    testing::{
        assert_err, assert_future_can_complete, assert_ok, new_temp_directory_with_nonexistent_file,
    },
};

use client::MultisigClient;

#[tokio::test]
async fn should_ignore_rts_for_unknown_key() {
    let account_id = &ACCOUNT_IDS[0];
    let logger = logging::test_utils::new_test_logger();
    let (_dir, db_file) = new_temp_directory_with_nonexistent_file();

    // Use any key id, as the key db will be empty
    let key_id = KeyId(Vec::from([0u8; 32]));

    // Create a client
    let (keygen_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
    let (signing_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
    let client = MultisigClient::<_, EthSigning>::new(
        account_id.clone(),
        PersistentKeyDB::new_and_migrate_to_latest(&db_file, &logger)
            .expect("Failed to open database"),
        keygen_request_sender,
        signing_request_sender,
        &logging::test_utils::new_test_logger(),
    );

    // Send Sign Request
    let signing_request_fut = client.initiate_signing(
        DEFAULT_SIGNING_CEREMONY_ID,
        key_id,
        ACCOUNT_IDS.to_vec(),
        MessageHash([0; 32]),
    );

    // Check sign request fails immediately with "unknown key" error
    let error = assert_err!(assert_future_can_complete(signing_request_fut));
    // TODO: [SC-3352] Check the reason for failure in multisig tests #1552
    assert_eq!(&error.1.to_string(), "Signing request ignored: unknown key");
}

#[tokio::test]
async fn should_save_key_after_keygen() {
    let logger = logging::test_utils::new_test_logger();
    let (_dir, db_file) = new_temp_directory_with_nonexistent_file();

    // Generate a key to use in this test
    let (key_id, keygen_result_info) = {
        let (key_id, key_data, _, _) =
            helpers::run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
        (key_id, key_data.into_iter().next().unwrap().1)
    };

    {
        // Create a client
        let (keygen_request_sender, mut keygen_request_receiver) =
            tokio::sync::mpsc::unbounded_channel();
        let (signing_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
        let client = MultisigClient::<_, EthSigning>::new(
            ACCOUNT_IDS[0].clone(),
            PersistentKeyDB::<EthPoint>::new_and_migrate_to_latest(&db_file, &logger)
                .expect("Failed to open database"),
            keygen_request_sender,
            signing_request_sender,
            &logging::test_utils::new_test_logger(),
        );

        // Send Keygen Request
        let keygen_request_fut =
            client.initiate_keygen(DEFAULT_KEYGEN_CEREMONY_ID, ACCOUNT_IDS.to_vec());

        // Get the oneshot channel that is linked to the keygen request
        // and send a successful keygen result
        keygen_request_receiver
            .recv()
            .await
            .unwrap()
            .3
            .send(Ok(keygen_result_info))
            .unwrap();

        // Complete the keygen request
        assert_ok!(keygen_request_fut.await);
    }

    // Check that the key was saved by Loading it from the same db file
    let key_store = KeyStore::<_, EthPoint>::new(
        PersistentKeyDB::new_and_migrate_to_latest(&db_file, &logger)
            .expect("Failed to open database"),
    );
    assert!(key_store.get_key(&key_id).is_some(), "Key not found in db");
}

#[tokio::test]
async fn should_load_keys_on_creation() {
    // Generate a key to use in this test
    let (key_id, stored_keygen_result_info) = {
        let (key_id, key_data, _, _) =
            helpers::run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
        (key_id, key_data.into_iter().next().unwrap().1)
    };

    // A temp directory to store the key db for this test
    let (_dir, db_file) = new_temp_directory_with_nonexistent_file();

    // Create a new db and store the key in it
    let logger = logging::test_utils::new_test_logger();
    {
        let mut key_store = KeyStore::new(
            PersistentKeyDB::new_and_migrate_to_latest(&db_file, &logger)
                .expect("Failed to open database"),
        );
        key_store.set_key(key_id.clone(), stored_keygen_result_info.clone());
    }

    // Create the client using the existing db file
    let (keygen_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
    let (signing_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
    let client = MultisigClient::<_, EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        PersistentKeyDB::new_and_migrate_to_latest(&db_file, &logger)
            .expect("Failed to open database"),
        keygen_request_sender,
        signing_request_sender,
        &logging::test_utils::new_test_logger(),
    );

    // Check that the key was loaded during the creation of the client and it matches the original key
    assert_eq!(
        *client
            .key_store
            .lock()
            .unwrap()
            .get_key(&key_id)
            .expect("Key not found in db"),
        stored_keygen_result_info
    );
}
