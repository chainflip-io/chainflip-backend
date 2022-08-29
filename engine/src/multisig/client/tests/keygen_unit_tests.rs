use cf_traits::AuthorityCount;
use rand_legacy::FromEntropy;
use std::collections::BTreeSet;

use crate::multisig::{
    client::{
        common::{
            BroadcastFailureReason, CeremonyFailureReason, CeremonyStageName, KeygenFailureReason,
        },
        keygen::{
            self, generate_key_data_until_compatible, Complaints6, VerifyComplaints7,
            VerifyHashComm2,
        },
        tests::helpers::{
            all_stages_with_single_invalid_share_keygen_coroutine, for_each_stage,
            gen_invalid_keygen_comm1, get_invalid_hash_comm, get_keygen_stage_name_from_number,
            new_nodes, run_keygen, run_stages, split_messages_for, standard_keygen,
            KeygenCeremonyRunner,
        },
        utils::PartyIdxMapping,
    },
    crypto::Rng,
};

use super::*;

use crate::multisig::crypto::eth::Point;
type CoeffComm3 = keygen::CoeffComm3<Point>;
type VerifyCoeffComm4 = keygen::VerifyCoeffComm4<Point>;
type SecretShare5 = keygen::SecretShare5<Point>;
type BlameResponse8 = keygen::BlameResponse8<Point>;
type VerifyBlameResponses9 = keygen::VerifyBlameResponses9<Point>;
type KeygenData = keygen::KeygenData<Point>;

/// If all nodes are honest and behave as expected we should
/// generate a key without entering a blaming stage
#[tokio::test]
async fn happy_path_results_in_valid_key() {
    let (_, _, _, _) = run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
}

#[tokio::test]
async fn should_delay_comm1_before_keygen_request() {
    let (_, _, messages, _nodes) = standard_keygen(KeygenCeremonyRunner::new_with_default()).await;

    let mut ceremony = KeygenCeremonyRunner::new_with_default();
    let [test_id, late_id] = ceremony.select_account_ids();

    let (late_msg, early_msgs) =
        split_messages_for(messages.stage_1a_messages.clone(), &test_id, &late_id);

    ceremony.distribute_messages(early_msgs).await;

    assert_eq!(
        ceremony.nodes[&test_id].ceremony_runner.get_stage_name(),
        None
    );

    ceremony.request().await;

    assert_eq!(
        ceremony.nodes[&test_id].ceremony_runner.get_stage_name(),
        Some(CeremonyStageName::HashCommitments1),
    );

    ceremony.distribute_messages(late_msg).await;

    assert_eq!(
        ceremony.nodes[&test_id].ceremony_runner.get_stage_name(),
        Some(CeremonyStageName::VerifyHashCommitmentsBroadcast2),
    );
}

// Data for any stage that arrives one stage too early should be properly delayed
// and processed after the stage transition is made
#[tokio::test]
async fn should_delay_stage_data() {
    for_each_stage(
        1..KEYGEN_STAGES,
        || Box::pin(async { KeygenCeremonyRunner::new_with_default() }),
        all_stages_with_single_invalid_share_keygen_coroutine,
        |stage_number, mut ceremony, (_key_id, messages, _type_messages)| async move {
            let target_account_id = &ACCOUNT_IDS[0];
            let late_account_id = ACCOUNT_IDS[3].clone();
            let (late_messages, early_messages) = split_messages_for(
                messages[stage_number - 1].clone(),
                target_account_id,
                &late_account_id,
            );
            ceremony.distribute_messages(early_messages).await;

            let (late_messages_next, early_messages) = split_messages_for(
                messages[stage_number].clone(),
                target_account_id,
                &late_account_id,
            );
            ceremony.distribute_messages(early_messages).await;

            assert_eq!(
                ceremony.nodes[target_account_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_keygen_stage_name_from_number(stage_number)
            );

            ceremony.distribute_messages(late_messages).await;

            assert_eq!(
                ceremony.nodes[target_account_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_keygen_stage_name_from_number(stage_number + 1)
            );

            ceremony.distribute_messages(late_messages_next).await;

            // Check that the stage correctly advanced or finished
            assert_eq!(
                ceremony.nodes[target_account_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_keygen_stage_name_from_number(stage_number + 2)
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
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let messages = ceremony.request().await;

    let mut messages = run_stages!(
        ceremony,
        messages,
        keygen::VerifyHashComm2,
        CoeffComm3,
        VerifyCoeffComm4,
        SecretShare5
    );

    // One party sends another a bad secret share to cause entering the blaming stage
    let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
    *messages
        .get_mut(bad_share_sender_id)
        .unwrap()
        .get_mut(bad_share_receiver_id)
        .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

    let messages = run_stages!(
        ceremony,
        messages,
        keygen::Complaints6,
        keygen::VerifyComplaints7,
        BlameResponse8,
        VerifyBlameResponses9
    );
    ceremony.distribute_messages(messages).await;
    ceremony.complete().await;
}

#[tokio::test]
async fn should_enter_blaming_stage_on_timeout_secret_shares() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let messages = ceremony.request().await;

    let mut messages = run_stages!(
        ceremony,
        messages,
        keygen::VerifyHashComm2,
        CoeffComm3,
        VerifyCoeffComm4,
        SecretShare5
    );

    // One party fails to send a secret share to another causing everyone to later enter the blaming stage
    let [non_sending_party_id, timed_out_party_id] = &ceremony.select_account_ids();
    messages
        .get_mut(non_sending_party_id)
        .unwrap()
        .remove(timed_out_party_id);

    ceremony.distribute_messages(messages).await;

    // This node doesn't receive non_sending_party_id's message, so must timeout
    ceremony
        .get_mut_node(timed_out_party_id)
        .force_stage_timeout()
        .await;

    let messages = ceremony
        .gather_outgoing_messages::<Complaints6, KeygenData>()
        .await;

    let messages = run_stages!(
        ceremony,
        messages,
        VerifyComplaints7,
        BlameResponse8,
        VerifyBlameResponses9
    );
    ceremony.distribute_messages(messages).await;
    ceremony.complete().await;
}

/// If one or more parties send an invalid secret share both the first
/// time and during the blaming stage, the ceremony is aborted with these
/// parties reported
#[tokio::test]
async fn should_report_on_invalid_blame_response6() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();
    let party_idx_mapping =
        PartyIdxMapping::from_participants(BTreeSet::from_iter(ceremony.nodes.keys().cloned()));
    let [bad_node_id_1, bad_node_id_2, target_node_id] = ceremony.select_account_ids();

    let messages = ceremony.request().await;

    let mut messages = run_stages!(
        ceremony,
        messages,
        VerifyHashComm2,
        CoeffComm3,
        VerifyCoeffComm4,
        SecretShare5
    );

    // bad_node_id_1 and bad_node_id_2 send a bad secret share
    *messages
        .get_mut(&bad_node_id_1)
        .unwrap()
        .get_mut(&target_node_id)
        .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

    *messages
        .get_mut(&bad_node_id_2)
        .unwrap()
        .get_mut(&target_node_id)
        .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

    let mut messages = run_stages!(
        ceremony,
        messages,
        Complaints6,
        VerifyComplaints7,
        BlameResponse8
    );

    // bad_node_id_1 also sends a bad blame responses, and so gets blamed when ceremony finished
    let secret_share = SecretShare5::create_random(&mut ceremony.rng);
    for message in messages.get_mut(&bad_node_id_1).unwrap().values_mut() {
        *message = keygen::BlameResponse8(
            std::iter::once((
                party_idx_mapping.get_idx(&bad_node_id_2).unwrap(),
                secret_share.clone(),
            ))
            .collect(),
        )
    }

    let messages = ceremony
        .run_stage::<VerifyBlameResponses9, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages).await;
    ceremony
        .complete_with_error(
            &[bad_node_id_1.clone()],
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidBlameResponse),
        )
        .await;
}

/// If party is blamed by one or more peers, its BlameResponse sent in
/// the next stage must be complete, that is, it must contain a (valid)
/// entry for *every* peer it is blamed by. Otherwise the blamed party
/// get reported.
#[tokio::test]
async fn should_report_on_incomplete_blame_response() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let [bad_node_id_1, target_node_id] = ceremony.select_account_ids();

    let messages = ceremony.request().await;

    let mut messages = run_stages!(
        ceremony,
        messages,
        VerifyHashComm2,
        CoeffComm3,
        VerifyCoeffComm4,
        SecretShare5
    );

    // bad_node_id_1 sends a bad secret share
    *messages
        .get_mut(&bad_node_id_1)
        .unwrap()
        .get_mut(&target_node_id)
        .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

    let mut messages = run_stages!(
        ceremony,
        messages,
        Complaints6,
        VerifyComplaints7,
        BlameResponse8
    );

    // bad_node_id_1 sends an empty BlameResponse
    for message in messages.get_mut(&bad_node_id_1).unwrap().values_mut() {
        *message = keygen::BlameResponse8::<Point>(std::collections::BTreeMap::default())
    }

    let messages = ceremony
        .run_stage::<VerifyBlameResponses9, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages).await;
    ceremony
        .complete_with_error(
            &[bad_node_id_1.clone()],
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidBlameResponse),
        )
        .await;
}

// Ignore unexpected messages at all stages. This includes:
// - Messages with stage data that is not the current stage or the next stage
// - Duplicate messages from the same sender AccountId
// - Messages from unknown AccountId (not in the keygen ceremony)
#[tokio::test]
async fn should_ignore_unexpected_message_for_stage() {
    for_each_stage(
        1..=KEYGEN_STAGES,
        || Box::pin(async { KeygenCeremonyRunner::new_with_default() }),
        all_stages_with_single_invalid_share_keygen_coroutine,
        |stage_number, mut ceremony, (_key_id, messages, _type_messages)| async move {
            let [target_account_id, unexpected_message_sender] = &ceremony.select_account_ids();
            let (msg_from_1, other_msgs) = split_messages_for(
                messages[stage_number - 1].clone(),
                target_account_id,
                unexpected_message_sender,
            );

            ceremony.distribute_messages(other_msgs.clone()).await;

            for ignored_stage_index in (0..stage_number - 1).chain(stage_number + 1..KEYGEN_STAGES)
            {
                let (msg_from_1, _) = split_messages_for(
                    messages[ignored_stage_index].clone(),
                    target_account_id,
                    unexpected_message_sender,
                );
                ceremony.distribute_messages(msg_from_1).await;
            }

            assert_eq!(
                ceremony.nodes[target_account_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_keygen_stage_name_from_number(stage_number),
                "Failed to ignore a message from an unexpected stage"
            );

            ceremony.distribute_messages(other_msgs).await;

            assert_eq!(
                ceremony.nodes[target_account_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_keygen_stage_name_from_number(stage_number),
                "Failed to ignore duplicate messages"
            );

            let unknown_id = AccountId::new([0; 32]);
            assert!(!ACCOUNT_IDS.contains(&unknown_id));
            ceremony
                .distribute_messages(
                    msg_from_1
                        .iter()
                        .map(|(_, message)| (unknown_id.clone(), message.clone()))
                        .collect(),
                )
                .await;
            assert_eq!(
                ceremony.nodes[target_account_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_keygen_stage_name_from_number(stage_number),
                "Failed to ignore a message from an unknown account id"
            );

            ceremony.distribute_messages(msg_from_1).await;

            assert_eq!(
                ceremony.nodes[target_account_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_keygen_stage_name_from_number(stage_number + 1),
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
async fn should_report_on_inconsistent_broadcast_comm1() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let messages = ceremony.request().await;
    let mut messages = helpers::run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

    let [bad_account_id] = &ceremony.select_account_ids();

    // Make one of the nodes send a different commitment to half of the others
    // Note: the bad node must send different comm1 to more than 1/3 of the participants
    let commitment =
        gen_invalid_keygen_comm1(&mut ceremony.rng, ACCOUNT_IDS.len() as AuthorityCount);
    for message in messages
        .get_mut(bad_account_id)
        .unwrap()
        .values_mut()
        .step_by(2)
    {
        *message = commitment.clone();
    }

    let messages = ceremony.run_stage::<VerifyCoeffComm4, _, _>(messages).await;
    ceremony.distribute_messages(messages).await;
    ceremony
        .complete_with_error(
            &[bad_account_id.clone()],
            CeremonyFailureReason::BroadcastFailure(
                BroadcastFailureReason::Inconsistency,
                CeremonyStageName::VerifyCommitmentsBroadcast4,
            ),
        )
        .await;
}

#[tokio::test]
async fn should_report_on_inconsistent_broadcast_hash_comm1a() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let mut messages = ceremony.request().await;

    let bad_account_id = &ACCOUNT_IDS[1];

    // Make one of the nodes send a different hash commitment to half of the others
    // Note: the bad node must send different values to more than 1/3 of the participants
    let hash_comm = get_invalid_hash_comm(&mut ceremony.rng);
    for message in messages
        .get_mut(bad_account_id)
        .unwrap()
        .values_mut()
        .step_by(2)
    {
        *message = hash_comm.clone();
    }

    let messages = helpers::run_stages!(ceremony, messages, VerifyHashComm2,);

    ceremony.distribute_messages(messages).await;
    ceremony
        .complete_with_error(
            &[bad_account_id.clone()],
            CeremonyFailureReason::BroadcastFailure(
                BroadcastFailureReason::Inconsistency,
                CeremonyStageName::VerifyHashCommitmentsBroadcast2,
            ),
        )
        .await;
}

// If one or more parties reveal invalid coefficients that don't correspond
// to the hash commitments sent earlier, the ceremony should be aborted with
// those parties reported.
#[tokio::test]
async fn should_report_on_invalid_hash_comm1a() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let messages = ceremony.request().await;
    let mut messages = helpers::run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

    let [bad_account_id] = ceremony.select_account_ids();

    // Make a node send a bad commitment to the others
    // Note: we must send the same bad commitment to all of the nodes,
    // or we will fail on the `inconsistent` error instead of the validation error.
    let corrupted_message = {
        let mut original_message = messages
            .get(&bad_account_id)
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone();
        original_message.corrupt_secondary_coefficient(&mut ceremony.rng);
        original_message
    };
    for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
        *message = corrupted_message.clone();
    }

    let messages = ceremony.run_stage::<VerifyCoeffComm4, _, _>(messages).await;
    ceremony.distribute_messages(messages).await;

    ceremony
        .complete_with_error(
            &[bad_account_id],
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidCommitment),
        )
        .await;
}

#[tokio::test]
async fn should_report_on_inconsistent_broadcast_complaints4() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let messages = ceremony.request().await;

    let mut messages = run_stages!(
        ceremony,
        messages,
        VerifyHashComm2,
        CoeffComm3,
        VerifyCoeffComm4,
        SecretShare5,
        Complaints6
    );

    let [bad_account_id] = &ceremony.select_account_ids();

    // Make one of the nodes send 2 different complaints evenly to the others
    // Note: the bad node must send different complaints to more than 1/3 of the participants
    for (counter, message) in messages
        .get_mut(bad_account_id)
        .unwrap()
        .values_mut()
        .enumerate()
    {
        let counter = counter as AuthorityCount;
        *message = Complaints6(BTreeSet::from_iter(
            counter % 2..((counter % 2) + ACCOUNT_IDS.len() as AuthorityCount),
        ));
    }

    let messages = ceremony
        .run_stage::<keygen::VerifyComplaints7, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages).await;
    ceremony
        .complete_with_error(
            &[bad_account_id.clone()],
            CeremonyFailureReason::BroadcastFailure(
                BroadcastFailureReason::Inconsistency,
                CeremonyStageName::VerifyComplaintsBroadcastStage7,
            ),
        )
        .await;
}

#[tokio::test]
async fn should_report_on_inconsistent_broadcast_blame_responses6() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let party_idx_mapping =
        PartyIdxMapping::from_participants(BTreeSet::from_iter(ceremony.nodes.keys().cloned()));

    let messages = ceremony.request().await;

    let mut messages = run_stages!(
        ceremony,
        messages,
        VerifyHashComm2,
        CoeffComm3,
        VerifyCoeffComm4,
        SecretShare5
    );

    let [bad_node_id, blamed_node_id] = &ceremony.select_account_ids();

    // One party sends another a bad secret share to cause entering the blaming stage
    let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
    *messages
        .get_mut(bad_share_sender_id)
        .unwrap()
        .get_mut(bad_share_receiver_id)
        .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

    let mut messages = run_stages!(
        ceremony,
        messages,
        Complaints6,
        VerifyComplaints7,
        BlameResponse8
    );

    let [bad_account_id] = &ceremony.select_account_ids();

    // Make one of the nodes send 2 different blame responses evenly to the others
    // Note: the bad node must send different blame response to more than 1/3 of the participants
    let secret_share = SecretShare5::create_random(&mut ceremony.rng);
    for message in messages
        .get_mut(bad_node_id)
        .unwrap()
        .values_mut()
        .step_by(2)
    {
        *message = keygen::BlameResponse8::<Point>(
            std::iter::once((
                party_idx_mapping.get_idx(blamed_node_id).unwrap(),
                secret_share.clone(),
            ))
            .collect(),
        )
    }

    let messages = ceremony
        .run_stage::<VerifyBlameResponses9, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages).await;
    ceremony
        .complete_with_error(
            &[bad_account_id.clone()],
            CeremonyFailureReason::BroadcastFailure(
                BroadcastFailureReason::Inconsistency,
                CeremonyStageName::VerifyBlameResponsesBroadcastStage9,
            ),
        )
        .await;
}

// If one or more parties send invalid commitments, the ceremony should be aborted.
// Fail on `validate_commitments` during `VerifyCommitmentsBroadcast2`.
#[tokio::test]
async fn should_report_on_invalid_comm1() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let messages = ceremony.request().await;
    let mut messages = helpers::run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

    let [bad_account_id] = ceremony.select_account_ids();

    // Make a node send a bad commitment to the others
    // Note: we must send the same bad commitment to all of the nodes,
    // or we will fail on the `inconsistent` error instead of the validation error.
    let corrupted_message = {
        let mut original_message = messages
            .get(&bad_account_id)
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone();
        original_message.corrupt_primary_coefficient(&mut ceremony.rng);
        original_message
    };
    for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
        *message = corrupted_message.clone();
    }

    let messages = ceremony.run_stage::<VerifyCoeffComm4, _, _>(messages).await;
    ceremony.distribute_messages(messages).await;

    ceremony
        .complete_with_error(
            &[bad_account_id],
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidCommitment),
        )
        .await;
}

#[tokio::test]
async fn should_report_on_invalid_complaints4() {
    let mut ceremony = KeygenCeremonyRunner::new_with_default();

    let messages = ceremony.request().await;

    let mut messages = run_stages!(
        ceremony,
        messages,
        VerifyHashComm2,
        CoeffComm3,
        VerifyCoeffComm4,
        SecretShare5,
        Complaints6
    );

    let [bad_account_id] = ceremony.select_account_ids();

    // This complaint is invalid because it has an invalid index
    let invalid_complaint: Complaints6 = keygen::Complaints6([1, u32::MAX].into_iter().collect());

    for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
        *message = invalid_complaint.clone();
    }

    let messages = ceremony
        .run_stage::<keygen::VerifyComplaints7, _, _>(messages)
        .await;
    ceremony.distribute_messages(messages).await;
    ceremony
        .complete_with_error(
            &[bad_account_id],
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidComplaint),
        )
        .await;
}

// Keygen aborts if the key is not compatible with the contract at VerifyCommitmentsBroadcast2
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

mod timeout {

    use super::*;

    use crate::multisig::client::tests::helpers::KeygenCeremonyRunner;

    mod during_regular_stage {

        use super::*;

        #[tokio::test]
        async fn should_recover_if_party_appears_offline_to_minority_stage1a() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let mut messages = ceremony.request().await;

            let [non_sending_party_id, timed_out_party_id] = ceremony.select_account_ids();

            messages
                .get_mut(&non_sending_party_id)
                .unwrap()
                .remove(&timed_out_party_id);

            ceremony.distribute_messages(messages).await;

            // This node doesn't receive non_sending_party's message, so must timeout
            ceremony
                .get_mut_node(&timed_out_party_id)
                .force_stage_timeout()
                .await;

            let messages = ceremony
                .gather_outgoing_messages::<VerifyHashComm2, KeygenData>()
                .await;

            let messages = run_stages!(
                ceremony,
                messages,
                CoeffComm3,
                VerifyCoeffComm4,
                SecretShare5,
                Complaints6,
                VerifyComplaints7
            );
            ceremony.distribute_messages(messages).await;
            ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_recover_if_party_appears_offline_to_minority_stage1() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let mut messages = run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

            let [non_sending_party_id, timed_out_party_id] = ceremony.select_account_ids();

            messages
                .get_mut(&non_sending_party_id)
                .unwrap()
                .remove(&timed_out_party_id);

            ceremony.distribute_messages(messages).await;

            // This node doesn't receive non_sending_party's message, so must timeout
            ceremony
                .get_mut_node(&timed_out_party_id)
                .force_stage_timeout()
                .await;

            let messages = ceremony
                .gather_outgoing_messages::<VerifyCoeffComm4, KeygenData>()
                .await;

            let messages = run_stages!(
                ceremony,
                messages,
                SecretShare5,
                Complaints6,
                VerifyComplaints7
            );
            ceremony.distribute_messages(messages).await;
            ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_recover_if_party_appears_offline_to_minority_stage4() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let mut messages = run_stages!(
                ceremony,
                messages,
                VerifyHashComm2,
                CoeffComm3,
                VerifyCoeffComm4,
                SecretShare5,
                Complaints6
            );

            let [non_sending_party_id, timed_out_party_id] = ceremony.select_account_ids();

            messages
                .get_mut(&non_sending_party_id)
                .unwrap()
                .remove(&timed_out_party_id);

            ceremony.distribute_messages(messages).await;

            // This node doesn't receive non_sending_party's message, so must timeout
            ceremony
                .get_mut_node(&timed_out_party_id)
                .force_stage_timeout()
                .await;

            let messages = ceremony
                .gather_outgoing_messages::<VerifyComplaints7, KeygenData>()
                .await;

            ceremony.distribute_messages(messages).await;
            ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_recover_if_party_appears_offline_to_minority_stage6() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let mut messages = run_stages!(
                ceremony,
                messages,
                VerifyHashComm2,
                CoeffComm3,
                VerifyCoeffComm4,
                SecretShare5
            );

            // One party sends another a bad secret share to cause entering the blaming stage
            let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
            *messages
                .get_mut(bad_share_sender_id)
                .unwrap()
                .get_mut(bad_share_receiver_id)
                .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

            let [non_sending_party_id, timed_out_party_id] = ceremony.select_account_ids();

            let mut messages = run_stages!(
                ceremony,
                messages,
                Complaints6,
                VerifyComplaints7,
                BlameResponse8
            );

            messages
                .get_mut(&non_sending_party_id)
                .unwrap()
                .remove(&timed_out_party_id);

            ceremony.distribute_messages(messages).await;

            // This node doesn't receive non_sending_party's message, so must timeout
            ceremony
                .get_mut_node(&timed_out_party_id)
                .force_stage_timeout()
                .await;

            let messages = ceremony
                .gather_outgoing_messages::<VerifyBlameResponses9, KeygenData>()
                .await;

            ceremony.distribute_messages(messages).await;
            ceremony.complete().await;
        }
    }

    mod during_broadcast_verification_stage {

        use super::*;

        #[tokio::test]
        async fn should_recover_if_agree_on_values_stage2a() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let messages = run_stages!(ceremony, messages, VerifyHashComm2,);

            let [non_sender_id] = &ceremony.select_account_ids();
            let messages = ceremony
                .run_stage_with_non_sender::<CoeffComm3, _, _>(messages, non_sender_id)
                .await;

            let messages = run_stages!(
                ceremony,
                messages,
                VerifyCoeffComm4,
                SecretShare5,
                Complaints6,
                VerifyComplaints7
            );

            ceremony.distribute_messages(messages).await;
            ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_recover_if_agree_on_values_stage2() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let messages = run_stages!(
                ceremony,
                messages,
                VerifyHashComm2,
                CoeffComm3,
                VerifyCoeffComm4
            );

            let [non_sender_id] = &ceremony.select_account_ids();
            let messages = ceremony
                .run_stage_with_non_sender::<SecretShare5, _, _>(messages, non_sender_id)
                .await;

            let messages = run_stages!(ceremony, messages, Complaints6, VerifyComplaints7);

            ceremony.distribute_messages(messages).await;
            ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_recover_if_agree_on_values_stage5() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let messages = run_stages!(
                ceremony,
                messages,
                VerifyHashComm2,
                CoeffComm3,
                VerifyCoeffComm4,
                SecretShare5,
                Complaints6,
                VerifyComplaints7
            );

            let [non_sender_id] = ceremony.select_account_ids();
            ceremony
                .distribute_messages_with_non_sender(messages, &non_sender_id)
                .await;

            ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_recover_if_agree_on_values_stage7() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let mut messages = run_stages!(
                ceremony,
                messages,
                VerifyHashComm2,
                CoeffComm3,
                VerifyCoeffComm4,
                SecretShare5
            );

            // One party sends another a bad secret share to cause entering the blaming stage
            let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
            *messages
                .get_mut(bad_share_sender_id)
                .unwrap()
                .get_mut(bad_share_receiver_id)
                .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

            let messages = run_stages!(
                ceremony,
                messages,
                Complaints6,
                VerifyComplaints7,
                BlameResponse8,
                VerifyBlameResponses9
            );

            let [non_sender_id] = ceremony.select_account_ids();
            ceremony
                .distribute_messages_with_non_sender(messages, &non_sender_id)
                .await;

            ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_report_if_insufficient_messages_stage2a() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let [non_sending_party_id_1, non_sending_party_id_2] = ceremony.select_account_ids();

            // bad party 1 times out during a broadcast stage. It should be reported
            let messages = ceremony
                .run_stage_with_non_sender::<VerifyHashComm2, _, _>(
                    messages,
                    &non_sending_party_id_1,
                )
                .await;

            // bad party 2 times out during a broadcast verification stage. It won't get reported.
            ceremony
                .distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
                .await;

            ceremony
                .complete_with_error(
                    &[non_sending_party_id_1],
                    CeremonyFailureReason::BroadcastFailure(
                        BroadcastFailureReason::InsufficientMessages,
                        CeremonyStageName::VerifyHashCommitmentsBroadcast2,
                    ),
                )
                .await
        }

        #[tokio::test]
        async fn should_report_if_insufficient_messages_stage2() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let [non_sending_party_id_1, non_sending_party_id_2] = ceremony.select_account_ids();

            let messages = run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

            // bad party 1 times out during a broadcast stage. It should be reported
            let messages = ceremony
                .run_stage_with_non_sender::<VerifyCoeffComm4, _, _>(
                    messages,
                    &non_sending_party_id_1,
                )
                .await;

            // bad party 2 times out during a broadcast verification stage. It won't get reported.
            ceremony
                .distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
                .await;

            ceremony
                .complete_with_error(
                    &[non_sending_party_id_1],
                    CeremonyFailureReason::BroadcastFailure(
                        BroadcastFailureReason::InsufficientMessages,
                        CeremonyStageName::VerifyCommitmentsBroadcast4,
                    ),
                )
                .await
        }

        #[tokio::test]
        async fn should_report_if_insufficient_messages_stage5() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let messages = run_stages!(
                ceremony,
                messages,
                VerifyHashComm2,
                CoeffComm3,
                VerifyCoeffComm4,
                SecretShare5,
                Complaints6
            );

            let [non_sending_party_id_1, non_sending_party_id_2] = ceremony.select_account_ids();

            // bad party 1 times out during a broadcast stage. It should be reported
            let messages = ceremony
                .run_stage_with_non_sender::<VerifyComplaints7, _, _>(
                    messages,
                    &non_sending_party_id_1,
                )
                .await;

            // bad party 2 times out during a broadcast verification stage. It won't get reported.
            ceremony
                .distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
                .await;

            ceremony
                .complete_with_error(
                    &[non_sending_party_id_1],
                    CeremonyFailureReason::BroadcastFailure(
                        BroadcastFailureReason::InsufficientMessages,
                        CeremonyStageName::VerifyComplaintsBroadcastStage7,
                    ),
                )
                .await
        }

        #[tokio::test]
        async fn should_report_if_insufficient_messages_stage7() {
            let mut ceremony = KeygenCeremonyRunner::new_with_default();

            let messages = ceremony.request().await;

            let mut messages = run_stages!(
                ceremony,
                messages,
                VerifyHashComm2,
                CoeffComm3,
                VerifyCoeffComm4,
                SecretShare5
            );

            // One party sends another a bad secret share to cause entering the blaming stage
            let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
            *messages
                .get_mut(bad_share_sender_id)
                .unwrap()
                .get_mut(bad_share_receiver_id)
                .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

            let messages = run_stages!(
                ceremony,
                messages,
                Complaints6,
                VerifyComplaints7,
                BlameResponse8
            );

            let [non_sending_party_id_1, non_sending_party_id_2] = ceremony.select_account_ids();

            // bad party 1 times out during a broadcast stage. It should be reported
            let messages = ceremony
                .run_stage_with_non_sender::<VerifyBlameResponses9, _, _>(
                    messages,
                    &non_sending_party_id_1,
                )
                .await;

            // bad party 2 times out during a broadcast verification stage. It won't get reported.
            ceremony
                .distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
                .await;

            ceremony
                .complete_with_error(
                    &[non_sending_party_id_1],
                    CeremonyFailureReason::BroadcastFailure(
                        BroadcastFailureReason::InsufficientMessages,
                        CeremonyStageName::VerifyBlameResponsesBroadcastStage9,
                    ),
                )
                .await
        }
    }
}

#[tokio::test]
async fn genesis_keys_can_sign() {
    use crate::multisig::crypto::eth::Point;
    use crate::multisig::tests::fixtures::MESSAGE_HASH;

    let account_ids: BTreeSet<_> = [1, 2, 3, 4]
        .iter()
        .map(|i| AccountId::new([*i; 32]))
        .collect();

    let rng = Rng::from_entropy();
    let (key_id, key_data) =
        generate_key_data_until_compatible::<Point>(account_ids.clone(), 20, rng);

    let (mut signing_ceremony, _non_signing_nodes) =
        SigningCeremonyRunner::new_with_threshold_subset_of_signers(
            new_nodes(account_ids),
            1,
            key_id.clone(),
            key_data.clone(),
            MESSAGE_HASH.clone(),
            Rng::from_entropy(),
        );
    standard_signing(&mut signing_ceremony).await;
}
