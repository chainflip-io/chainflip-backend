use super::*;
use crate::{
    logging::{self},
    multisig::{
        client::{self},
        KeyDBMock, KeyId, MessageHash,
    },
    testing::{assert_err, assert_future_can_complete},
};

use client::MultisigClient;

#[tokio::test]
async fn should_ignore_rts_for_unknown_key() {
    let account_id = &ACCOUNT_IDS[0];

    let key_id = KeyId(Vec::from([0u8; 32]));
    let (keygen_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
    let (signing_request_sender, _) = tokio::sync::mpsc::unbounded_channel();

    let client = MultisigClient::new(
        account_id.clone(),
        KeyDBMock::default(),
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

    // Check sign request completes after signature is provided
    let error = assert_err!(assert_future_can_complete(signing_request_fut));
    assert_eq!(&error.1.to_string(), "Signing request ignored: unknown key");
}
