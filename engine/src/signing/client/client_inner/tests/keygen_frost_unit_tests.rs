use super::helpers;

// Should be in AwaitCommitments1 stage right after a
// request to sign
#[tokio::test]
async fn basic_keygen() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // let mut c1 = keygen_states.key_ready.clients[0].clone();

    // let key = keygen_states.key_ready.sec_keys[0].clone();

    // c1.signing_manager.on_request_to_sign(
    //     MESSAGE_HASH.clone(),
    //     key,
    //     SIGNER_IDS.clone(),
    //     SIGN_CEREMONY_ID,
    // );

    // assert_stage1!(c1);
}
