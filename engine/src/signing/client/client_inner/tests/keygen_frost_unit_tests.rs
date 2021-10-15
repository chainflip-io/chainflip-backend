use crate::signing::MultisigInstruction;

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

    // Recieve an early stage1 message, should be delayed
    receive_comm1!(c1, 1, keygen_states);

    assert_no_stage(&c1);

    c1.process_multisig_instruction(MultisigInstruction::KeyGen(KEYGEN_INFO.clone()));

    assert_stage1(&c1);

    // Recieve the remaining stage1 messages. Provided that the first
    // message was properly delayed, this should advance us to the next stage
    receive_comm1!(c1, 2, keygen_states);
    receive_comm1!(c1, 3, keygen_states);

    assert_stage2(&c1);
}

// TODO: more tests (see https://github.com/chainflip-io/chainflip-backend/issues/677)
