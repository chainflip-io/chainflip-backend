use itertools::Itertools;

use super::*;

macro_rules! assert_stage {
    ($c1:expr, $stage:expr) => {
        assert_eq!(helpers::get_stage_for_default_ceremony(&$c1), $stage);
    };
}

macro_rules! assert_no_stage {
    ($c1:expr) => {
        assert_stage!(&$c1, None);
    };
}

macro_rules! assert_stage1 {
    ($c1:expr) => {
        assert_stage!($c1, Some("BroadcastStage<AwaitCommitments1>".to_string()));
    };
}

macro_rules! assert_stage2 {
    ($c1:expr) => {
        assert_stage!(
            $c1,
            Some("BroadcastStage<VerifyCommitmentsBroadcast2>".to_string())
        );
    };
}

macro_rules! assert_stage3 {
    ($c1:expr) => {
        assert_stage!($c1, Some("BroadcastStage<LocalSigStage3>".to_string()));
    };
}

macro_rules! assert_stage4 {
    ($c1:expr) => {
        assert_stage!(
            $c1,
            Some("BroadcastStage<VerifyLocalSigsBroadcastStage4>".to_string())
        );
    };
}

macro_rules! receive_comm1 {
    ($c1:expr, $sender: expr, $sign_states:expr) => {
        let comm1 = $sign_states.sign_phase1.comm1_vec[$sender].clone();
        let m = helpers::sig_data_to_p2p(comm1, &VALIDATOR_IDS[$sender]);
        $c1.process_p2p_message(m);
    };
}

macro_rules! receive_ver2 {
    ($c1:expr, $sender: expr, $sign_states:expr) => {
        let ver2 = $sign_states.sign_phase2.ver2_vec[$sender].clone();
        let m = helpers::sig_data_to_p2p(ver2, &VALIDATOR_IDS[$sender]);
        $c1.process_p2p_message(m);
    };
}

macro_rules! receive_sig3 {
    ($c1:expr, $sender: expr, $sign_states:expr) => {
        let sign_phase3 = $sign_states.sign_phase3.as_ref().expect("phase 3");
        let sig3 = sign_phase3.local_sigs[$sender].clone();
        let m = helpers::sig_data_to_p2p(sig3, &VALIDATOR_IDS[$sender]);
        $c1.process_p2p_message(m);
    };
}

macro_rules! receive_ver4 {
    ($c1:expr, $sender: expr, $sign_states:expr) => {
        let sign_phase4 = $sign_states.sign_phase4.as_ref().expect("phase 4");
        let ver4 = sign_phase4.ver4_vec[$sender].clone();
        let m = helpers::sig_data_to_p2p(ver4, &VALIDATOR_IDS[$sender]);
        $c1.process_p2p_message(m);
    };
}

// Should be in AwaitCommitments1 stage right after a
// request to sign
#[tokio::test]
async fn should_await_comm1_after_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    let key = keygen_states.key_ready.sec_keys[0].clone();

    c1.signing_manager.on_request_to_sign(
        MESSAGE_HASH.clone(),
        key,
        SIGNER_IDS.clone(),
        SIGN_CEREMONY_ID,
    );

    assert_stage1!(c1);
}

// Should be able to correctly delay messages
// before the request to sign
#[tokio::test]
async fn should_delay_comm1_before_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let sign_states = ctx.sign().await;

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    // "Slow" client c1 receives a message before a request to sign, it should be delayed
    receive_comm1!(c1, 1, sign_states);

    assert_no_stage!(c1);

    let key = keygen_states.key_ready.sec_keys[0].clone();

    // when c1 receives a request to sign, it processes the delayed message
    c1.signing_manager.on_request_to_sign(
        MESSAGE_HASH.clone(),
        key,
        SIGNER_IDS.clone(),
        SIGN_CEREMONY_ID,
    );

    assert_stage1!(c1);

    // One more comm1 should advance us to the next stage
    receive_comm1!(c1, 2, sign_states);

    assert_stage2!(c1);
}

#[tokio::test]
async fn should_delay_ver2() {
    let mut ctx = helpers::KeygenContext::new();
    let _keygen_states = ctx.generate().await;

    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase1.clients[0].clone();

    assert_stage1!(c1);

    // "Slow" client c1 receives a ver2 message before stage 2, it should be delayed
    receive_comm1!(c1, 1, sign_states);
    receive_ver2!(c1, 1, sign_states);

    assert_stage1!(c1);

    // c1 finally receives the remaining comm1, which advances us to stage 2
    receive_comm1!(c1, 2, sign_states);
    assert_stage2!(c1);

    // Because we have already processed the delayed message, just one more
    // message should be enough to advance us to stage 3
    receive_ver2!(c1, 2, sign_states);

    assert_stage3!(c1);
}

#[tokio::test]
async fn should_delay_sig3() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;

    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase2.clients[0].clone();

    assert_stage2!(c1);

    // "Slow" client c1 receives a sig3 message before stage 3, it should be delayed
    receive_ver2!(c1, 1, sign_states);
    receive_sig3!(c1, 1, &sign_states);
    assert_stage2!(c1);

    // This should advance us to the next stage and trigger processing of the delayed message
    receive_ver2!(c1, 2, sign_states);
    assert_stage3!(c1);

    // Because we have already processed the delayed message, just one more
    // message should be enough to advance us to stage 4
    receive_sig3!(c1, 2, &sign_states);
    assert_stage4!(c1);
}

#[tokio::test]
async fn should_delay_ver4() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;

    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase3.as_ref().unwrap().clients[0].clone();

    assert_stage3!(c1);

    // "Slow" client c1 receives a ver4 message before stage 4, it should be delayed
    receive_sig3!(c1, 1, &sign_states);
    receive_ver4!(c1, 1, sign_states);

    assert_stage3!(c1);

    // This should trigger processing of the delayed message
    receive_sig3!(c1, 2, &sign_states);

    assert_stage4!(c1);

    // Because we have already processed the delayed message, just one more
    // message should be enough to create the signature (stage becomes None)
    receive_ver4!(c1, 2, sign_states);
    assert_no_stage!(c1);

    // TODO: check that we've created a signature!
}

// ********************** Handle invalid local sigs **********************

#[tokio::test]
async fn should_handle_invalid_local_sig() {
    let mut ctx = helpers::KeygenContext::new();
    let _keygen_states = ctx.generate().await;

    // Party at this idx will send an invalid signature
    let bad_idx = 1;

    ctx.use_invalid_local_sig(bad_idx);

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
}

#[tokio::test]
async fn should_handle_inconsistent_broadcast_com1() {
    let mut ctx = helpers::KeygenContext::new();
    let _keygen_states = ctx.generate().await;

    // Party at this idx will send and invalid signature
    let bad_idx = 1;

    ctx.use_inconsistent_broadcast_for_comm1(bad_idx, 0);
    ctx.use_inconsistent_broadcast_for_comm1(bad_idx, 2);

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
}

#[tokio::test]
async fn should_handle_inconsistent_broadcast_sig3() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;

    // Party at this idx will send and invalid signature

    // This is the index in the array
    let bad_idx = 1;

    ctx.use_inconsistent_broadcast_for_sig3(bad_idx, 0);
    ctx.use_inconsistent_broadcast_for_sig3(bad_idx, 2);

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
}

async fn check_blamed_paries(mut rx: &mut helpers::InnerEventReceiver, expected: &[usize]) {
    let (_, blamed_parties) = helpers::check_outcome(&mut rx)
        .await
        .expect("should procude outcome")
        .result
        .clone()
        .unwrap_err();

    assert_eq!(
        blamed_parties,
        expected
            .iter()
            // Needs +1 to map from array idx to signer idx
            .map(|idx| AccountId([*idx as u8 + 1; 32]))
            .collect_vec()
    );
}

#[tokio::test]
async fn should_report_on_timeout_before_request_to_sign() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let sign_states = ctx.sign().await;

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    assert_no_stage!(c1);

    let bad_array_idxs = [1usize, 2];

    for idx in bad_array_idxs.iter() {
        receive_comm1!(c1, *idx, sign_states);
    }

    assert_no_stage!(c1);

    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &bad_array_idxs).await;
}

#[tokio::test]
async fn should_report_on_timeout_stage1() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase1.clients[0].clone();

    // This party sends data as expected
    let good_party_idx = 1;
    receive_comm1!(c1, good_party_idx, sign_states);

    // This party fails to send data in time
    let bad_party_idx = 2;

    assert_stage1!(c1);

    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &[bad_party_idx]).await;
}

#[tokio::test]
async fn should_report_on_timeout_stage2() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase2.clients[0].clone();

    // This party sends data as expected
    let good_party_idx = 1;
    receive_ver2!(c1, good_party_idx, sign_states);

    // This party fails to send data in time
    let bad_party_idx = 2;

    assert_stage2!(c1);

    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &[bad_party_idx]).await;
}

#[tokio::test]
async fn should_report_on_timeout_stage3() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase3.as_ref().unwrap().clients[0].clone();

    // This party sends data as expected
    let good_party_idx = 1;
    receive_sig3!(c1, good_party_idx, sign_states);

    // This party fails to send data in time
    let bad_party_idx = 2;

    assert_stage3!(c1);

    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &[bad_party_idx]).await;
}

#[tokio::test]
async fn should_report_on_timeout_stage4() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase4.as_ref().unwrap().clients[0].clone();

    // This party sends data as expected
    let good_party_idx = 1;
    receive_ver4!(c1, good_party_idx, sign_states);

    // This party fails to send data in time
    let bad_party_idx = 2;

    assert_stage4!(c1);

    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &[bad_party_idx]).await;
}

#[tokio::test]
async fn should_ignore_duplicate_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;

    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase2.clients[0].clone();

    assert_stage2!(c1);

    // Send another request to sign with the same ceremony_id and key_id
    c1.send_request_to_sign_default(ctx.key_id());

    // The request should have been rejected and the existing ceremony is unchanged
    assert_stage2!(c1);
}

#[tokio::test]
async fn should_delay_rts_until_key_is_ready() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.keygen_phase2.clients[0].clone();

    assert_no_stage!(c1);

    // send the request to sign
    c1.send_request_to_sign_default(ctx.key_id());

    // The request should have been delayed, so the stage is unaffected
    assert_no_stage!(c1);

    // complete the keygen by sending the sec2 from each other client to client 0
    for sender_idx in 1..=3 {
        let s_id = keygen_states.keygen_phase2.clients[sender_idx].get_my_account_id();
        let sec2 = keygen_states.keygen_phase2.sec2_vec[sender_idx]
            .get(&c1.get_my_account_id())
            .unwrap();

        let m = helpers::keygen_data_to_p2p(sec2.clone(), &s_id, KEYGEN_CEREMONY_ID);
        c1.process_p2p_message(m);
    }

    // Now that the keygen completed, the rts should have started
    assert_stage1!(c1);
}

#[tokio::test]
async fn should_ignore_signing_non_participant() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase2.clients[0].clone();

    assert_stage2!(c1);

    // send all but 1 ver2 data to the client
    for sender_idx in 1..sign_states.sign_phase2.clients.len() - 2 {
        let s_id = sign_states.sign_phase2.clients[sender_idx].get_my_account_id();
        let ver2 = sign_states.sign_phase2.ver2_vec[sender_idx].clone();

        let m = helpers::sig_data_to_p2p(ver2, &s_id);
        c1.process_p2p_message(m);
    }

    // Make use that the non_participant_id is not a signer
    let non_participant_idx = 3;
    let non_participant_id = VALIDATOR_IDS[non_participant_idx].clone();
    assert!(SIGNER_IDS
        .iter()
        .find(|v_id| *v_id == &non_participant_id)
        .is_none());

    // Send some ver2 data from the non-participant to the client
    let ver2 = sign_states.sign_phase2.ver2_vec[non_participant_idx - 1].clone();
    c1.process_p2p_message(helpers::sig_data_to_p2p(ver2, &non_participant_id));

    // The message should of been ignored and the client stage should not advanced/fail
    assert_stage2!(c1);
}
