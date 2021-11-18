use crate::multisig::client::CeremonyAbortReason;
use crate::multisig::MultisigInstruction;

use super::helpers::check_blamed_paries;
use super::*;

macro_rules! receive_comm1 {
    ($client:expr, $sender: expr, $keygen_states:expr) => {
        let comm1 = $keygen_states.comm_stage1.comm1_vec[$sender].clone();
        let m = helpers::keygen_data_to_p2p(comm1, &VALIDATOR_IDS[$sender], KEYGEN_CEREMONY_ID);
        $client.process_p2p_message(m);
    };
}

fn assert_no_stage(c: &helpers::MultisigClientNoDB) {
    assert_eq!(helpers::get_stage_for_keygen_ceremony(&c), None);
}

fn assert_stage1(c: &helpers::MultisigClientNoDB) {
    assert_eq!(
        helpers::get_stage_for_keygen_ceremony(&c).as_deref(),
        Some("BroadcastStage<AwaitCommitments1>")
    );
}

fn assert_stage2(c: &helpers::MultisigClientNoDB) {
    assert_eq!(
        helpers::get_stage_for_keygen_ceremony(&c).as_deref(),
        Some("BroadcastStage<VerifyCommitmentsBroadcast2>")
    );
}

/// If all nodes are honest and behave as expected we should
/// generate a key without entering a blaming stage
#[tokio::test]
async fn happy_path_results_in_valid_key() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // No blaming stage
    assert!(keygen_states.blame_responses6.is_none());

    // Able to generate a valid signature
    assert!(ctx.sign().await.outcome.result.is_ok());
}

/// If keygen state expires before a formal request to keygen
/// (from our SC), we should report initiators of that ceremony
#[tokio::test]
async fn report_initiators_of_unexpected_keygen() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.stage0.clients[0].clone();

    let bad_party_idx = 1;

    receive_comm1!(c1, bad_party_idx, keygen_states);

    // Force all ceremonies to time out
    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &[bad_party_idx]).await;
}

/// If a ceremony expires in the middle of the first stage,
/// we should report the slow parties
#[tokio::test]
async fn should_report_on_timeout_stage1() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.comm_stage1.clients[0].clone();

    let bad_party_idxs = [1, 2];
    let good_party_idx = 3;

    receive_comm1!(c1, good_party_idx, keygen_states);

    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &bad_party_idxs).await;
}

#[tokio::test]
async fn should_delay_comm1_before_keygen_request() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.stage0.clients[0].clone();

    // Receive an early stage1 message, should be delayed
    receive_comm1!(c1, 1, keygen_states);

    assert_no_stage(&c1);

    c1.process_multisig_instruction(MultisigInstruction::Keygen(KEYGEN_INFO.clone()));

    assert_stage1(&c1);

    // Receive the remaining stage1 messages. Provided that the first
    // message was properly delayed, this should advance us to the next stage
    receive_comm1!(c1, 2, keygen_states);
    receive_comm1!(c1, 3, keygen_states);

    assert_stage2(&c1);
}

/// If at least one party is blamed during the "Complaints" stage, we
/// should enter a blaming stage, where the blamed party sends a valid
/// share, so the ceremony should be successful in the end
#[tokio::test]
async fn should_enter_blaming_stage_on_invalid_secret_shares() {
    let mut ctx = helpers::KeygenContext::new();

    // Instruct (1) to send an invalid secret share to (2)
    ctx.use_invalid_secret_share(1, 2);

    let keygen_states = ctx.generate().await;

    // Check that nodes had to go through a blaming stage
    assert!(keygen_states.blame_responses6.is_some());

    // Check that we are still able to sign
    assert!(ctx.sign().await.outcome.result.is_ok());
}

/// If one or more parties send an invalid secret share both the first
/// time and during the blaming stage, the ceremony is aborted with these
/// parties reported
#[tokio::test]
async fn should_report_on_invalid_blame_response() {
    let mut ctx = helpers::KeygenContext::new();

    let bad_node_idx = 1;

    // Node (bad_node_idx) sends an invalid secret share to (2) and
    // also sends an invalid blame response later on
    ctx.use_invalid_secret_share(bad_node_idx, 2);
    ctx.use_invalid_blame_response(bad_node_idx, 2);

    // Node (bad_node_idx + 1) sends an invalid secret share to (3),
    // but later sends a valid blame response (sent by default)
    ctx.use_invalid_secret_share(bad_node_idx + 1, 3);

    let keygen_states = ctx.generate().await;

    // Check that nodes had to go through a blaming stage
    assert!(keygen_states.blame_responses6.is_some());

    assert!(keygen_states.key_ready.is_err());

    let (reason, reported) = keygen_states.key_ready.unwrap_err();

    assert_eq!(reason, CeremonyAbortReason::Invalid);

    // Only (bad_node_idx) should be reported
    assert_eq!(
        reported.as_slice(),
        &[AccountId([bad_node_idx as u8 + 1; 32])]
    );
}

#[tokio::test]
async fn should_abort_on_blames_at_invalid_indexes() {
    let mut ctx = helpers::KeygenContext::new();

    let bad_node_idx = 1;

    ctx.use_invalid_complaint(bad_node_idx);

    let keygen_states = ctx.generate().await;

    let (reason, reported) = keygen_states.key_ready.unwrap_err();

    assert_eq!(reason, CeremonyAbortReason::Invalid);
    assert_eq!(
        reported.as_slice(),
        &[AccountId([bad_node_idx as u8 + 1; 32])]
    );
}

// TODO: test that blame responses sent by nodes not blamed
// earlier are ignored

// TODO: more tests (see https://github.com/chainflip-io/chainflip-backend/issues/677)
