use rand_legacy::{FromEntropy, SeedableRng};
use tokio::sync::oneshot;

use crate::multisig::{
    client::{
        keygen::{self, SecretShare3},
        tests::helpers::{
            gen_invalid_keygen_comm1, modify_participants, new_node, new_nodes, run_keygen,
            split_messages_for, STAGE_FINISHED_OR_NOT_STARTED,
        },
        utils::PartyIdxMapping,
    },
    crypto::Rng,
    KeygenOptions,
};

use super::helpers::{
    self, all_stages_with_single_invalid_share_keygen_coroutine, for_each_stage, standard_keygen,
    KeygenCeremonyRunner,
};

use crate::testing::assert_ok;

use super::*;

use crate::logging::KEYGEN_REQUEST_IGNORED;

/// If all nodes are honest and behave as expected we should
/// generate a key without entering a blaming stage
#[tokio::test]
async fn happy_path_results_in_valid_key() {
    let (_, _, _, _) = run_keygen(
        new_nodes(ACCOUNT_IDS.clone()),
        1,
        KeygenOptions::allowing_high_pubkey(),
    )
    .await;
}

/*
/// If keygen state expires before a formal request to keygen
/// (from our SC), we should report initiators of that ceremony
#[tokio::test]
#[ignore = "functionality disabled as SC does not expect this response"]
async fn should_report_on_timeout_before_keygen_request() {
    let (_, _, messages, _nodes) = run_keygen(
        new_nodes(ACCOUNT_IDS.clone()),
        1,
        KeygenOptions::allowing_high_pubkey(),
    )
    .await;

    let good_account_id = &ACCOUNT_IDS[0];

    let mut node = new_node(good_account_id.clone());

    let bad_account_id = ACCOUNT_IDS[1].clone();

    node.ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[1].clone(),
        1,
        messages.stage_1_messages[&bad_account_id][good_account_id]
            .clone()
            .into(),
    );

    // Force all ceremonies to time out
    node.force_stage_timeout();

    let (_, reported) = node
        .try_recv_outcome::<KeygenResultInfo>()
        .await
        .unwrap()
        .result
        .unwrap_err();
    assert_eq!(&[bad_account_id], &reported[..]);
}*/

#[tokio::test]
async fn should_delay_comm1_before_keygen_request() {
    let ceremony_id = 1;
    let new_keygen_ceremony = || {
        KeygenCeremonyRunner::new(
            new_nodes(ACCOUNT_IDS.clone()),
            ceremony_id,
            KeygenOptions::allowing_high_pubkey(),
            Rng::from_seed([8; 32]),
        )
    };

    let (_, _, messages, _nodes) = standard_keygen(new_keygen_ceremony()).await;

    let mut ceremony = new_keygen_ceremony();
    let [test_id, late_id] = ceremony.select_account_ids();

    let (late_msg, early_msgs) =
        split_messages_for(messages.stage_1_messages.clone(), &test_id, &late_id);

    ceremony.distribute_messages(early_msgs);

    assert_ok!(ceremony.nodes[&test_id]
        .ensure_ceremony_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED, ceremony.ceremony_id));

    ceremony.request().await;

    assert_ok!(ceremony.nodes[&test_id].ensure_ceremony_at_keygen_stage(1, ceremony.ceremony_id));

    ceremony.distribute_messages(late_msg);

    assert_ok!(ceremony.nodes[&test_id].ensure_ceremony_at_keygen_stage(2, ceremony.ceremony_id));
}

// Data for any stage that arrives one stage too early should be properly delayed
// and processed after the stage transition is made
#[tokio::test]
async fn should_delay_stage_data() {
    for_each_stage(
        1..KEYGEN_STAGES,
        || {
            Box::pin(async {
                KeygenCeremonyRunner::new(
                    new_nodes(ACCOUNT_IDS.clone()),
                    1,
                    KeygenOptions::allowing_high_pubkey(),
                    Rng::from_seed([8; 32]),
                )
            })
        },
        all_stages_with_single_invalid_share_keygen_coroutine,
        |stage_number, mut ceremony, (_key_id, messages, _type_messages)| async move {
            let target_account_id = &ACCOUNT_IDS[0];
            let late_account_id = ACCOUNT_IDS[3].clone();
            let (late_messages, early_messages) = split_messages_for(
                messages[stage_number - 1].clone(),
                target_account_id,
                &late_account_id,
            );
            ceremony.distribute_messages(early_messages);

            let (late_messages_next, early_messages) = split_messages_for(
                messages[stage_number].clone(),
                target_account_id,
                &late_account_id,
            );
            ceremony.distribute_messages(early_messages);

            assert_ok!(ceremony.nodes[target_account_id]
                .ensure_ceremony_at_keygen_stage(stage_number, ceremony.ceremony_id));

            ceremony.distribute_messages(late_messages);

            assert_ok!(ceremony.nodes[target_account_id]
                .ensure_ceremony_at_keygen_stage(stage_number + 1, ceremony.ceremony_id));

            ceremony.distribute_messages(late_messages_next);

            // Check that the stage correctly advanced or finished
            assert_ok!(
                ceremony.nodes[target_account_id].ensure_ceremony_at_keygen_stage(
                    if stage_number + 2 > KEYGEN_STAGES {
                        STAGE_FINISHED_OR_NOT_STARTED
                    } else {
                        stage_number + 2
                    },
                    ceremony.ceremony_id
                )
            );
        },
    )
    .await;
}

/// If at least one party is blamed during the "Complaints" stage, we
/// should enter a blaming stage, where the blamed party sends a valid
/// share, so the ceremony should be successful in the end
#[tokio::test]
async fn should_enter_blaming_stage_on_invalid_secret_shares() {
    let mut ceremony = KeygenCeremonyRunner::new(
        new_nodes(ACCOUNT_IDS.iter().cloned()),
        1,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
    );

    let (messages, result_receivers) = ceremony.request().await;

    let mut messages = helpers::run_stages!(
        ceremony,
        messages,
        keygen::VerifyComm2,
        keygen::SecretShare3
    );

    // stage 3 - with account 0 sending account 1 a bad secret share
    *messages
        .get_mut(&ACCOUNT_IDS[0])
        .unwrap()
        .get_mut(&ACCOUNT_IDS[1])
        .unwrap() = SecretShare3::create_random(&mut ceremony.rng);

    let messages = helpers::run_stages!(
        ceremony,
        messages,
        keygen::Complaints4,
        keygen::VerifyComplaints5,
        keygen::BlameResponse6,
        keygen::VerifyBlameResponses7
    );
    ceremony.distribute_messages(messages);
    ceremony.complete(result_receivers).await;
}

/// If one or more parties send an invalid secret share both the first
/// time and during the blaming stage, the ceremony is aborted with these
/// parties reported
#[tokio::test]
async fn should_report_on_invalid_blame_response() {
    let mut ceremony = KeygenCeremonyRunner::new(
        new_nodes(ACCOUNT_IDS.iter().cloned()),
        1,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
    );
    let party_idx_mapping = PartyIdxMapping::from_unsorted_signers(
        &ceremony.nodes.keys().cloned().collect::<Vec<_>>()[..],
    );
    let [bad_node_id_1, bad_node_id_2, target_node_id] = ceremony.select_account_ids();

    // stage 1
    let (messages, result_receivers) = ceremony.request().await;

    let mut messages = helpers::run_stages!(
        ceremony,
        messages,
        keygen::VerifyComm2,
        keygen::SecretShare3
    );

    // stage 3 - with bad_node_id_1, and bad_node_id_2 sending a bad secret share
    *messages
        .get_mut(&bad_node_id_1)
        .unwrap()
        .get_mut(&target_node_id)
        .unwrap() = SecretShare3::create_random(&mut ceremony.rng);

    *messages
        .get_mut(&bad_node_id_2)
        .unwrap()
        .get_mut(&target_node_id)
        .unwrap() = SecretShare3::create_random(&mut ceremony.rng);

    let mut messages = helpers::run_stages!(
        ceremony,
        messages,
        keygen::Complaints4,
        keygen::VerifyComplaints5,
        keygen::BlameResponse6
    );

    // stage 7 - bad_node_id_1 also sends a bad blame responses, and so gets blamed when ceremony finished
    let secret_share = SecretShare3::create_random(&mut ceremony.rng);
    for message in messages.get_mut(&bad_node_id_1).unwrap().values_mut() {
        *message = keygen::BlameResponse6(
            std::iter::once((
                party_idx_mapping.get_idx(&bad_node_id_2).unwrap(),
                secret_share.clone(),
            ))
            .collect(),
        )
    }

    let messages = ceremony
        .run_stage::<keygen::VerifyBlameResponses7, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages);
    ceremony
        .complete_with_error(&[bad_node_id_1.clone()], result_receivers)
        .await;
}

/// If party is blamed by one or more peers, its BlameResponse sent in
/// the next stage must be complete, that is, it must contain a (valid)
/// entry for *every* peer it is blamed by. Otherwise the blamed party
/// get reported.
#[tokio::test]
async fn should_report_on_incomplete_blame_response() {
    let mut ceremony = KeygenCeremonyRunner::new(
        new_nodes(ACCOUNT_IDS.iter().cloned()),
        1,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
    );

    let [bad_node_id_1, target_node_id] = ceremony.select_account_ids();

    // stage 1
    let (messages, result_receivers) = ceremony.request().await;

    let mut messages = helpers::run_stages!(
        ceremony,
        messages,
        keygen::VerifyComm2,
        keygen::SecretShare3
    );

    // stage 3 - with bad_node_id_1 sending a bad secret share
    *messages
        .get_mut(&bad_node_id_1)
        .unwrap()
        .get_mut(&target_node_id)
        .unwrap() = SecretShare3::create_random(&mut ceremony.rng);

    let mut messages = helpers::run_stages!(
        ceremony,
        messages,
        keygen::Complaints4,
        keygen::VerifyComplaints5,
        keygen::BlameResponse6
    );

    // stage 7 - bad_node_id_1 sends an empty BlameResponse
    for message in messages.get_mut(&bad_node_id_1).unwrap().values_mut() {
        *message = keygen::BlameResponse6(std::collections::BTreeMap::default())
    }

    let messages = ceremony
        .run_stage::<keygen::VerifyBlameResponses7, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages);
    ceremony
        .complete_with_error(&[bad_node_id_1.clone()], result_receivers)
        .await;
}

#[tokio::test]
async fn should_abort_on_blames_at_invalid_indexes() {
    let mut keygen_ceremony = KeygenCeremonyRunner::new(
        new_nodes(ACCOUNT_IDS.iter().cloned()),
        1,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
    );
    let (messages, result_receivers) = keygen_ceremony.request().await;

    let mut stage_4_messages = helpers::run_stages!(
        keygen_ceremony,
        messages,
        keygen::VerifyComm2,
        keygen::SecretShare3,
        keygen::Complaints4
    );

    let bad_node_id = &ACCOUNT_IDS[1];
    for message in stage_4_messages.get_mut(bad_node_id).unwrap().values_mut() {
        *message = keygen::Complaints4(std::array::IntoIter::new([1, usize::MAX]).collect());
    }

    let stage_5_messages = keygen_ceremony
        .run_stage::<keygen::VerifyComplaints5, _, _>(stage_4_messages)
        .await;
    keygen_ceremony.distribute_messages(stage_5_messages);
    keygen_ceremony
        .complete_with_error(&[bad_node_id.clone()], result_receivers)
        .await;
}

#[tokio::test]
#[should_panic]
async fn should_ignore_keygen_request_if_not_participating() {
    let mut node = new_node(ACCOUNT_IDS[0].clone());

    // Get an id that is not `c0`s id
    let unknown_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&unknown_id));
    let mut keygen_ids = ACCOUNT_IDS.clone();
    keygen_ids[0] = unknown_id;

    // Send the keygen request
    let ceremony_id = 1;
    let (result_sender, _result_receiver) = oneshot::channel();
    node.ceremony_manager.on_keygen_request(
        ceremony_id,
        keygen_ids,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
        result_sender,
    );
}

#[tokio::test]
async fn should_ignore_duplicate_keygen_request() {
    let mut ceremony = KeygenCeremonyRunner::new(
        new_nodes(ACCOUNT_IDS.clone()),
        1,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
    );

    let (messages, _result_receivers) = ceremony.request().await;
    let _messages = ceremony
        .run_stage::<keygen::VerifyComm2, _, _>(messages)
        .await;

    let [node_id] = ceremony.select_account_ids();

    // Send another keygen request with the same ceremony_id but different signers
    let mut keygen_ceremony_details = ceremony.keygen_ceremony_details();
    let unknown_id = AccountId::new([0; 32]);
    assert!(!ceremony.nodes.contains_key(&unknown_id));
    modify_participants(
        &mut keygen_ceremony_details.signers,
        node_id.clone(),
        unknown_id,
    );

    ceremony
        .get_mut_node(&node_id)
        .request_keygen(keygen_ceremony_details);

    // The request should have been rejected and the existing ceremony is unchanged
    assert_ok!(ceremony.nodes[&node_id].ensure_ceremony_at_keygen_stage(2, ceremony.ceremony_id));
    assert!(ceremony
        .get_mut_node(&node_id)
        .tag_cache
        .contains_tag(KEYGEN_REQUEST_IGNORED));
}

// Ignore unexpected messages at all stages. This includes:
// - Messages with stage data that is not the current stage or the next stage
// - Duplicate messages from the same sender AccountId
// - Messages from unknown AccountId (not in the keygen ceremony)
#[tokio::test]
async fn should_ignore_unexpected_message_for_stage() {
    for_each_stage(
        1..=KEYGEN_STAGES,
        || {
            Box::pin(async {
                KeygenCeremonyRunner::new(
                    new_nodes(ACCOUNT_IDS.clone()),
                    1,
                    KeygenOptions::allowing_high_pubkey(),
                    Rng::from_seed([8; 32]),
                )
            })
        },
        all_stages_with_single_invalid_share_keygen_coroutine,
        |stage_number, mut ceremony, (_key_id, messages, _type_messages)| async move {
            let target_account_id = &ACCOUNT_IDS[0];
            let unexpected_message_sender = ACCOUNT_IDS[1].clone();
            let (msg_from_1, other_msgs) = split_messages_for(
                messages[stage_number - 1].clone(),
                target_account_id,
                &unexpected_message_sender,
            );

            ceremony.distribute_messages(other_msgs.clone());

            for ignored_stage_index in (0..stage_number - 1).chain(stage_number + 1..KEYGEN_STAGES)
            {
                let (msg_from_1, _) = split_messages_for(
                    messages[ignored_stage_index].clone(),
                    target_account_id,
                    &unexpected_message_sender,
                );
                ceremony.distribute_messages(msg_from_1);
            }

            assert!(
                ceremony.nodes[target_account_id]
                    .ensure_ceremony_at_keygen_stage(stage_number, ceremony.ceremony_id)
                    .is_ok(),
                "Failed to ignore a message from an unexpected stage"
            );

            ceremony.distribute_messages(other_msgs);
            assert!(
                ceremony.nodes[target_account_id]
                    .ensure_ceremony_at_keygen_stage(stage_number, ceremony.ceremony_id)
                    .is_ok(),
                "Failed to ignore duplicate messages"
            );

            let unknown_id = AccountId::new([0; 32]);
            assert!(!ACCOUNT_IDS.contains(&unknown_id));
            ceremony.distribute_messages(
                msg_from_1
                    .iter()
                    .map(|(_, message)| (unknown_id.clone(), message.clone()))
                    .collect(),
            );
            assert!(
                ceremony.nodes[target_account_id]
                    .ensure_ceremony_at_keygen_stage(stage_number, ceremony.ceremony_id)
                    .is_ok(),
                "Failed to ignore a message from an unknown account id"
            );

            ceremony.distribute_messages(msg_from_1);

            assert!(
                ceremony.nodes[target_account_id]
                    .ensure_ceremony_at_keygen_stage(
                        if stage_number + 1 > KEYGEN_STAGES {
                            STAGE_FINISHED_OR_NOT_STARTED
                        } else {
                            stage_number + 1
                        },
                        ceremony.ceremony_id
                    )
                    .is_ok(),
                "Failed to proceed to next stage"
            );
        },
    )
    .await;
}

// If one of more parties (are thought to) broadcast data inconsistently,
// the ceremony should be aborted and all faulty parties should be reported.
// Fail on `verify_broadcasts` during `VerifyCommitmentsBroadcast2`
#[tokio::test]
async fn should_handle_inconsistent_broadcast_comm1() {
    let mut ceremony = KeygenCeremonyRunner::new(
        new_nodes(ACCOUNT_IDS.clone()),
        1,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
    );

    let (mut messages, result_receivers) = ceremony.request().await;

    let bad_account_id = &ACCOUNT_IDS[1];

    // Make one of the nodes send different comm1 to most of the others
    // Note: the bad node must send different comm1 to more than 1/3 of the participants
    for message in messages.get_mut(bad_account_id).unwrap().values_mut() {
        *message = gen_invalid_keygen_comm1(&mut ceremony.rng);
    }

    let messages = ceremony
        .run_stage::<keygen::VerifyComm2, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages);
    ceremony
        .complete_with_error(&[bad_account_id.clone()], result_receivers)
        .await;
}

// If one or more parties send invalid commitments, the ceremony should be aborted.
// Fail on `validate_commitments` during `VerifyCommitmentsBroadcast2`.
#[tokio::test]
async fn should_handle_invalid_commitments() {
    let mut ceremony = KeygenCeremonyRunner::new(
        new_nodes(ACCOUNT_IDS.clone()),
        1,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
    );

    let (mut messages, result_receivers) = ceremony.request().await;

    let [bad_account_id] = ceremony.select_account_ids();

    // Make a node send a bad commitment to the others
    // Note: we must send the same bad commitment to all of the nodes,
    // or we will fail on the `inconsistent` error instead of the validation error.
    let invalid_comm1 = gen_invalid_keygen_comm1(&mut ceremony.rng);
    for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
        *message = invalid_comm1.clone();
    }

    let messages = ceremony
        .run_stage::<keygen::VerifyComm2, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages);
    ceremony
        .complete_with_error(&[bad_account_id], result_receivers)
        .await;
}

// Keygen aborts if the key is not compatible with the contract at VerifyCommitmentsBroadcast2
// TODO: Once we are able to seed the keygen (deterministic crypto), this test can be replaced
// with a proper test that has a known incompatible aggkey.
#[tokio::test]
async fn should_handle_not_compatible_keygen() {
    let mut counter = 0;
    loop {
        if let Err(()) = run_keygen_with_err_on_high_pubkey(ACCOUNT_IDS.clone()).await {
            break;
        } else {
            // We have a 50/50 chance of failing each time, so we should have failed keygen within 40 tries
            // But it has a 0.0000000001% chance of failing this test as a false positive.
            counter += 1;
            assert!(
                counter < 40,
                "Should have failed keygen with high pub key by now"
            )
        }
    }
}

// If the list of signers in the keygen request contains a duplicate id, the request should be ignored
#[tokio::test]
async fn should_ignore_keygen_request_with_duplicate_signer() {
    let ceremony_id = 1;

    let mut keygen_ids = ACCOUNT_IDS.clone();
    keygen_ids[1] = keygen_ids[2].clone();

    let mut node = new_node(ACCOUNT_IDS[2].clone());

    let (result_sender, _result_receiver) = oneshot::channel();
    node.ceremony_manager.on_keygen_request(
        ceremony_id,
        keygen_ids,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([8; 32]),
        result_sender,
    );

    assert_ok!(node.ensure_ceremony_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED, ceremony_id));
    assert!(node.tag_cache.contains_tag(KEYGEN_REQUEST_IGNORED));
}

#[tokio::test]
async fn should_ignore_keygen_request_with_used_ceremony_id() {
    let ceremony_id = 1;

    let (_, _, _messages, mut nodes) = run_keygen(
        new_nodes(ACCOUNT_IDS.iter().cloned()),
        ceremony_id,
        KeygenOptions::allowing_high_pubkey(),
    )
    .await;

    let node = nodes.get_mut(&ACCOUNT_IDS[0]).unwrap();

    // use the same ceremony id as was used in the previous ceremony
    let (result_sender, _result_receiver) = oneshot::channel();
    node.ceremony_manager.on_keygen_request(
        ceremony_id,
        ACCOUNT_IDS.clone(),
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_entropy(),
        result_sender,
    );

    assert_ok!(node.ensure_ceremony_at_keygen_stage(STAGE_FINISHED_OR_NOT_STARTED, ceremony_id));

    assert!(node.tag_cache.contains_tag(KEYGEN_REQUEST_IGNORED));
}

#[tokio::test]
async fn should_ignore_stage_data_with_used_ceremony_id() {
    let ceremony_id = 1;

    let (_, _, messages, mut nodes) = run_keygen(
        new_nodes(ACCOUNT_IDS.clone()),
        ceremony_id,
        KeygenOptions::allowing_high_pubkey(),
    )
    .await;

    let node = nodes.get_mut(&ACCOUNT_IDS[0]).unwrap();

    assert_eq!(node.ceremony_manager.get_keygen_states_len(), 0);

    // Receive a comm1 with a used ceremony id (same default keygen ceremony id)
    node.ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[1].clone(),
        ceremony_id,
        messages.stage_3_messages[&ACCOUNT_IDS[1]][&ACCOUNT_IDS[0]]
            .clone()
            .into(),
    );

    // The message should have been ignored and no ceremony was started
    // In this case, the ceremony would be unauthorised, so we must check how many keygen states exist
    // to see if a unauthorised state was created.
    assert_eq!(node.ceremony_manager.get_keygen_states_len(), 0);
}

#[tokio::test]
async fn should_not_consume_ceremony_id_if_unauthorised() {
    let mut ceremony = KeygenCeremonyRunner::new(
        new_nodes(ACCOUNT_IDS.clone()),
        1,
        KeygenOptions::allowing_high_pubkey(),
        Rng::from_seed([4; 32]),
    );

    {
        let [test_id, sender_id] = ceremony.select_account_ids();

        assert_eq!(
            ceremony.nodes[&test_id]
                .ceremony_manager
                .get_keygen_states_len(),
            0
        );

        // Receive comm1 with the default keygen ceremony id
        ceremony.distribute_message(
            &sender_id,
            &test_id,
            gen_invalid_keygen_comm1(&mut Rng::from_entropy()),
        );

        // Check that the unauthorised ceremony was created
        assert_eq!(
            ceremony.nodes[&test_id]
                .ceremony_manager
                .get_keygen_states_len(),
            1
        );

        // Timeout the unauthorised ceremony
        ceremony.get_mut_node(&test_id).force_stage_timeout();
    }

    standard_keygen(ceremony).await;
}

mod timeout {

    use super::*;

    // What should be tested w.r.t timeouts:

    // 1. [todo] If timeout during a broadcast verification stage, and we have enough data, we can recover
    // TODO: more test cases

    mod during_broadcast_verification_stage {

        use crate::multisig::client::{
            keygen::{Complaints4, VerifyComm2, VerifyComplaints5},
            tests::helpers::KeygenCeremonyRunner,
        };

        use super::*;

        #[tokio::test]
        async fn recover_if_agree_on_values_stage2() {
            let mut ceremony = KeygenCeremonyRunner::new(
                new_nodes(ACCOUNT_IDS.iter().cloned()),
                1,
                KeygenOptions::allowing_high_pubkey(),
                Rng::from_seed([8; 32]),
            );

            let (messages, result_receivers) = ceremony.request().await;

            let messages = helpers::run_stages!(ceremony, messages, VerifyComm2,);

            let messages = ceremony
                .run_stage_with_non_sender::<SecretShare3, _, _>(messages, &ACCOUNT_IDS[0].clone())
                .await;

            let messages = helpers::run_stages!(ceremony, messages, Complaints4, VerifyComplaints5);

            ceremony.distribute_messages(messages);
            ceremony.complete(result_receivers).await;
        }
    }
}
