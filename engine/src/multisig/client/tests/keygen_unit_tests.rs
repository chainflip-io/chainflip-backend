use crate::multisig::client::tests::helpers::get_stage_for_keygen_ceremony;
use crate::multisig::client::CeremonyAbortReason;
use crate::multisig::MultisigInstruction;

use super::helpers::{self, check_blamed_paries};

use super::*;

use crate::logging::{
    CEREMONY_IGNORED, KEYGEN_CEREMONY_FAILED, KEYGEN_REQUEST_EXPIRED, KEYGEN_REQUEST_IGNORED,
};

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
async fn should_report_on_timeout_before_keygen_request() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    ctx.tag_cache.clear();

    let mut c1 = keygen_states.get_client_at_stage(0);

    let bad_party_idx = 1;

    c1.receive_keygen_stage_data(1, &keygen_states, bad_party_idx);

    // Force all ceremonies to time out
    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &[bad_party_idx]).await;
    assert!(ctx.tag_cache.contains_tag(KEYGEN_REQUEST_EXPIRED));
}

/// If a ceremony expires in the middle of the any stage,
/// we should report the slow parties
#[tokio::test]
async fn should_report_on_timeout_stage() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let bad_party_idxs = [1, 2];
    let good_party_idx = 3;

    // Test the timeout for all stages 1 to 5
    for stage in 1..=5 {
        // Get a client at the correct stage
        let mut c1 = keygen_states.get_client_at_stage(stage);

        // Receive data from one client but not the others
        c1.receive_keygen_stage_data(stage, &keygen_states, good_party_idx);

        // Trigger timeout
        c1.expire_all();
        c1.cleanup();

        // Check that the late 2 clients are correctly reported
        check_blamed_paries(&mut ctx.rxs[0], &bad_party_idxs).await;
        assert!(ctx.tag_cache.contains_tag(KEYGEN_REQUEST_EXPIRED));
    }
}

#[tokio::test]
async fn should_delay_comm1_before_keygen_request() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.get_client_at_stage(0);

    // Receive an early stage1 message, should be delayed
    c1.receive_keygen_stage_data(1, &keygen_states, 1);

    assert!(c1.is_at_keygen_stage(0));

    c1.process_multisig_instruction(MultisigInstruction::KeyGen(KEYGEN_INFO.clone()));

    assert!(c1.is_at_keygen_stage(1));

    // Receive the remaining stage1 messages. Provided that the first
    // message was properly delayed, this should advance us to the next stage
    c1.receive_keygen_stage_data(1, &keygen_states, 2);
    c1.receive_keygen_stage_data(1, &keygen_states, 3);

    assert!(c1.is_at_keygen_stage(2));
}

// Data for any stage that arrives one stage too early should be properly delayed
// and processed after the stage transition is made
#[tokio::test]
async fn should_delay_stage_data() {
    let mut ctx = helpers::KeygenContext::new();
    ctx.use_invalid_secret_share(2, 0);
    let keygen_states = ctx.generate().await;

    // Test the delay functionality for all stages except the last stage
    for stage in 1..=6 {
        // Get a client at the correct stage
        let mut c1 = keygen_states.get_client_at_stage(stage);

        // Receive the data of this stage and the next from all but 1 client
        c1.receive_keygen_stage_data(stage, &keygen_states, 1);
        c1.receive_keygen_stage_data(stage, &keygen_states, 2);
        c1.receive_keygen_stage_data(stage + 1, &keygen_states, 1);
        c1.receive_keygen_stage_data(stage + 1, &keygen_states, 2);
        assert!(c1.is_at_keygen_stage(stage));

        // Now receive the final clients data to advance the stage
        c1.receive_keygen_stage_data(stage, &keygen_states, 3);
        assert!(c1.is_at_keygen_stage(stage + 1));

        // If the messages were delayed properly, then receiving
        // the last clients data will advance the stage again
        c1.receive_keygen_stage_data(stage + 1, &keygen_states, 3);

        // Check that the stage correctly advanced or finished
        if stage + 2 > 7 {
            // The keygen finished
            assert!(c1.is_at_keygen_stage(0));
        } else {
            assert!(c1.is_at_keygen_stage(stage + 2));
        }
    }
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
    assert!(ctx.tag_cache.contains_tag(KEYGEN_CEREMONY_FAILED));

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
    assert!(ctx.tag_cache.contains_tag(KEYGEN_CEREMONY_FAILED));
    assert_eq!(
        reported.as_slice(),
        &[AccountId([bad_node_idx as u8 + 1; 32])]
    );
}

#[tokio::test]
async fn should_ignore_keygen_request_if_not_participating() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    ctx.tag_cache.clear();

    let mut c1 = keygen_states.get_client_at_stage(0);

    // Get an id that is not `c1`s id
    let unknown_id = AccountId([0; 32]);
    assert_ne!(&unknown_id, &c1.get_my_account_id());
    let mut keygen_ids = VALIDATOR_IDS.clone();
    keygen_ids[0] = unknown_id;

    // Send the keygen request
    let keygen_info = KeygenInfo::new(KEYGEN_INFO.ceremony_id, keygen_ids);
    c1.process_multisig_instruction(MultisigInstruction::KeyGen(keygen_info));

    // The request should have been ignored and the not started a ceremony
    assert!(c1.is_at_keygen_stage(0));
    assert!(ctx.tag_cache.contains_tag(KEYGEN_REQUEST_IGNORED));
}

#[tokio::test]
async fn should_ignore_duplicate_keygen_request() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    ctx.tag_cache.clear();

    // Get a client that is already in the middle of a keygen
    let mut c1 = keygen_states.get_client_at_stage(2);

    // Send another keygen request with the same ceremony_id and key_id
    c1.process_multisig_instruction(MultisigInstruction::KeyGen(KEYGEN_INFO.clone()));

    // The request should have been rejected and the existing ceremony is unchanged
    assert!(c1.is_at_keygen_stage(2));
    assert!(ctx.tag_cache.contains_tag(CEREMONY_IGNORED));
}

// Ignore unexpected messages at all stages. This includes:
// - Messages with stage data that is not the current stage or the next stage
// - Duplicate messages from the same sender AccountId
// - Messages from unknown AccountId (not in the keygen ceremony)
#[tokio::test]
async fn should_ignore_unexpected_message_for_stage() {
    let mut ctx = helpers::KeygenContext::new();
    ctx.use_invalid_secret_share(2, 0);
    let keygen_states = ctx.generate().await;

    // Create another keygen that has the blaming stages
    // let mut ctx_branch = helpers::KeygenContext::new();
    // let keygen_states_branch = ctx.generate().await;

    // Get an id that is not in the keygen ceremony
    let unknown_id = AccountId([0; 32]);
    assert!(!VALIDATOR_IDS.contains(&unknown_id));

    // Test for all keygen stages
    for stage in 1..=7 {
        // Get a client at the correct stage
        let mut c1 = keygen_states.get_client_at_stage(stage);

        // Get the correct data from 2 clients so that we only need one more to advance
        c1.receive_keygen_stage_data(stage, &keygen_states, 1);
        c1.receive_keygen_stage_data(stage, &keygen_states, 2);

        // Receive messages from all unexpected stages (not the current stage or the next)
        for i in 1..=7 {
            if i != stage && i != stage + 1 {
                c1.receive_keygen_stage_data(i, &keygen_states, 3);
            }
        }

        // Receive a duplicate message
        c1.receive_keygen_stage_data(stage, &keygen_states, 1);

        // Receive a message from an unknown AccountId
        let message = c1.get_keygen_p2p_message_for_stage(stage, &keygen_states, 1, &unknown_id);
        c1.process_p2p_message(message);

        // Check that the stage did not advance
        assert!(c1.is_at_keygen_stage(stage));

        // Receive the last message and advance the stage
        c1.receive_keygen_stage_data(stage, &keygen_states, 3);
        if stage + 1 > 7 {
            // The keygen finished
            assert!(c1.is_at_keygen_stage(0));
        } else {
            assert!(
                c1.is_at_keygen_stage(stage + 1),
                "Incorrect stage {:?}, should be at stage {}",
                get_stage_for_keygen_ceremony(&c1),
                stage + 1
            );
        }
    }
}

// If one of more parties (are thought to) broadcast data inconsistently,
// the ceremony should be aborted and all faulty parties should be reported.
// Fail on `verify_broadcasts` during `VerifyCommitmentsBroadcast2`
#[tokio::test]
async fn should_handle_inconsistent_broadcast_comm1() {
    let mut ctx = helpers::KeygenContext::new();

    // Make one of the nodes send different comm1 to most of the others
    // Note: the bad node must send different comm1 to more than 1/3 of the participants
    let bad_node_idx = 1;
    ctx.use_inconsistent_broadcast_for_keygen_comm1(bad_node_idx, 0);
    ctx.use_inconsistent_broadcast_for_keygen_comm1(bad_node_idx, 2);

    // Now continue the keygen as normal
    let keygen_states = ctx.generate().await;

    // The keygen should have failed
    assert!(keygen_states.key_ready.is_err());

    let (reason, reported) = keygen_states.key_ready.unwrap_err();

    // Check that it failed for the correct reason and that the correct node was reported
    assert_eq!(reason, CeremonyAbortReason::Invalid);
    assert!(ctx.tag_cache.contains_tag(KEYGEN_CEREMONY_FAILED));
    assert_eq!(
        reported.as_slice(),
        &[AccountId([bad_node_idx as u8 + 1; 32])]
    );
}

// If one or more parties send invalid commitments, the ceremony should be aborted.
// Fail on `validate_commitments` during `VerifyCommitmentsBroadcast2`.
#[tokio::test]
async fn should_handle_invalid_commitments() {
    let mut ctx = helpers::KeygenContext::new();

    // Make a node send a bad commitment to the others
    // Note: we must send the same bad commitment to all of the nodes,
    // or we will fail on the `inconsistent` error instead of the validation error.
    let bad_node_idx = 1;
    ctx.use_invalid_keygen_comm1(bad_node_idx, vec![0, 1, 2, 3]);

    // Now continue the keygen as normal
    let keygen_states = ctx.generate().await;

    // The keygen should have failed
    assert!(keygen_states.key_ready.is_err());

    let (reason, reported) = keygen_states.key_ready.unwrap_err();

    // Check that it failed for the correct reason and that the correct node was reported
    assert_eq!(reason, CeremonyAbortReason::Invalid);
    assert!(ctx.tag_cache.contains_tag(KEYGEN_CEREMONY_FAILED));
    assert_eq!(
        reported.as_slice(),
        &[AccountId([bad_node_idx as u8 + 1; 32])]
    );
}

// Keygen aborts if the key is not compatible with the contract at VerifyCommitmentsBroadcast2
#[tokio::test]
async fn should_handle_not_compatible_keygen() {
    let mut counter = 0;
    loop {
        // Disallow the high pubkey and run the keygen as normal
        let mut ctx = helpers::KeygenContext::new_disallow_high_pubkey();
        let keygen_states = ctx.generate().await;

        // Wait for it to fail
        if keygen_states.key_ready.is_err() {
            let (reason, reported) = keygen_states.key_ready.unwrap_err();

            assert_eq!(reason, CeremonyAbortReason::Invalid);
            assert!(ctx.tag_cache.contains_tag(KEYGEN_CEREMONY_FAILED));
            assert_eq!(reported, vec![], "No parties should be blamed");
            println!("Test Pass, keygen failed after loop {}", counter);
            break;
        }

        // We have a 50/50 chance of failing each time, so we should have failed keygen within 40 tries
        // But it has a 0.0000000001% chance of failing this test as a false positive.
        counter += 1;
        assert!(
            counter < 40,
            "Should have failed keygen with high pub key by now"
        )
    }
}
