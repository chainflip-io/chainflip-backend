use super::*;

macro_rules! assert_stage {
    ($c1:expr, $stage:expr) => {
        assert_eq!(helpers::get_stage_for_msg(&$c1, &MESSAGE_INFO), $stage);
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
    ($c1:expr, $sign_states:expr) => {
        let comm1 = $sign_states.sign_phase1.comm1_vec[1].clone();
        let m = helpers::sig_data_to_p2p(comm1, &VALIDATOR_IDS[1], &MESSAGE_INFO);
        $c1.process_p2p_mq_message(m);
    };
}

macro_rules! receive_ver2 {
    ($c1:expr, $sign_states:expr) => {
        let ver2 = $sign_states.sign_phase2.ver2_vec[1].clone();
        let m = helpers::sig_data_to_p2p(ver2, &VALIDATOR_IDS[1], &MESSAGE_INFO);
        $c1.process_p2p_mq_message(m);
    };
}

macro_rules! receive_sig3 {
    ($c1:expr, $sign_states:expr) => {
        let sig3 = $sign_states.sign_phase3.local_sigs[1].clone();
        let m = helpers::sig_data_to_p2p(sig3, &VALIDATOR_IDS[1], &MESSAGE_INFO);
        $c1.process_p2p_mq_message(m);
    };
}

macro_rules! receive_ver4 {
    ($c1:expr, $sign_states:expr) => {
        let ver4 = $sign_states.sign_phase4.ver4_vec[1].clone();
        let m = helpers::sig_data_to_p2p(ver4, &VALIDATOR_IDS[1], &MESSAGE_INFO);
        $c1.process_p2p_mq_message(m);
    };
}

// Should be in AwaitCommitments1 stage right after a
// request to sign
#[tokio::test]
async fn should_await_comm1_after_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    let key = c1.get_key(KEY_ID).expect("no key").to_owned();

    c1.signing_manager
        .on_request_to_sign(MESSAGE_HASH.clone(), key, SIGN_INFO.clone());

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
    receive_comm1!(c1, sign_states);

    assert_eq!(helpers::get_stage_for_msg(&c1, &MESSAGE_INFO), None);

    // when c1 receives a request to sign, it processes the delayed message,
    // which should advance the client two stages forward
    let key = c1.get_key(KEY_ID).expect("no key").to_owned();
    c1.signing_manager
        .on_request_to_sign(MESSAGE_HASH.clone(), key, SIGN_INFO.clone());

    assert_stage2!(c1);
}

#[tokio::test]
async fn should_delay_ver2() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase1.clients[0].clone();

    assert_stage1!(c1);

    // "Slow" client c1 receives a ver2 message before stage 2, it should be delayed
    receive_ver2!(c1, sign_states);

    assert_stage1!(c1);

    // c1 finally receives the last comm1
    receive_comm1!(c1, sign_states);

    // Now it should be able to process the delayed message ver2 and proceed
    // to the third stage
    assert_stage3!(c1);
}

#[tokio::test]
async fn should_delay_sig3() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase2.clients[0].clone();

    assert_stage2!(c1);

    // This should be delayed
    receive_sig3!(c1, sign_states);

    assert_stage2!(c1);

    // This should trigger processing of the delayed message
    receive_ver2!(c1, sign_states);

    assert_stage4!(c1);
}

#[tokio::test]
async fn should_delay_ver4() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase3.clients[0].clone();

    assert_stage3!(c1);

    // This should be delayed
    receive_ver4!(c1, sign_states);

    assert_stage3!(c1);

    // This should trigger processing of the delayed message
    receive_sig3!(c1, sign_states);

    assert_no_stage!(c1);

    // TODO: check that we've created a signature!
}

// ********************** Handle invalid local sigs **********************

#[tokio::test]
async fn should_handle_invalid_local_sig() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase3.clients[0].clone();

    // Option 1: instrument client to produce invalid sigs
    // Option 2: instrument test generator code to substitute emitted sig

    // let ver4 = sign_states.sign_phase4.ver4_vec[1].clone();
    // let m = helpers::sig_data_to_p2p(ver4, &VALIDATOR_IDS[1], &MESSAGE_INFO);
    // c1.process_p2p_mq_message(m);
}
