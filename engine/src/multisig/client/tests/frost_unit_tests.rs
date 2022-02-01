use crate::multisig::client::{
    self,
    tests::helpers::{
        check_blamed_paries, gen_invalid_signing_comm1, next_with_timeout, sig_data_to_p2p,
        STAGE_FINISHED_OR_NOT_STARTED,
    },
};

use client::tests::*;

use crate::testing::assert_ok;

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
    for stage in 1..SIGNING_STAGES {
        // Get a client at the correct stage
        let mut c0 = sign_states.get_client_at_stage(&ctx.get_account_id(0), stage);

        let id1 = ctx.get_account_id(1);

        // Receive the data of this stage and the next from all but 1 client
        c0.receive_signing_stage_data(stage, &sign_states, &id1);
        c0.receive_signing_stage_data(stage + 1, &sign_states, &id1);
        assert_ok!(c0.ensure_at_signing_stage(stage));

        let id2 = ctx.get_account_id(2);

        // Now receive the final clients data to advance the stage
        c0.receive_signing_stage_data(stage, &sign_states, &id2);
        assert_ok!(c0.ensure_at_signing_stage(stage + 1));

        // If the messages were delayed properly, then receiving
        // the last clients data will advance the stage again
        c0.receive_signing_stage_data(stage + 1, &sign_states, &id2);

        // Check that the stage correctly advanced or finished
        if stage + 2 > SIGNING_STAGES {
            // The keygen finished
            assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
        } else {
            assert_ok!(c0.ensure_at_signing_stage(stage + 2));
        }
    }
}

// If any initial commitments arrive before the request to sign,
// they should be delayed and processed after it arrives
#[tokio::test]
async fn should_delay_comm1_before_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let id0 = ctx.get_account_id(0);

    let mut c0 = keygen_states
        .key_ready_data()
        .expect("successful keygen")
        .clients[&id0]
        .clone();
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

    // Send comm1 from the other 2 clients before the request to sign
    c0.receive_signing_stage_data(1, &sign_states, &ctx.get_account_id(1));
    c0.receive_signing_stage_data(1, &sign_states, &ctx.get_account_id(2));

    // Now get the request to sign
    c0.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // It should advance to stage 2 right away if the comm1's were delayed correctly
    assert_ok!(c0.ensure_at_signing_stage(2));
}

#[tokio::test]
async fn should_handle_invalid_local_sig() {
    let mut ctx = helpers::KeygenContext::new();
    let _keygen_states = ctx.generate().await;
    ctx.auto_clear_tag_cache = false;

    // Party at this idx will send an invalid signature
    let bad_id = ctx.get_account_id(1);

    ctx.use_invalid_local_sig(&bad_id);

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.sign_finished.outcome.result.unwrap_err();

    assert_eq!(blamed_parties, vec![bad_id]);
    assert!(ctx.tag_cache.contains_tag(SIGNING_CEREMONY_FAILED));
}

#[tokio::test]
async fn should_handle_inconsistent_broadcast_com1() {
    let mut ctx = helpers::KeygenContext::new();
    let _keygen_states = ctx.generate().await;
    ctx.auto_clear_tag_cache = false;

    // Party at this idx will send and invalid signature
    let bad_id = ctx.get_account_id(1);

    ctx.use_inconsistent_broadcast_for_signing_comm1(&bad_id, &ctx.get_account_id(0));
    ctx.use_inconsistent_broadcast_for_signing_comm1(&bad_id, &ctx.get_account_id(2));

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.sign_finished.outcome.result.unwrap_err();

    assert_eq!(blamed_parties, vec![bad_id]);
    assert!(ctx.tag_cache.contains_tag(SIGNING_CEREMONY_FAILED));
}

#[tokio::test]
async fn should_handle_inconsistent_broadcast_sig3() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    ctx.auto_clear_tag_cache = false;

    // Party at this idx will send and invalid signature
    // This is the index in the array
    let bad_id = ctx.get_account_id(1);

    ctx.use_inconsistent_broadcast_for_sig3(&bad_id, &ctx.get_account_id(0));
    ctx.use_inconsistent_broadcast_for_sig3(&bad_id, &ctx.get_account_id(2));

    let sign_states = ctx.sign().await;

    let (_, blamed_parties) = sign_states.sign_finished.outcome.result.unwrap_err();

    assert_eq!(blamed_parties, vec![bad_id]);
    assert!(ctx.tag_cache.contains_tag(SIGNING_CEREMONY_FAILED));
}

#[tokio::test]
async fn should_ignore_duplicate_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;

    let sign_states = ctx.sign().await;

    let id0 = ctx.get_account_id(0);

    let mut c0 = sign_states.sign_phase2.clients[&id0].clone();
    assert_ok!(c0.ensure_at_signing_stage(2));

    // Send another request to sign with the same ceremony_id and key_id
    c0.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // The request should have been rejected and the existing ceremony is unchanged
    assert_ok!(c0.ensure_at_signing_stage(2));
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_delay_rts_until_key_is_ready() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let id0 = ctx.get_account_id(0);

    let mut c0 = keygen_states.ver_comp_stage5.as_ref().unwrap().clients[&id0].clone();
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

    // send the request to sign
    c0.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // The request should have been delayed, so the stage is unaffected
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

    // complete the keygen by sending the ver5 from each other client to client 0
    for sender_id in ctx.get_account_ids() {
        if sender_id != &id0 {
            let ver5 = keygen_states.ver_comp_stage5.as_ref().unwrap().ver5[&sender_id].clone();
            let message = helpers::keygen_data_to_p2p(ver5);
            c0.process_p2p_message(sender_id.clone(), message);
        }
    }

    // Now that the keygen completed, the rts should have been processed
    assert_ok!(c0.ensure_at_signing_stage(1));
}

#[tokio::test]
async fn should_ignore_rts_with_unknown_signer_id() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let id0 = ctx.get_account_id(0);

    let mut c0 = keygen_states
        .key_ready_data()
        .expect("successful keygen")
        .clients[&id0]
        .clone();
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

    // Get an id that was not in the keygen and substitute it in the signer list
    let unknown_signer_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&unknown_signer_id));
    let mut signer_ids = SIGNER_IDS.clone();
    signer_ids[1] = unknown_signer_id;

    // Send the rts with the modified signer_ids
    c0.send_request_to_sign_default(ctx.key_id(), signer_ids);

    // The rts should not have started a ceremony
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_ignore_rts_if_not_participating() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let id3 = ctx.get_account_id(3);

    let mut c1 = keygen_states
        .key_ready_data()
        .expect("successful keygen")
        .clients[&id3]
        .clone();
    assert_ok!(c1.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

    // Make sure our id is not in the signers list
    assert!(!SIGNER_IDS.contains(&c1.get_my_account_id()));

    // Send the request to sign
    c1.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // The rts should not have started a ceremony
    assert_ok!(c1.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_ignore_rts_with_incorrect_amount_of_signers() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let id0 = ctx.get_account_id(0);
    let mut c0 = keygen_states
        .key_ready_data()
        .expect("successful keygen")
        .clients[&id0]
        .clone();
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

    // Send the request to sign with not enough signers
    let mut signer_ids = SIGNER_IDS.clone();
    let _ = signer_ids.pop();
    c0.send_request_to_sign_default(ctx.key_id(), signer_ids);

    // The rts should not have started a ceremony and we should see an error tag
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn pending_rts_should_expire() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let id0 = ctx.get_account_id(0);
    let mut c0 = keygen_states.ver_comp_stage5.as_ref().unwrap().clients[&id0].clone();
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

    // Send the rts with the key id currently unknown to the client
    c0.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // Timeout all the requests
    c0.force_stage_timeout();

    // Complete the keygen by sending the ver5 from each other client to client 0
    for sender_id in ctx.get_account_ids() {
        if sender_id != &id0 {
            let ver5 = keygen_states.ver_comp_stage5.as_ref().unwrap().ver5[&sender_id].clone();
            let message = helpers::keygen_data_to_p2p(ver5);
            c0.process_p2p_message(sender_id.clone(), message);
        }
    }

    // Should be no pending rts, so no stage advancement once the keygen completed.
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
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
    let unknown_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&unknown_id));

    // Test for all keygen stages
    for current_stage in 1..=SIGNING_STAGES {
        // Get a client at the correct stage
        let mut c0 = sign_states.get_client_at_stage(&ctx.get_account_id(0), current_stage);

        // Get the correct data from 1 client so that we only need one more to advance
        c0.receive_signing_stage_data(current_stage, &sign_states, &ctx.get_account_id(1));

        // Receive messages from all unexpected stages (not the current stage or the next)
        for stage in 1..=SIGNING_STAGES {
            if stage != current_stage && stage != current_stage + 1 {
                c0.receive_signing_stage_data(stage, &sign_states, &ctx.get_account_id(2));
            }
        }
        assert!(
            c0.ensure_at_signing_stage(current_stage).is_ok(),
            "Failed to ignore a message from an unexpected stage"
        );

        // Receive a duplicate message
        c0.receive_signing_stage_data(current_stage, &sign_states, &ctx.get_account_id(1));
        assert!(
            c0.ensure_at_signing_stage(current_stage).is_ok(),
            "Failed to ignore a message from a duplicate sender id"
        );

        // Receive a message from an unknown AccountId
        let message = c0.get_signing_p2p_message_for_stage(
            current_stage,
            &sign_states,
            &ctx.get_account_id(1),
        );
        c0.process_p2p_message(unknown_id.clone(), message);
        assert!(
            c0.ensure_at_signing_stage(current_stage).is_ok(),
            "Failed to ignore a message from an unknown id"
        );

        // Receive a message from a node that is not in the signing ceremony

        let non_participant_id = ctx.get_account_id(3);

        assert!(!SIGNER_IDS.contains(&non_participant_id));
        let message = c0.get_signing_p2p_message_for_stage(
            current_stage,
            &sign_states,
            &ctx.get_account_id(1),
        );
        c0.process_p2p_message(non_participant_id.clone(), message);
        assert!(
            c0.ensure_at_signing_stage(current_stage).is_ok(),
            "Failed to ignore a message from an non-participant"
        );

        // Receive the last message and advance the stage
        c0.receive_signing_stage_data(current_stage, &sign_states, &ctx.get_account_id(2));
        if current_stage + 1 > SIGNING_STAGES {
            // The keygen finished
            assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
        } else {
            assert_ok!(c0.ensure_at_signing_stage(current_stage + 1));
        }
    }
}

// If the list of signers in the sign request contains a duplicate id, the request should be ignored
#[tokio::test]
async fn should_ignore_rts_with_duplicate_signer() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let id0 = ctx.get_account_id(0);
    let mut c0 = keygen_states
        .key_ready_data()
        .expect("successful keygen")
        .clients[&id0]
        .clone();
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

    // Send the request to sign with a duplicate ID in the signers
    let mut signer_ids = SIGNER_IDS.clone();
    signer_ids[1] = signer_ids[2].clone();
    c0.send_request_to_sign_default(ctx.key_id(), signer_ids);

    // The rts should not have started a ceremony and we should see an error tag
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_ignore_rts_with_used_ceremony_id() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    // Get a client and finish a signing ceremony
    let mut c0 = sign_states.get_client_at_stage(&ctx.get_account_id(0), 4);
    c0.receive_signing_stage_data(4, &sign_states, &ctx.get_account_id(1));
    c0.receive_signing_stage_data(4, &sign_states, &ctx.get_account_id(2));

    // Send an rts with the same ceremony id (the default signing ceremony id for tests)
    c0.send_request_to_sign_default(ctx.key_id(), SIGNER_IDS.clone());

    // The rts should have been ignored
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert!(ctx.tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));
}

#[tokio::test]
async fn should_ignore_stage_data_with_used_ceremony_id() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;
    let sign_states = ctx.sign().await;

    let mut c1 = sign_states.sign_finished.clients[&ctx.get_account_id(0)].clone();
    assert_eq!(c1.ceremony_manager.get_signing_states_len(), 0);

    // Receive comm1 from a used ceremony id (the default signing ceremony id)
    let message = sig_data_to_p2p(sign_states.sign_phase1.comm1s[&ctx.get_account_id(1)].clone());
    assert_eq!(message.ceremony_id, SIGN_CEREMONY_ID);
    c1.process_p2p_message(ACCOUNT_IDS[1].clone(), message);

    // The message should have been ignored and no ceremony was started
    // In this case, the ceremony would be unauthorised, so we must check how many signing states exist
    // to see if a unauthorised state was created.
    assert_eq!(c1.ceremony_manager.get_signing_states_len(), 0);
}

#[tokio::test]
async fn should_not_consume_ceremony_id_if_unauthorised() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    // Get a client that has not used the default signing ceremony id yet
    let id0 = ctx.get_account_id(0);
    let mut c0 = keygen_states
        .key_ready_data()
        .expect("successful keygen")
        .clients[&id0]
        .clone();
    assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));
    assert_eq!(c0.ceremony_manager.get_signing_states_len(), 0);

    // Receive comm1 with the default signing ceremony id
    let message = sig_data_to_p2p(gen_invalid_signing_comm1());
    assert_eq!(message.ceremony_id, SIGN_CEREMONY_ID);
    c0.process_p2p_message(ACCOUNT_IDS[1].clone(), message);

    // Check that the unauthorised ceremony was created
    assert_eq!(c0.ceremony_manager.get_signing_states_len(), 1);

    // Timeout the unauthorised ceremony
    c0.force_stage_timeout();

    // Clear out the timeout outcome
    next_with_timeout(ctx.outcome_receivers.get_mut(&id0).unwrap()).await;

    // Sign as normal using the default ceremony id
    let sign_states = ctx.sign().await;

    // Should not have been rejected because of a used ceremony id
    assert!(sign_states.sign_finished.outcome.result.is_ok());
}

#[tokio::test]
async fn should_sign_with_all_parties() {
    let mut ctx = helpers::KeygenContext::new();
    let _ = ctx.generate().await;

    // Run the signing ceremony using all of the accounts that were in keygen (ACCOUNT_IDS)
    assert_ok!(
        ctx.sign_custom(&*SIGNER_IDS, None)
            .await
            .sign_finished
            .outcome
            .result
    );
}

mod timeout {

    // What should be tested w.r.t timeouts:

    // 0. [ignored] If timeout during an "unauthorised" ceremony, we report the nodes that attempted to start it
    //           (i.e. whoever send stage data for the ceremony)

    // 1a. [done] If timeout during a broadcast verification stage, and we have enough data, we can recover
    // 1b. [done] If timeout during a broadcast verification stage, and we don't have enough data to
    //            recover some of the parties messages, we report those parties (note that we can't report
    //            the parties that were responsible for the timeout in the first place as we would need
    //            another round of "voting" which can also timeout, and then we are back where we started)

    // 2a.        If timeout during a regular stage, but the majority of nodes can agree on all values,
    //            we proceed with the ceremony and use the data received by the majority
    // 2b. [done] If timeout during a regular stage, and the majority of nodes didn't receive the data
    //            from some nodes (i.e. they vote on value `None`), those nodes are reported
    // 2c.        Same as [2b], but the nodes are reported if the majority can't agree on any one value
    //            (even if all values are `Some(...)` such as when a node does an inconsistent broadcast)

    // 3.         If timeout before the key is ready, the ceremony should be ignored, but need to ensure that
    //    we return a response

    use super::*;

    // This covers [0]
    #[tokio::test]
    #[ignore = "functionality disabled as SC does not expect this response"]
    async fn should_report_on_timeout_before_request_to_sign() {
        let mut ctx = helpers::KeygenContext::new();
        let keygen_states = ctx.generate().await;
        let sign_states = ctx.sign().await;

        let id0 = ctx.get_account_id(0);

        let mut c0 = keygen_states
            .key_ready_data()
            .expect("successful keygen")
            .clients[&id0]
            .clone();

        assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

        let bad_array_ids = [ctx.get_account_id(1), ctx.get_account_id(2)];

        for id in &bad_array_ids {
            c0.receive_signing_stage_data(1, &sign_states, id);
        }

        assert_ok!(c0.ensure_at_signing_stage(STAGE_FINISHED_OR_NOT_STARTED));

        c0.force_stage_timeout();

        check_blamed_paries(ctx.outcome_receivers.get_mut(&id0).unwrap(), &bad_array_ids).await;
    }

    mod during_regular_stage {

        // This covers 2a
        async fn recover_if_party_appears_offline_to_minority(stage_idx: usize) {
            // If a party times out during a regular stage,
            // and the majority of nodes agree on this in the following
            // (broadcast verification) stage, the party gets reported
            let mut ctx = helpers::KeygenContext::new();
            let _ = ctx.generate().await;

            let bad_party_id = ctx.get_account_id(1);

            let other_party_id = ctx.get_account_id(2);

            ctx.force_party_timeout_signing(&bad_party_id, Some(&other_party_id), stage_idx);

            let result = ctx.sign().await.sign_finished.outcome.result;

            assert_ok!(result);
        }

        use super::*;
        // This covers 2b
        async fn offline_party_should_be_reported(stage_idx: usize) {
            // If a party times out during a regular stage,
            // and the majority of nodes agree on this in the following
            // (broadcast verification) stage, the party gets reported
            let mut ctx = helpers::KeygenContext::new();
            let _ = ctx.generate().await;

            let bad_party_id = ctx.get_account_id(1);

            ctx.force_party_timeout_signing(&bad_party_id, None, stage_idx);

            let result = ctx.sign().await.sign_finished.outcome.result;

            let error = result.as_ref().unwrap_err();

            assert_eq!(error.1, &[bad_party_id]);
        }

        #[tokio::test]
        async fn recover_if_party_appears_offline_to_minority_stage1() {
            recover_if_party_appears_offline_to_minority(1).await;
        }

        #[tokio::test]
        async fn recover_if_party_appears_offline_to_minority_stage3() {
            recover_if_party_appears_offline_to_minority(3).await;
        }

        #[tokio::test]
        async fn offline_party_should_be_reported_stage1() {
            offline_party_should_be_reported(1).await;
        }

        #[tokio::test]
        async fn offline_party_should_be_reported_stage3() {
            offline_party_should_be_reported(3).await;
        }
    }

    mod during_broadcast_verification_stage {

        use super::*;

        // This covers 1a
        async fn recover_if_agree_on_values(stage_idx: usize) {
            let mut ctx = helpers::KeygenContext::new();
            let _ = ctx.generate().await;

            let bad_party_id = ctx.get_account_id(1);

            ctx.force_party_timeout_signing(&bad_party_id, None, stage_idx);

            assert_ok!(ctx.sign().await.sign_finished.outcome.result);
        }

        #[tokio::test]
        async fn recover_if_agree_on_values_stage2() {
            recover_if_agree_on_values(2).await;
        }

        #[tokio::test]
        async fn recover_if_agree_on_values_stage4() {
            recover_if_agree_on_values(4).await;
        }

        // This covers 1b
        async fn report_if_cannot_agree_on_values(stage_idx: usize) {
            assert!(
                stage_idx > 1,
                "expected a broadcast verification stage index"
            );

            let mut ctx = helpers::KeygenContext::new();
            let _ = ctx.generate().await;

            // This party will time out during the preceding regular stage,
            // it should be reported
            let bad_party_1 = ctx.get_account_id(1);
            ctx.force_party_timeout_signing(&bad_party_1, None, stage_idx - 1);

            // This party will time out during a broadcast
            // verification stage, it won't get reported
            // (ideally it would, but we can't due to the
            // limitations of the protocol)
            let bad_party_2 = ctx.get_account_id(2);
            ctx.force_party_timeout_signing(&bad_party_2, None, stage_idx);

            let result = ctx.sign().await.sign_finished.outcome.result;

            let error = result.as_ref().unwrap_err();

            assert_eq!(error.1, &[bad_party_1]);
        }

        #[tokio::test]
        async fn report_if_cannot_agree_on_values_stage_2() {
            report_if_cannot_agree_on_values(2).await;
        }

        #[tokio::test]
        async fn report_if_cannot_agree_on_values_stage_4() {
            report_if_cannot_agree_on_values(4).await;
        }
    }
}
