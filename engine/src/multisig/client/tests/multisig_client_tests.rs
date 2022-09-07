use std::{collections::BTreeSet, sync::Arc};

use super::*;
use crate::{
    logging::{
        test_utils::{new_test_logger, new_test_logger_with_tag_cache},
        REQUEST_TO_SIGN_IGNORED,
    },
    multisig::{
        client::{
            self,
            common::{CeremonyFailureReason, SigningFailureReason},
            key_store::KeyStore,
            CeremonyRequestDetails,
        },
        eth::EthSigning,
        KeyId, MessageHash, PersistentKeyDB,
    },
    testing::{assert_future_can_complete, new_temp_directory_with_nonexistent_file},
};

use client::MultisigClient;
use utilities::{assert_err, assert_ok};

#[tokio::test]
async fn should_ignore_rts_for_unknown_key() {
    let account_id = &ACCOUNT_IDS[0];
    let (logger, tag_cache) = new_test_logger_with_tag_cache();
    let (_dir, db_file) = new_temp_directory_with_nonexistent_file();

    // Use any key id, as the key db will be empty
    let key_id = KeyId(Vec::from([0u8; 32]));

    // Create a client
    let (ceremony_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
    let client = MultisigClient::<EthSigning>::new(
        account_id.clone(),
        KeyStore::new(Arc::new(
            PersistentKeyDB::new_and_migrate_to_latest(&db_file, None, &logger)
                .expect("Failed to open database"),
        )),
        ceremony_request_sender,
        &logger,
    );

    // Send Sign Request
    let signing_request_fut = client.initiate_signing(
        DEFAULT_SIGNING_CEREMONY_ID,
        key_id,
        BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
        MessageHash([0; 32]),
    );

    // Check sign request fails immediately with "unknown key" error
    let (_, failure_reason) = assert_err!(assert_future_can_complete(signing_request_fut));
    assert_eq!(
        failure_reason,
        CeremonyFailureReason::Other(SigningFailureReason::UnknownKey)
    );

    // Check that the signing failure reason is being logged
    assert!(tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_save_key_after_keygen() {
    let logger = new_test_logger();
    let (_dir, db_file) = new_temp_directory_with_nonexistent_file();

    // Generate a key to use in this test
    let (key_id, keygen_result_info) = {
        let (key_id, key_data, _, _) =
            helpers::run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
        (key_id, key_data.into_iter().next().unwrap().1)
    };

    {
        // Create a client
        let (ceremony_request_sender, mut ceremony_request_receiver) =
            tokio::sync::mpsc::unbounded_channel();
        let client = MultisigClient::<EthSigning>::new(
            ACCOUNT_IDS[0].clone(),
            KeyStore::new(Arc::new(
                PersistentKeyDB::new_and_migrate_to_latest(&db_file, None, &logger)
                    .expect("Failed to open database"),
            )),
            ceremony_request_sender,
            &logger,
        );

        // Send Keygen Request
        let keygen_request_fut = client.initiate_keygen(
            DEFAULT_KEYGEN_CEREMONY_ID,
            BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
        );

        // Get the oneshot channel that is linked to the keygen request
        // and send a successful keygen result
        let request = ceremony_request_receiver.recv().await.unwrap();
        match request.details.unwrap() {
            CeremonyRequestDetails::Keygen(details) => {
                details.result_sender.send(Ok(keygen_result_info)).unwrap();
            }
            _ => {
                panic!("Unexpected ceremony request");
            }
        }

        // Complete the keygen request
        assert_ok!(keygen_request_fut.await);
    }

    // Check that the key was saved by Loading it from the same db file
    let key_store = KeyStore::<EthSigning>::new(Arc::new(
        PersistentKeyDB::new_and_migrate_to_latest(&db_file, None, &logger)
            .expect("Failed to open database"),
    ));
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
    let logger = new_test_logger();
    {
        let mut key_store = KeyStore::<EthSigning>::new(Arc::new(
            PersistentKeyDB::new_and_migrate_to_latest(&db_file, None, &logger)
                .expect("Failed to open database"),
        ));
        key_store.set_key(key_id.clone(), stored_keygen_result_info.clone());
    }

    // Create the client using the existing db file
    let (ceremony_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
    let client = MultisigClient::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        KeyStore::new(Arc::new(
            PersistentKeyDB::new_and_migrate_to_latest(&db_file, None, &logger)
                .expect("Failed to open database"),
        )),
        ceremony_request_sender,
        &new_test_logger(),
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
