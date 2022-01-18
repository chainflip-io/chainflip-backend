use crate::multisig::client::{
    tests::helpers::{
        gen_invalid_keygen_comm1, keygen_data_to_p2p, next_with_timeout,
        STAGE_FINISHED_OR_NOT_STARTED,
    },
    CeremonyAbortReason,
};
use crate::multisig::MultisigInstruction;

use super::helpers::{self, check_blamed_paries};

use crate::testing::assert_ok;

use super::*;

use crate::logging::{
    KEYGEN_CEREMONY_FAILED, KEYGEN_REJECTED_INCOMPATIBLE, KEYGEN_REQUEST_IGNORED,
};

/// If all nodes are honest and behave as expected we should
/// generate a key without entering a blaming stage
#[tokio::test]
async fn happy_path_results_in_valid_key() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // No blaming stage
    assert!(keygen_states.blame_responses6.is_none());

    let pubkey = keygen_states.key_ready.unwrap().pubkey;

    // Should always generate the same key (given the same rng seed)
    assert_eq!(
        hex::encode(pubkey.serialize()),
        "02da23ee9b5837d1a834e658a43bb0047794a57b1df3a777abee4939becab9d903"
    );

    // Able to generate a valid signature
    let signature = assert_ok!(ctx.sign().await.sign_finished.outcome.result);

    // Should always generate the same signature (given the same rng seed)
    assert_eq!(
        hex::encode(signature.s),
        "5437008e73079744cbac3bb231ccabd87b654a63ea38f219056ce115a78cb4ee"
    );
    assert_eq!(
        hex::encode(signature.r.serialize()),
        "0394f12c8706482c54363295ef88fe5910bfc73a0423b0deb048f4181d6defdf53"
    );
}

/// If keygen state expires before a formal request to keygen
/// (from our SC), we should report initiators of that ceremony
#[tokio::test]
#[ignore = "functionality disabled as SC does not expect this response"]
async fn should_report_on_timeout_before_keygen_request() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c0 = keygen_states.get_client_at_stage(&ctx.get_account_id(0), 0);

    let bad_party_id = ctx.get_account_id(1);

    c0.receive_keygen_stage_data(1, &keygen_states, &bad_party_id);

    // Force all ceremonies to time out
    c0.force_stage_timeout();

    check_blamed_paries(
        ctx.outcome_receivers
            .get_mut(&ctx.get_account_id(0))
            .unwrap(),
        &[bad_party_id],
    )
    .await;
}

#[tokio::test]
async fn should_delay_comm1_before_keygen_request() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c0 = keygen_states.get_client_at_stage(&ctx.get_account_id(0), 0);

    // Receive an early stage1 message, should be delayed
    c0.receive_keygen_stage_data(1, &keygen_states, &ctx.get_account_id(1));

    assert_ok!(c0.ensure_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED));

    c0.process_multisig_instruction(
        MultisigInstruction::Keygen(KEYGEN_INFO.clone()),
        &mut ctx.rng,
    );

    assert_ok!(c0.ensure_at_keygen_stage(1));

    // Receive the remaining stage1 messages. Provided that the first
    // message was properly delayed, this should advance us to the next stage
    c0.receive_keygen_stage_data(1, &keygen_states, &ctx.get_account_id(2));
    c0.receive_keygen_stage_data(1, &keygen_states, &ctx.get_account_id(3));

    assert_ok!(c0.ensure_at_keygen_stage(2));
}

// Data for any stage that arrives one stage too early should be properly delayed
// and processed after the stage transition is made
#[tokio::test]
async fn should_delay_stage_data() {
    let mut ctx = helpers::KeygenContext::new();

    // Use invalid secret share so the ceremony will go all the way to the blaming stages
    ctx.use_invalid_secret_share(&ctx.get_account_id(2), &ctx.get_account_id(0));
    let keygen_states = ctx.generate().await;

    // Test the delay functionality for all stages except the last stage
    for stage in 1..KEYGEN_STAGES {
        // Get a client at the correct stage
        let mut c0 = keygen_states.get_client_at_stage(&ctx.get_account_id(0), stage);

        // Receive the data of this stage and the next from all but 1 client
        c0.receive_keygen_stage_data(stage, &keygen_states, &ctx.get_account_id(1));
        c0.receive_keygen_stage_data(stage, &keygen_states, &ctx.get_account_id(2));
        c0.receive_keygen_stage_data(stage + 1, &keygen_states, &ctx.get_account_id(1));
        c0.receive_keygen_stage_data(stage + 1, &keygen_states, &ctx.get_account_id(2));
        assert_ok!(c0.ensure_at_keygen_stage(stage));

        // Now receive the final clients data to advance the stage
        c0.receive_keygen_stage_data(stage, &keygen_states, &ctx.get_account_id(3));
        assert_ok!(c0.ensure_at_keygen_stage(stage + 1));

        // If the messages were delayed properly, then receiving
        // the last clients data will advance the stage again
        c0.receive_keygen_stage_data(stage + 1, &keygen_states, &ctx.get_account_id(3));

        // Check that the stage correctly advanced or finished
        if stage + 2 > KEYGEN_STAGES {
            // The keygen finished
            assert_ok!(c0.ensure_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED));
        } else {
            assert_ok!(c0.ensure_at_keygen_stage(stage + 2));
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
    ctx.use_invalid_secret_share(&ctx.get_account_id(1), &ctx.get_account_id(2));

    let keygen_states = ctx.generate().await;

    // Check that nodes had to go through a blaming stage
    assert!(keygen_states.blame_responses6.is_some());

    // Check that we are still able to sign
    assert!(ctx.sign().await.sign_finished.outcome.result.is_ok());
}

/// If one or more parties send an invalid secret share both the first
/// time and during the blaming stage, the ceremony is aborted with these
/// parties reported
#[tokio::test]
async fn should_report_on_invalid_blame_response() {
    let mut ctx = helpers::KeygenContext::new();
    ctx.auto_clear_tag_cache = false;

    let bad_node_id = ctx.get_account_id(1);

    // Node (bad_node_id) sends an invalid secret share to (2) and
    // also sends an invalid blame response later on
    ctx.use_invalid_secret_share(&bad_node_id, &ctx.get_account_id(2));
    ctx.use_invalid_blame_response(&bad_node_id, &ctx.get_account_id(2));

    // Node (2) sends an invalid secret share to (3),
    // but later sends a valid blame response (sent by default)
    ctx.use_invalid_secret_share(&ctx.get_account_id(2), &ctx.get_account_id(3));

    // Run the keygen ceremony and check that the failure details match
    let keygen_states = ctx
        .run_keygen_and_check_failure(
            CeremonyAbortReason::Invalid,
            vec![bad_node_id],
            KEYGEN_CEREMONY_FAILED,
        )
        .await
        .unwrap();

    // Check that nodes had to go through a blaming stage
    assert!(keygen_states.blame_responses6.is_some());
}

#[tokio::test]
async fn should_abort_on_blames_at_invalid_indexes() {
    let mut ctx = helpers::KeygenContext::new();
    ctx.auto_clear_tag_cache = false;

    let bad_node_id = ctx.get_account_id(1);

    ctx.use_invalid_complaint(&bad_node_id);

    // Run the keygen ceremony and check that the failure details match
    assert_ok!(
        ctx.run_keygen_and_check_failure(
            CeremonyAbortReason::Invalid,
            vec![bad_node_id],
            KEYGEN_CEREMONY_FAILED,
        )
        .await
    );
}

#[tokio::test]
async fn should_ignore_keygen_request_if_not_participating() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c0 = keygen_states.get_client_at_stage(&ctx.get_account_id(0), 0);

    // Get an id that is not `c0`s id
    let unknown_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&unknown_id));
    let mut keygen_ids = ACCOUNT_IDS.clone();
    keygen_ids[0] = unknown_id;

    // Send the keygen request
    let keygen_info = KeygenInfo::new(KEYGEN_INFO.ceremony_id, keygen_ids);
    c0.process_multisig_instruction(MultisigInstruction::Keygen(keygen_info), &mut ctx.rng);

    // The request should have been ignored and the not started a ceremony
    assert_ok!(c0.ensure_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert!(ctx.tag_cache.contains_tag(KEYGEN_REQUEST_IGNORED));
}

#[tokio::test]
async fn should_ignore_duplicate_keygen_request() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // Get a client that is already in the middle of a keygen
    let mut c0 = keygen_states.get_client_at_stage(&ctx.get_account_id(0), 2);

    // Create a list of accounts that is different from the default Keygen
    let unknown_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&unknown_id));
    let mut keygen_ids = ACCOUNT_IDS.clone();
    keygen_ids[1] = unknown_id;

    // Send another keygen request with the same ceremony_id but different signers
    let keygen_info = KeygenInfo::new(KEYGEN_INFO.ceremony_id, keygen_ids);
    c0.process_multisig_instruction(MultisigInstruction::Keygen(keygen_info), &mut ctx.rng);

    // The request should have been rejected and the existing ceremony is unchanged
    assert_ok!(c0.ensure_at_keygen_stage(2));
    assert!(ctx.tag_cache.contains_tag(KEYGEN_REQUEST_IGNORED));
}

// Ignore unexpected messages at all stages. This includes:
// - Messages with stage data that is not the current stage or the next stage
// - Duplicate messages from the same sender AccountId
// - Messages from unknown AccountId (not in the keygen ceremony)
#[tokio::test]
async fn should_ignore_unexpected_message_for_stage() {
    let mut ctx = helpers::KeygenContext::new();

    // Use invalid secret share so the ceremony will go all the way to the blaming stages
    ctx.use_invalid_secret_share(&ctx.get_account_id(2), &ctx.get_account_id(0));
    let keygen_states = ctx.generate().await;

    // Get an id that is not in the keygen ceremony
    let unknown_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&unknown_id));

    // Test for all keygen stages
    for current_stage in 1..=KEYGEN_STAGES {
        // Get a client at the correct stage
        let mut c0 = keygen_states.get_client_at_stage(&ctx.get_account_id(0), current_stage);

        // Get the correct data from 2 clients so that we only need one more to advance
        c0.receive_keygen_stage_data(current_stage, &keygen_states, &ctx.get_account_id(1));
        c0.receive_keygen_stage_data(current_stage, &keygen_states, &ctx.get_account_id(2));

        // Receive messages from all unexpected stages (not the current stage or the next)
        for stage in 1..=KEYGEN_STAGES {
            if stage != current_stage && stage != current_stage + 1 {
                c0.receive_keygen_stage_data(stage, &keygen_states, &ctx.get_account_id(3));
            }
        }
        assert!(
            c0.ensure_at_keygen_stage(current_stage).is_ok(),
            "Failed to ignore a message from an unexpected stage"
        );

        // Receive a duplicate message
        c0.receive_keygen_stage_data(current_stage, &keygen_states, &ctx.get_account_id(1));
        c0.receive_keygen_stage_data(current_stage, &keygen_states, &ctx.get_account_id(2));
        assert!(
            c0.ensure_at_keygen_stage(current_stage).is_ok(),
            "Failed to ignore a message from a duplicate sender id"
        );

        // Receive a message from an unknown AccountId
        let message = c0.get_keygen_p2p_message_for_stage(
            current_stage,
            &keygen_states,
            &ctx.get_account_id(1),
        );
        c0.process_p2p_message(unknown_id.clone(), message);
        assert!(
            c0.ensure_at_keygen_stage(current_stage).is_ok(),
            "Failed to ignore a message from an non=participant"
        );

        // Receive the last message and advance the stage
        c0.receive_keygen_stage_data(current_stage, &keygen_states, &ctx.get_account_id(3));
        if current_stage + 1 > KEYGEN_STAGES {
            // The keygen finished
            assert_ok!(c0.ensure_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED));
        } else {
            assert_ok!(c0.ensure_at_keygen_stage(current_stage + 1));
        }
    }
}

// If one of more parties (are thought to) broadcast data inconsistently,
// the ceremony should be aborted and all faulty parties should be reported.
// Fail on `verify_broadcasts` during `VerifyCommitmentsBroadcast2`
#[tokio::test]
async fn should_handle_inconsistent_broadcast_comm1() {
    let mut ctx = helpers::KeygenContext::new();
    ctx.auto_clear_tag_cache = false;

    // Make one of the nodes send different comm1 to most of the others
    // Note: the bad node must send different comm1 to more than 1/3 of the participants
    let bad_node_id = ctx.get_account_id(1);
    ctx.use_inconsistent_broadcast_for_keygen_comm1(&bad_node_id, &ctx.get_account_id(0));
    ctx.use_inconsistent_broadcast_for_keygen_comm1(&bad_node_id, &ctx.get_account_id(2));

    // Run the keygen ceremony and check that the failure details match
    assert_ok!(
        ctx.run_keygen_and_check_failure(
            CeremonyAbortReason::Invalid,
            vec![bad_node_id],
            KEYGEN_CEREMONY_FAILED,
        )
        .await
    );
}

// If one or more parties send invalid commitments, the ceremony should be aborted.
// Fail on `validate_commitments` during `VerifyCommitmentsBroadcast2`.
#[tokio::test]
async fn should_handle_invalid_commitments() {
    let mut ctx = helpers::KeygenContext::new();
    ctx.auto_clear_tag_cache = false;

    // Make a node send a bad commitment to the others
    // Note: we must send the same bad commitment to all of the nodes,
    // or we will fail on the `inconsistent` error instead of the validation error.
    let bad_node_ids = vec![ctx.get_account_id(1), ctx.get_account_id(2)];
    for id in &bad_node_ids {
        ctx.use_invalid_keygen_comm1(id.clone());
    }

    // Run the keygen ceremony and check that the failure details match
    assert_ok!(
        ctx.run_keygen_and_check_failure(
            CeremonyAbortReason::Invalid,
            bad_node_ids,
            KEYGEN_CEREMONY_FAILED,
        )
        .await
    );
}

// Keygen aborts if the key is not compatible with the contract at VerifyCommitmentsBroadcast2
// TODO: Once we are able to seed the keygen (deterministic crypto), this test can be replaced
// with a proper test that has a known incompatible aggkey.
#[tokio::test]
async fn should_handle_not_compatible_keygen() {
    let mut counter = 0;
    loop {
        // Disallow the high pubkey and run the keygen as in production
        let mut ctx = helpers::KeygenContext::builder()
            .allowing_high_pubkey(false)
            .build();
        ctx.auto_clear_tag_cache = false;
        let keygen_states = ctx.generate().await;

        // Wait for it to fail
        if keygen_states.key_ready.is_err() {
            let (reason, reported) = keygen_states.key_ready.unwrap_err();

            assert_eq!(reason, CeremonyAbortReason::Invalid);
            assert!(ctx.tag_cache.contains_tag(KEYGEN_CEREMONY_FAILED));
            assert!(ctx.tag_cache.contains_tag(KEYGEN_REJECTED_INCOMPATIBLE));
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

// If the list of signers in the keygen request contains a duplicate id, the request should be ignored
#[tokio::test]
async fn should_ignore_keygen_request_with_duplicate_signer() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // Get a client that hasn't gotten a keygen request yet
    let mut c0 = keygen_states.get_client_at_stage(&ctx.get_account_id(0), 0);

    // Create a duplicate in the list of signers
    let mut keygen_ids = ACCOUNT_IDS.clone();
    keygen_ids[1] = keygen_ids[2].clone();

    // Send the keygen request with the modified signers list
    let keygen_info = KeygenInfo::new(KEYGEN_INFO.ceremony_id, keygen_ids);
    c0.process_multisig_instruction(MultisigInstruction::Keygen(keygen_info), &mut ctx.rng);

    // Check that the keygen request was ignored
    assert_ok!(c0.ensure_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert!(ctx.tag_cache.contains_tag(KEYGEN_REQUEST_IGNORED));
}

#[tokio::test]
async fn should_ignore_keygen_request_with_used_ceremony_id() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c0 = keygen_states
        .key_ready_data()
        .expect("successful keygen")
        .clients[&ctx.get_account_id(0)]
        .clone();

    // Send another keygen request with the same ceremony_id
    c0.process_multisig_instruction(
        MultisigInstruction::Keygen(KEYGEN_INFO.clone()),
        &mut ctx.rng,
    );

    // Check that the keygen request was ignored
    assert_ok!(c0.ensure_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert!(ctx.tag_cache.contains_tag(KEYGEN_REQUEST_IGNORED));
}

#[tokio::test]
async fn should_ignore_stage_data_with_used_ceremony_id() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // Get a client that has already completed keygen
    let mut c0 = keygen_states
        .key_ready_data()
        .expect("successful keygen")
        .clients[&ctx.get_account_id(0)]
        .clone();
    assert_eq!(c0.ceremony_manager.get_keygen_states_len(), 0);

    // Receive a comm1 with a used ceremony id (same default keygen ceremony id)
    c0.receive_keygen_stage_data(1, &keygen_states, &ctx.get_account_id(1));

    // The message should have been ignored and no ceremony was started
    // In this case, the ceremony would be unauthorised, so we must check how many keygen states exist
    // to see if a unauthorised state was created.
    assert_eq!(c0.ceremony_manager.get_keygen_states_len(), 0);
}

#[tokio::test]
async fn should_not_consume_ceremony_id_if_unauthorised() {
    let mut ctx = helpers::KeygenContext::new();

    // Get a client that has not used the default keygen ceremony id yet
    let id0 = ctx.get_account_id(0);
    let mut c0 = ctx.clients[&id0].clone();
    assert_eq!(c0.ceremony_manager.get_keygen_states_len(), 0);

    // Receive comm1 with the default keygen ceremony id
    let message = keygen_data_to_p2p(gen_invalid_keygen_comm1(&mut ctx.rng));
    assert_eq!(message.ceremony_id, KEYGEN_CEREMONY_ID);
    c0.process_p2p_message(ACCOUNT_IDS[1].clone(), message);

    // Check that the unauthorised ceremony was created
    assert_eq!(c0.ceremony_manager.get_keygen_states_len(), 1);

    // Timeout the unauthorised ceremony
    c0.force_stage_timeout();

    // Clear out the timeout outcome
    next_with_timeout(ctx.outcome_receivers.get_mut(&id0).unwrap()).await;

    // keygen as normal using the default ceremony id
    let keygen_states = ctx.generate().await;

    // Should not of been rejected because of a used ceremony id
    assert!(keygen_states.key_ready.is_ok());
}

mod timeout {

    use super::*;

    // What should be tested w.r.t timeouts:

    // 1. [todo] If timeout during a broadcast verification stage, and we have enough data, we can recover
    // TODO: more test cases

    mod during_broadcast_verification_stage {

        use super::*;

        async fn recover_if_agree_on_values(stage_idx: usize) {
            let mut ctx = helpers::KeygenContext::new();
            let bad_party_id = ctx.get_account_id(1);

            ctx.force_party_timeout_keygen(&bad_party_id, None, stage_idx);

            let _ = ctx.generate().await;
        }

        #[tokio::test]
        async fn recover_if_agree_on_values_stage2() {
            recover_if_agree_on_values(2).await;
        }
    }
}
