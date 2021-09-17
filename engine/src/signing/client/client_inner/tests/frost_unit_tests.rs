use itertools::Itertools;

use super::*;

macro_rules! assert_stage {
    ($c1:expr, $stage:expr, $mi:expr) => {
        assert_eq!(helpers::get_stage_for_msg(&$c1, $mi), $stage);
    };
}

macro_rules! assert_no_stage {
    ($c1:expr, $mi:expr) => {
        assert_stage!(&$c1, None, $mi);
    };
}

macro_rules! assert_stage1 {
    ($c1:expr, $mi:expr) => {
        assert_stage!(
            $c1,
            Some("BroadcastStage<AwaitCommitments1>".to_string()),
            $mi
        );
    };
}

macro_rules! assert_stage2 {
    ($c1:expr, $mi:expr) => {
        assert_stage!(
            $c1,
            Some("BroadcastStage<VerifyCommitmentsBroadcast2>".to_string()),
            $mi
        );
    };
}

macro_rules! assert_stage3 {
    ($c1:expr, $mi:expr) => {
        assert_stage!($c1, Some("BroadcastStage<LocalSigStage3>".to_string()), $mi);
    };
}

macro_rules! assert_stage4 {
    ($c1:expr, $mi:expr) => {
        assert_stage!(
            $c1,
            Some("BroadcastStage<VerifyLocalSigsBroadcastStage4>".to_string()),
            $mi
        );
    };
}

macro_rules! receive_comm1 {
    ($c1:expr, $sender: expr, $sign_states:expr, $mi:expr) => {
        let comm1 = $sign_states.sign_phase1.comm1_vec[$sender].clone();
        let m = helpers::sig_data_to_p2p(comm1, &VALIDATOR_IDS[$sender], $mi);
        $c1.process_p2p_message(m);
    };
}

macro_rules! receive_ver2 {
    ($c1:expr, $sender: expr, $sign_states:expr, $mi:expr) => {
        let ver2 = $sign_states.sign_phase2.ver2_vec[$sender].clone();
        let m = helpers::sig_data_to_p2p(ver2, &VALIDATOR_IDS[$sender], $mi);
        $c1.process_p2p_message(m);
    };
}

macro_rules! receive_sig3 {
    ($c1:expr, $sender: expr, $sign_states:expr, $mi:expr) => {
        let sign_phase3 = $sign_states.sign_phase3.as_ref().expect("phase 3");
        let sig3 = sign_phase3.local_sigs[$sender].clone();
        let m = helpers::sig_data_to_p2p(sig3, &VALIDATOR_IDS[$sender], $mi);
        $c1.process_p2p_message(m);
    };
}

macro_rules! receive_ver4 {
    ($c1:expr, $sender: expr, $sign_states:expr, $mi:expr) => {
        let sign_phase4 = $sign_states.sign_phase4.as_ref().expect("phase 4");
        let ver4 = sign_phase4.ver4_vec[$sender].clone();
        let m = helpers::sig_data_to_p2p(ver4, &VALIDATOR_IDS[$sender], $mi);
        $c1.process_p2p_message(m);
    };
}

// Should be in AwaitCommitments1 stage right after a
// request to sign
#[tokio::test]
async fn should_await_comm1_after_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    let key = c1.get_key(key_id).expect("no key").to_owned();

    c1.signing_manager
        .on_request_to_sign(MESSAGE_HASH.clone(), key, sign_info);

    assert_stage1!(c1, &message_info);
}

// Should be able to correctly delay messages
// before the request to sign
#[tokio::test]
async fn should_delay_comm1_before_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    // "Slow" client c1 receives a message before a request to sign, it should be delayed
    receive_comm1!(c1, 1, sign_states, &message_info);

    assert_eq!(helpers::get_stage_for_msg(&c1, &message_info), None);

    // when c1 receives a request to sign, it processes the delayed message
    let key = c1.get_key(key_id).expect("no key").to_owned();
    c1.signing_manager
        .on_request_to_sign(MESSAGE_HASH.clone(), key, sign_info.clone());

    assert_stage1!(c1, &message_info);

    // One more comm1 should advance us to the next stage
    receive_comm1!(c1, 2, sign_states, &message_info);

    assert_stage2!(c1, &message_info);
}

#[tokio::test]
async fn should_delay_ver2() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = sign_states.sign_phase1.clients[0].clone();

    assert_stage1!(c1, &message_info);

    // "Slow" client c1 receives a ver2 message before stage 2, it should be delayed
    receive_comm1!(c1, 1, sign_states, &message_info);
    receive_ver2!(c1, 1, sign_states, &message_info);

    assert_stage1!(c1, &message_info);

    // c1 finally receives the remaining comm1, which advances us to stage 2
    receive_comm1!(c1, 2, sign_states, &message_info);
    assert_stage2!(c1, &message_info);

    // Because we have already processed the delayed message, just one more
    // message should be enough to advance us to stage 3
    receive_ver2!(c1, 2, sign_states, &message_info);

    assert_stage3!(c1, &message_info);
}

#[tokio::test]
async fn should_delay_sig3() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = sign_states.sign_phase2.clients[0].clone();

    assert_stage2!(c1, &message_info);

    // "Slow" client c1 receives a sig3 message before stage 3, it should be delayed
    receive_ver2!(c1, 1, sign_states, &message_info);
    receive_sig3!(c1, 1, &sign_states, &message_info);
    assert_stage2!(c1, &message_info);

    // This should advance us to the next stage and trigger processing of the delayed message
    receive_ver2!(c1, 2, sign_states, &message_info);
    assert_stage3!(c1, &message_info);

    // Because we have already processed the delayed message, just one more
    // message should be enough to advance us to stage 4
    receive_sig3!(c1, 2, &sign_states, &message_info);
    assert_stage4!(c1, &message_info);
}

#[tokio::test]
async fn should_delay_ver4() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = sign_states.sign_phase3.as_ref().unwrap().clients[0].clone();

    assert_stage3!(c1, &message_info);

    // "Slow" client c1 receives a ver4 message before stage 4, it should be delayed
    receive_sig3!(c1, 1, &sign_states, &message_info);
    receive_ver4!(c1, 1, sign_states, &message_info);

    assert_stage3!(c1, &message_info);

    // This should trigger processing of the delayed message
    receive_sig3!(c1, 2, &sign_states, &message_info);

    assert_stage4!(c1, &message_info);

    // Because we have already processed the delayed message, just one more
    // message should be enough to create the signature (stage becomes None)
    receive_ver4!(c1, 2, sign_states, &message_info);
    assert_no_stage!(c1, &message_info);

    // TODO: check that we've created a signature!
}

// ********************** Handle invalid local sigs **********************

#[tokio::test]
async fn should_handle_invalid_local_sig() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // Party at this idx will send an invalid signature
    let bad_idx = 1;

    ctx.use_invalid_local_sig(bad_idx);

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
}

#[tokio::test]
async fn should_handle_inconsistent_broadcast_com1() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // Party at this idx will send and invalid signature
    let bad_idx = 1;

    ctx.use_inconsistent_broadcast_for_comm1(bad_idx, 0);
    ctx.use_inconsistent_broadcast_for_comm1(bad_idx, 2);

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
}

#[tokio::test]
async fn should_handle_inconsistent_broadcast_sig3() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // Party at this idx will send and invalid signature

    // This is the index in the array
    let bad_idx = 1;

    ctx.use_inconsistent_broadcast_for_sig3(bad_idx, 0);
    ctx.use_inconsistent_broadcast_for_sig3(bad_idx, 2);

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
}

#[tokio::test]
async fn should_report_on_timeout_before_reqeust_to_sign() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // MAXIM: deal with this boilerplate
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let (message_info, sign_info) =
        helpers::message_and_sign_info(MESSAGE_HASH.clone(), key_id.clone());
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    assert_no_stage!(c1, &message_info);

    let bad_array_idxs = [1usize, 2];

    for idx in bad_array_idxs.iter() {
        receive_comm1!(c1, *idx, sign_states, &message_info);
    }

    assert_no_stage!(c1, &message_info);

    c1.expire_all();
    c1.cleanup();

    let (_, blamed_parties) = helpers::check_outcome(&mut ctx.rxs[0])
        .await
        .expect("should procude outcome")
        .result
        .clone()
        .unwrap_err();

    assert_eq!(
        blamed_parties,
        bad_array_idxs
            .iter()
            // Needs +1 to map from array idx to signer idx
            .map(|idx| AccountId([*idx as u8 + 1; 32]))
            .collect_vec()
    );
}
