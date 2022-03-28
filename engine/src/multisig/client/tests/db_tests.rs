use crate::{
    logging,
    multisig::client::{keygen::KeygenOptions, MultisigClient},
};

use super::{
    helpers::{new_signing_ceremony_with_keygen, Node},
    standard_signing,
};

/* TODO NOW
#[tokio::test]
async fn check_signing_db() {
    // TODO: This uses an in-memory database mock, which might behave a
    // little different from rocks-db used in production. Either find a
    // better mock or use the actual DB here. (kvdb-memorydb doesn't quite
    // work as the tests need the database to by `Copy` and wrapping in
    // Rc/Arc is not an option)

    // 1. Generate a key. It should automatically be written to a database.
    let (mut signing_ceremony, _) = new_signing_ceremony_with_keygen().await;

    // 2. Extract the client's database
    let [account_id] = signing_ceremony.select_account_ids();
    let node = signing_ceremony.get_mut_node(&account_id);
    let db = node.client.get_db().clone();

    // 3. Create a new node (with new client) using the extracted database
    let (multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let logger = logging::test_utils::new_test_logger();
    let restarted_client = MultisigClient::new(
        account_id.clone(),
        db,
        multisig_outcome_sender,
        outgoing_p2p_message_sender,
        KeygenOptions::allowing_high_pubkey(),
        &logger,
    );

    let substituted_node = Node {
        client: restarted_client,
        multisig_outcome_receiver,
        outgoing_p2p_message_receiver,
        tag_cache: node.tag_cache.clone(),
    };

    // 4. Replace the node
    *node = substituted_node;

    // 5. Signing should not crash
    standard_signing(&mut signing_ceremony).await;
}
*/
