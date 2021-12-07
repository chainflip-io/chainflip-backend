use futures::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{
    logging,
    multisig::{
        client::{keygen::KeygenOptions, MultisigClient},
        MultisigDBMock,
    },
};

use super::helpers;

#[tokio::test]
async fn check_signing_db() {
    // TODO: This uses an in-memory database mock, which might behave a
    // little different from rocks-db used in production. Either find a
    // better mock or use the actual DB here. (kvdb-memorydb doesn't quite
    // work as the tests need the database to by `Copy` and wrapping in
    // Rc/Arc is not an option)
    let mut ctx = helpers::KeygenContext::new();

    // 1. Generate a key. It should automatically be written to a database
    let _ = ctx.generate().await;

    let account_id = ctx.get_account_id(0);

    // 2. Extract the clients' database
    let client1 = ctx.get_client(&account_id);
    let db = client1.get_db();

    // 3. Create a new multisig client using the extracted database
    let id = client1.get_my_account_id();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (p2p_tx, p2p_rx) = tokio::sync::mpsc::unbounded_channel();
    let logger = logging::test_utils::new_test_logger();
    let mut restarted_client = MultisigClient::new(
        id,
        MultisigDBMock::new(),
        tx,
        p2p_tx,
        KeygenOptions::allowing_high_pubkey(),
        &logger,
    );

    restarted_client.set_db(db);

    // 4. Replace the client
    ctx.substitute_client_at(
        &account_id,
        restarted_client,
        Box::pin(UnboundedReceiverStream::new(rx).peekable()),
        Box::pin(UnboundedReceiverStream::new(p2p_rx).peekable()),
    );

    // 5. Signing should not crash
    ctx.sign().await;
}
