use crate::multisig::client::{self, tests::helpers::check_blamed_paries};

use client::tests::*;

use super::helpers;

use crate::logging::{REQUEST_TO_SIGN_EXPIRED, REQUEST_TO_SIGN_IGNORED, SIGNING_CEREMONY_FAILED};

// Data for any stage that arrives one stage too early should be properly delayed
// and processed after the stage transition is made
#[tokio::test]
async fn should_delay_stage_data() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    // Test the delay functionality for all stages except the last stage
    for stage in 1..*SIGNING_STAGES {
        // Get a client at the correct stage
        let mut c1 = sign_states.get_client_at_stage(stage);

        // Receive the data of this stage and the next from all but 1 client
        c1.receive_signing_stage_data(stage, &sign_states, 1);
        c1.receive_signing_stage_data(stage + 1, &sign_states, 1);
        c1.is_at_signing_stage(stage).unwrap();

        // Now receive the final clients data to advance the stage
        c1.receive_signing_stage_data(stage, &sign_states, 2);
        c1.is_at_signing_stage(stage + 1).unwrap();

        // If the messages were delayed properly, then receiving
        // the last clients data will advance the stage again
        c1.receive_signing_stage_data(stage + 1, &sign_states, 2);

        // Check that the stage correctly advanced or finished
        if stage + 2 > *SIGNING_STAGES {
            // The keygen finished
            c1.is_at_signing_stage(0).unwrap();
        } else {
            c1.is_at_signing_stage(stage + 2).unwrap();
        }
    }
}

#[tokio::test]
async fn should_handle_invalid_local_sig() {
    let mut ctx = helpers::KeygenContext::new();
    let _keygen_states = ctx.generate().await;
    ctx.auto_clear_tag_cache = false;

    // Party at this idx will send an invalid signature
    let bad_idx = 1;

    ctx.use_invalid_local_sig(bad_idx);

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
    assert!(ctx.tag_cache.contains_tag(SIGNING_CEREMONY_FAILED));
}

#[tokio::test]
async fn should_handle_inconsistent_broadcast_com1() {
    let mut ctx = helpers::KeygenContext::new();
    let _keygen_states = ctx.generate().await;
    ctx.auto_clear_tag_cache = false;

    // Party at this idx will send and invalid signature
    let bad_idx = 1;

    ctx.use_inconsistent_broadcast_for_signing_comm1(bad_idx, 0);
    ctx.use_inconsistent_broadcast_for_signing_comm1(bad_idx, 2);

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
    assert!(ctx.tag_cache.contains_tag(SIGNING_CEREMONY_FAILED));
}

#[tokio::test]
async fn should_handle_inconsistent_broadcast_sig3() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    ctx.auto_clear_tag_cache = false;

    // Party at this idx will send and invalid signature
    // This is the index in the array
    let bad_idx = 1;

    ctx.use_inconsistent_broadcast_for_sig3(bad_idx, 0);
    ctx.use_inconsistent_broadcast_for_sig3(bad_idx, 2);

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.outcome.result.unwrap_err();

    // Needs +1 to map from array idx to signer idx
    assert_eq!(blamed_parties, vec![AccountId([bad_idx as u8 + 1; 32])]);
    assert!(ctx.tag_cache.contains_tag(SIGNING_CEREMONY_FAILED));
}

#[tokio::test]
async fn should_report_on_timeout_before_request_to_sign() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = keygen_states.key_ready_data().clients[0].clone();

    c1.is_at_signing_stage(0).unwrap();

    let bad_array_idxs = [1usize, 2];

    for idx in bad_array_idxs.iter() {
        c1.receive_signing_stage_data(1, &sign_states, *idx);
    }

    c1.is_at_signing_stage(0).unwrap();

    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.outcome_receivers[0], &bad_array_idxs).await;
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_EXPIRED));
}

/// If a ceremony expires in the middle of any stage,
/// we should report the slow parties
#[tokio::test]
async fn should_report_on_timeout_stage() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let bad_party_idxs = [1];
    let good_party_idx = 2;

    // Test the timeout for all stages
    for stage in 1..=*SIGNING_STAGES {
        // Get a client at the correct stage
        let mut c1 = sign_states.get_client_at_stage(stage);

        // Receive data from one client but not the others
        c1.receive_signing_stage_data(stage, &sign_states, good_party_idx);

        // Trigger timeout
        c1.expire_all();
        c1.cleanup();

        // Check that the late 2 clients are correctly reported
        check_blamed_paries(&mut ctx.outcome_receivers[0], &bad_party_idxs).await;
        assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_EXPIRED));
    }
}

#[tokio::test]
async fn should_ignore_duplicate_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;

    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_phase2.clients[0].clone();
    c1.is_at_signing_stage(2).unwrap();

    // Send another request to sign with the same ceremony_id and key_id
    c1.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // The request should have been rejected and the existing ceremony is unchanged
    c1.is_at_signing_stage(2).unwrap();
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_delay_rts_until_key_is_ready() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.ver_comp_stage5.as_ref().unwrap().clients[0].clone();
    c1.is_at_signing_stage(0).unwrap();

    // send the request to sign
    c1.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // The request should have been delayed, so the stage is unaffected
    c1.is_at_signing_stage(0).unwrap();

    // complete the keygen by sending the ver5 from each other client to client 0
    for sender_idx in 1..=3 {
        // send all but 1 ver2 data to the client
        let s_id =
            keygen_states.ver_comp_stage5.as_ref().unwrap().clients[sender_idx].get_my_account_id();
        let ver5 = keygen_states.ver_comp_stage5.as_ref().unwrap().ver5[sender_idx].clone();

        let m = helpers::keygen_data_to_p2p(ver5.clone(), &s_id, KEYGEN_CEREMONY_ID);
        c1.process_p2p_message(m);
    }

    // Now that the keygen completed, the rts should have been processed
    c1.is_at_signing_stage(1).unwrap();
}

#[tokio::test]
async fn should_ignore_rts_with_unknown_signer_id() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.key_ready_data().clients[0].clone();
    c1.is_at_signing_stage(0).unwrap();

    // Get an id that was not in the keygen and substitute it in the signer list
    let unknown_signer_id = AccountId([0; 32]);
    assert!(!VALIDATOR_IDS.contains(&unknown_signer_id));
    let mut signer_ids = SIGNER_IDS.clone();
    signer_ids[1] = unknown_signer_id;

    // Send the rts with the modified signer_ids
    c1.send_request_to_sign_default(ctx.key_id(), signer_ids);

    // The rts should not have started a ceremony
    c1.is_at_signing_stage(0).unwrap();
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_ignore_rts_if_not_participating() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.key_ready_data().clients[3].clone();
    c1.is_at_signing_stage(0).unwrap();

    // Make sure our id is not in the signers list
    assert!(!SIGNER_IDS.contains(&c1.get_my_account_id()));

    // Send the request to sign
    c1.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // The rts should not have started a ceremony
    c1.is_at_signing_stage(0).unwrap();
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_ignore_rts_with_incorrect_amount_of_signers() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.key_ready_data().clients[0].clone();
    c1.is_at_signing_stage(0).unwrap();

    // Send the request to sign with not enough signers
    let mut signer_ids = SIGNER_IDS.clone();
    let _ = signer_ids.pop();
    c1.send_request_to_sign_default(ctx.key_id(), signer_ids);

    // The rts should not have started a ceremony and we should see an error tag
    c1.is_at_signing_stage(0).unwrap();
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
    ctx.tag_cache.clear();

    // Send the request to sign with too many signers
    let mut signer_ids = SIGNER_IDS.clone();
    signer_ids.push(VALIDATOR_IDS[3].clone());
    c1.send_request_to_sign_default(ctx.key_id(), signer_ids);

    // The rts should not have started a ceremony and we should see an error tag
    c1.is_at_signing_stage(0).unwrap();
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn pending_rts_should_expire() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.ver_comp_stage5.as_ref().unwrap().clients[0].clone();
    c1.is_at_signing_stage(0).unwrap();

    // Send the rts with the key id currently unknown to the client
    c1.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // Timeout all the requests
    c1.expire_all();
    c1.cleanup();

    // Complete the keygen by sending the ver5 from each other client to client 0
    for sender_idx in 1..=3 {
        let s_id =
            keygen_states.ver_comp_stage5.as_ref().unwrap().clients[sender_idx].get_my_account_id();
        let ver5 = keygen_states.ver_comp_stage5.as_ref().unwrap().ver5[sender_idx].clone();

        let m = helpers::keygen_data_to_p2p(ver5.clone(), &s_id, KEYGEN_CEREMONY_ID);
        c1.process_p2p_message(m);
    }

    // Should be no pending rts, so no stage advancement once the keygen completed.
    c1.is_at_signing_stage(0).unwrap();
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_EXPIRED));
}

// Ignore unexpected messages at all stages. This includes:
// - Messages with stage data that is not the current stage or the next stage
// - Duplicate messages from the same sender AccountId
// - Messages from unknown AccountId or not in the signing ceremony
#[tokio::test]
async fn should_ignore_unexpected_message_for_stage() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    // Get an id that is not in the keygen ceremony
    let unknown_id = AccountId([0; 32]);
    assert!(!VALIDATOR_IDS.contains(&unknown_id));

    // Test for all keygen stages
    for current_stage in 1..=*SIGNING_STAGES {
        // Get a client at the correct stage
        let mut c1 = sign_states.get_client_at_stage(current_stage);

        // Get the correct data from 1 client so that we only need one more to advance
        c1.receive_signing_stage_data(current_stage, &sign_states, 1);

        // Receive messages from all unexpected stages (not the current stage or the next)
        for stage in 1..=*SIGNING_STAGES {
            if stage != current_stage && stage != current_stage + 1 {
                c1.receive_signing_stage_data(stage, &sign_states, 2);
            }
        }
        assert!(
            c1.is_at_signing_stage(current_stage).is_ok(),
            "Failed to ignore a message from an unexpected stage"
        );

        // Receive a duplicate message
        c1.receive_signing_stage_data(current_stage, &sign_states, 1);
        assert!(
            c1.is_at_signing_stage(current_stage).is_ok(),
            "Failed to ignore a message from a duplicate sender id"
        );

        // Receive a message from an unknown AccountId
        let message =
            c1.get_signing_p2p_message_for_stage(current_stage, &sign_states, 1, &unknown_id);
        c1.process_p2p_message(message);
        assert!(
            c1.is_at_signing_stage(current_stage).is_ok(),
            "Failed to ignore a message from an unknown id"
        );

        // Receive a message from a node that is not in the signing ceremony
        let non_participant_id = &VALIDATOR_IDS[3];
        assert!(!SIGNER_IDS.contains(&non_participant_id));
        let message = c1.get_signing_p2p_message_for_stage(
            current_stage,
            &sign_states,
            1,
            &non_participant_id,
        );
        c1.process_p2p_message(message);
        assert!(
            c1.is_at_signing_stage(current_stage).is_ok(),
            "Failed to ignore a message from an non-participant"
        );

        // Receive the last message and advance the stage
        c1.receive_signing_stage_data(current_stage, &sign_states, 2);
        if current_stage + 1 > *SIGNING_STAGES {
            // The keygen finished
            c1.is_at_signing_stage(0).unwrap();
        } else {
            c1.is_at_signing_stage(current_stage + 1).unwrap();
        }
    }
}

// If the list of signers in the sign request contains a duplicate id, the request should be ignored
#[tokio::test]
async fn should_ignore_keygen_request_with_duplicate_signer() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.key_ready_data().clients[0].clone();
    c1.is_at_signing_stage(0).unwrap();

    // Send the request to sign with a duplicate ID in the signers
    let mut signer_ids = SIGNER_IDS.clone();
    signer_ids[1] = signer_ids[2].clone();
    c1.send_request_to_sign_default(ctx.key_id(), signer_ids);

    // The rts should not have started a ceremony and we should see an error tag
    c1.is_at_signing_stage(0).unwrap();
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}
