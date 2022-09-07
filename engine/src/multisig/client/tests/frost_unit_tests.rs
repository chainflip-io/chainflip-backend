#![cfg(test)]

use std::collections::BTreeSet;

use rand_legacy::SeedableRng;

use crate::multisig::{
    client::{
        common::{
            BroadcastFailureReason, CeremonyFailureReason, CeremonyStageName, SigningFailureReason,
        },
        keygen::generate_key_data,
        signing::frost,
        tests::helpers::{
            for_each_stage, gen_invalid_local_sig, gen_invalid_signing_comm1,
            get_signing_stage_name_from_number, new_signing_ceremony_with_keygen, run_stages,
            split_messages_for, standard_signing, standard_signing_coroutine,
        },
    },
    tests::fixtures::MESSAGE_HASH,
    Rng,
};

use super::*;

// Data for any stage that arrives one stage too early should be properly delayed
// and processed after the stage transition is made
#[tokio::test]
async fn should_delay_stage_data() {
    for_each_stage(
        1..SIGNING_STAGES,
        || Box::pin(async { new_signing_ceremony_with_keygen().await.0 }),
        standard_signing_coroutine,
        |stage_number, mut ceremony, (_, messages, _)| async move {
            let [late_sender, test_account] = ceremony.select_account_ids();

            let get_messages_for_stage = |stage_index: usize| {
                split_messages_for(messages[stage_index].clone(), &test_account, &late_sender)
            };

            // Receive the data of this stage and the next stage from all but one client
            let (late_msg, msgs) = get_messages_for_stage(stage_number - 1);
            ceremony.distribute_messages(msgs).await;
            let (next_late_msg, next_msgs) = get_messages_for_stage(stage_number);
            ceremony.distribute_messages(next_msgs).await;

            assert_eq!(
                ceremony.nodes[&test_account]
                    .ceremony_runner
                    .get_stage_name(),
                get_signing_stage_name_from_number(stage_number),
            );

            // Now receive the final client's data to advance the stage
            ceremony.distribute_messages(late_msg).await;

            assert_eq!(
                ceremony.nodes[&test_account]
                    .ceremony_runner
                    .get_stage_name(),
                get_signing_stage_name_from_number(stage_number + 1),
            );

            ceremony.distribute_messages(next_late_msg).await;

            // Check that the stage correctly advanced or finished
            assert_eq!(
                ceremony.nodes[&test_account]
                    .ceremony_runner
                    .get_stage_name(),
                get_signing_stage_name_from_number(stage_number + 2),
            );
        },
    )
    .await;
}

// If any initial commitments arrive before the request to sign,
// they should be delayed and processed after it arrives
#[tokio::test]
async fn should_delay_comm1_before_rts() {
    let mut signing_ceremony = new_signing_ceremony_with_keygen().await.0;
    let (_, signing_messages) = standard_signing(&mut signing_ceremony).await;

    let mut signing_ceremony = new_signing_ceremony_with_keygen().await.0;

    // Send comm1 messages from the other clients
    signing_ceremony
        .distribute_messages(signing_messages.stage_1_messages)
        .await;

    let [test_id] = &signing_ceremony.select_account_ids();
    assert_eq!(
        signing_ceremony.nodes[test_id]
            .ceremony_runner
            .get_stage_name(),
        None
    );

    // Now we get the request to sign (effectively receiving the request from our StateChain)
    signing_ceremony.request().await;

    // It should advance to stage 2 right away if the comm1's were delayed correctly
    assert_eq!(
        signing_ceremony.nodes[test_id]
            .ceremony_runner
            .get_stage_name(),
        Some(CeremonyStageName::VerifyCommitmentsBroadcast2),
    );
}

// We choose (arbitrarily) to use eth crypto for unit tests.
use crate::multisig::crypto::eth::Point;
type VerifyComm2 = frost::VerifyComm2<Point>;
type LocalSig3 = frost::LocalSig3<Point>;
type VerifyLocalSig4 = frost::VerifyLocalSig4<Point>;

#[tokio::test]
async fn should_report_on_invalid_local_sig3() {
    let (mut signing_ceremony, _) = new_signing_ceremony_with_keygen().await;

    let messages = signing_ceremony.request().await;
    let mut messages = run_stages!(signing_ceremony, messages, VerifyComm2, LocalSig3);

    // This account id will send an invalid signature
    let [bad_account_id] = signing_ceremony.select_account_ids();
    let invalid_sig3 = gen_invalid_local_sig(&mut signing_ceremony.rng);
    for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
        *message = invalid_sig3.clone();
    }

    let messages = signing_ceremony
        .run_stage::<VerifyLocalSig4, _, _>(messages)
        .await;
    signing_ceremony.distribute_messages(messages).await;
    signing_ceremony
        .complete_with_error(
            &[bad_account_id],
            CeremonyFailureReason::Other(SigningFailureReason::InvalidSigShare),
        )
        .await;
}

#[tokio::test]
async fn should_report_on_inconsistent_broadcast_comm1() {
    let (mut signing_ceremony, _) = new_signing_ceremony_with_keygen().await;

    let mut messages = signing_ceremony.request().await;

    // This account id will send an invalid signature
    let [bad_account_id] = signing_ceremony.select_account_ids();
    for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
        *message = gen_invalid_signing_comm1(&mut signing_ceremony.rng);
    }

    let messages = signing_ceremony
        .run_stage::<VerifyComm2, _, _>(messages)
        .await;
    signing_ceremony.distribute_messages(messages).await;
    signing_ceremony
        .complete_with_error(
            &[bad_account_id],
            CeremonyFailureReason::BroadcastFailure(
                BroadcastFailureReason::Inconsistency,
                CeremonyStageName::VerifyCommitmentsBroadcast2,
            ),
        )
        .await;
}

#[tokio::test]
async fn should_report_on_inconsistent_broadcast_local_sig3() {
    let (mut signing_ceremony, _) = new_signing_ceremony_with_keygen().await;

    let messages = signing_ceremony.request().await;

    let mut messages = run_stages!(signing_ceremony, messages, VerifyComm2, LocalSig3);

    // This account id will send an invalid signature
    let [bad_account_id] = signing_ceremony.select_account_ids();
    for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
        *message = gen_invalid_local_sig(&mut signing_ceremony.rng);
    }

    let messages = signing_ceremony
        .run_stage::<VerifyLocalSig4, _, _>(messages)
        .await;
    signing_ceremony.distribute_messages(messages).await;
    signing_ceremony
        .complete_with_error(
            &[bad_account_id],
            CeremonyFailureReason::BroadcastFailure(
                BroadcastFailureReason::Inconsistency,
                CeremonyStageName::VerifyLocalSigsBroadcastStage4,
            ),
        )
        .await;
}

// Ignore unexpected messages at all stages. This includes:
// - Messages with stage data that is not the current stage or the next stage
// - Duplicate messages from the same sender AccountId
// - Messages from unknown AccountId or not in the signing ceremony
#[tokio::test]
async fn should_ignore_unexpected_message_for_stage() {
    for_each_stage(
        1..=SIGNING_STAGES,
        || Box::pin(async { new_signing_ceremony_with_keygen().await.0 }),
        standard_signing_coroutine,
        |stage_number, mut ceremony, (_, messages, _)| async move {
            let previous_stage = stage_number - 1;

            let [test_node_id, sender_id] = &ceremony.select_account_ids();

            let get_messages_for_stage = |stage_index: usize| {
                split_messages_for(messages[stage_index].clone(), test_node_id, sender_id)
            };

            // Get the messages from all but one client for the previous stage
            let (msg_from_1, other_msgs) = get_messages_for_stage(previous_stage);
            ceremony.distribute_messages(other_msgs.clone()).await;

            // Receive messages from all unexpected stages (not the current stage or the next)
            for ignored_stage_index in (0..previous_stage).chain(stage_number + 1..SIGNING_STAGES) {
                let (msg_from_1, _) = get_messages_for_stage(ignored_stage_index);
                ceremony.distribute_messages(msg_from_1).await;
            }

            // We should not have progressed further when receiving unexpected messages
            assert_eq!(
                ceremony.nodes[test_node_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_signing_stage_name_from_number(stage_number),
                "Failed to ignore a message from an unexpected stage"
            );

            // Receive a duplicate message
            ceremony.distribute_messages(other_msgs).await;
            assert_eq!(
                ceremony.nodes[test_node_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_signing_stage_name_from_number(stage_number),
                "Failed to ignore a duplicate message"
            );

            // Receive a message from an unknown AccountId
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
                ceremony.nodes[test_node_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_signing_stage_name_from_number(stage_number),
                "Failed to ignore a message from an unknown account id"
            );

            // Receive a message from a node that is not in the signing ceremony
            let non_participant_id = ACCOUNT_IDS
                .iter()
                .find(|account_id| !ceremony.nodes.contains_key(*account_id))
                .unwrap();
            ceremony
                .distribute_messages(
                    msg_from_1
                        .iter()
                        .map(|(_, message)| (non_participant_id.clone(), message.clone()))
                        .collect(),
                )
                .await;
            assert_eq!(
                ceremony.nodes[test_node_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_signing_stage_name_from_number(stage_number),
                "Failed to ignore a message from non-participant account id"
            );

            // Receive the last message and advance the stage
            ceremony.distribute_messages(msg_from_1).await;
            assert_eq!(
                ceremony.nodes[test_node_id]
                    .ceremony_runner
                    .get_stage_name(),
                get_signing_stage_name_from_number(stage_number + 1),
                "Failed to proceed to next stage"
            );
        },
    )
    .await;
}

#[tokio::test]
async fn should_sign_with_all_parties() {
    let (key_id, key_data) = generate_key_data(
        BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
        &mut Rng::from_seed(DEFAULT_KEYGEN_SEED),
        true,
    )
    .expect("Should generate key for test");

    let mut signing_ceremony = SigningCeremonyRunner::new_with_all_signers(
        new_nodes(ACCOUNT_IDS.clone()),
        DEFAULT_SIGNING_CEREMONY_ID,
        key_id,
        key_data,
        MESSAGE_HASH.clone(),
        Rng::from_seed(DEFAULT_SIGNING_SEED),
    );

    let messages = signing_ceremony.request().await;
    let messages = run_stages!(
        signing_ceremony,
        messages,
        VerifyComm2,
        LocalSig3,
        VerifyLocalSig4
    );
    signing_ceremony.distribute_messages(messages).await;
    signing_ceremony.complete().await;
}

mod timeout {

    use super::*;

    mod during_regular_stage {

        type SigningData = crate::multisig::client::signing::frost::SigningData<Point>;

        use super::*;

        // ======================

        // The following tests cover:
        // If timeout during a regular (broadcast) stage, but the majority of nodes can agree on all values,
        // we proceed with the ceremony and use the data received by the majority. If majority of nodes
        // agree on a party timing out in the following broadcast verification stage, the party gets reported

        #[tokio::test]
        async fn should_recover_if_party_appears_offline_to_minority_stage1() {
            let (mut signing_ceremony, _) = new_signing_ceremony_with_keygen().await;

            let mut messages = signing_ceremony.request().await;

            let [non_sending_party_id, timed_out_party_id] = signing_ceremony.select_account_ids();

            messages
                .get_mut(&non_sending_party_id)
                .unwrap()
                .remove(&timed_out_party_id);

            signing_ceremony.distribute_messages(messages).await;

            // This node doesn't receive non_sending_party's message, so must timeout
            signing_ceremony
                .nodes
                .get_mut(&timed_out_party_id)
                .unwrap()
                .force_stage_timeout()
                .await;

            let messages = signing_ceremony
                .gather_outgoing_messages::<VerifyComm2, SigningData>()
                .await;

            let messages = run_stages!(signing_ceremony, messages, LocalSig3, VerifyLocalSig4);
            signing_ceremony.distribute_messages(messages).await;
            signing_ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_recover_if_party_appears_offline_to_minority_stage3() {
            let (mut signing_ceremony, _) = new_signing_ceremony_with_keygen().await;

            let messages = signing_ceremony.request().await;

            let mut messages = run_stages!(signing_ceremony, messages, VerifyComm2, LocalSig3);

            let [non_sending_party_id, timed_out_party_id] = signing_ceremony.select_account_ids();

            messages
                .get_mut(&non_sending_party_id)
                .unwrap()
                .remove(&timed_out_party_id);

            signing_ceremony.distribute_messages(messages).await;

            // This node doesn't receive non_sending_party's message, so must timeout
            signing_ceremony
                .nodes
                .get_mut(&timed_out_party_id)
                .unwrap()
                .force_stage_timeout()
                .await;

            let messages = signing_ceremony
                .gather_outgoing_messages::<VerifyLocalSig4, SigningData>()
                .await;

            signing_ceremony.distribute_messages(messages).await;
            signing_ceremony.complete().await;
        }

        // ======================
    }

    mod during_broadcast_verification_stage {

        use super::*;

        // ======================

        // The following tests cover:
        // If timeout during a broadcast verification stage, and we have enough data, we can recover

        #[tokio::test]
        async fn should_recover_if_agree_on_values_stage2() {
            let (mut ceremony, _) = new_signing_ceremony_with_keygen().await;

            let [bad_node_id] = &ceremony.select_account_ids();

            let messages = ceremony.request().await;
            let messages = ceremony.run_stage::<VerifyComm2, _, _>(messages).await;

            let messages = ceremony
                .run_stage_with_non_sender::<LocalSig3, _, _>(messages, bad_node_id)
                .await;

            let messages = ceremony.run_stage::<VerifyLocalSig4, _, _>(messages).await;
            ceremony.distribute_messages(messages).await;
            ceremony.complete().await;
        }

        #[tokio::test]
        async fn should_recover_if_agree_on_values_stage4() {
            let (mut ceremony, _) = new_signing_ceremony_with_keygen().await;

            let [bad_node_id] = &ceremony.select_account_ids();

            let messages = ceremony.request().await;
            let messages = run_stages!(ceremony, messages, VerifyComm2, LocalSig3, VerifyLocalSig4);

            ceremony
                .distribute_messages_with_non_sender(messages, bad_node_id)
                .await;

            ceremony.complete().await;
        }

        // ======================

        // ======================

        // The following tests cover:
        // Timeout during both the broadcast & broadcast verification stages means that
        // we don't have enough data to recover:
        // The parties that timeout during the broadcast stage will be reported,
        // but the parties the timeout during the verification stage will not
        // because that would need another round of "voting" which can also timeout.

        #[tokio::test]
        async fn should_report_if_insufficient_messages_stage2() {
            let (mut signing_ceremony, _) = new_signing_ceremony_with_keygen().await;

            // bad party 1 will timeout during a broadcast stage. It should be reported
            // bad party 2 will timeout during a broadcast verification stage. It won't get reported.
            let [non_sending_party_id_1, non_sending_party_id_2] =
                signing_ceremony.select_account_ids();

            let messages = signing_ceremony.request().await;

            // bad party 1 times out here
            let messages = signing_ceremony
                .run_stage_with_non_sender::<VerifyComm2, _, _>(messages, &non_sending_party_id_1)
                .await;

            // bad party 2 times out here (NB: They are different parties)
            signing_ceremony
                .distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
                .await;

            signing_ceremony
                .complete_with_error(
                    &[non_sending_party_id_1],
                    CeremonyFailureReason::BroadcastFailure(
                        BroadcastFailureReason::InsufficientMessages,
                        CeremonyStageName::VerifyCommitmentsBroadcast2,
                    ),
                )
                .await
        }

        #[tokio::test]
        async fn should_report_if_insufficient_messages_stage4() {
            let (mut signing_ceremony, _) = new_signing_ceremony_with_keygen().await;

            // bad party 1 will timeout during a broadcast stage. It should be reported
            // bad party 2 will timeout during a broadcast verification stage. It won't get reported.
            let [non_sending_party_id_1, non_sending_party_id_2] =
                signing_ceremony.select_account_ids();

            let messages = signing_ceremony.request().await;

            let messages = run_stages!(signing_ceremony, messages, VerifyComm2, LocalSig3);

            // bad party 1 times out here
            let messages = signing_ceremony
                .run_stage_with_non_sender::<VerifyLocalSig4, _, _>(
                    messages,
                    &non_sending_party_id_1,
                )
                .await;

            // bad party 2 times out here (NB: They are different parties)
            signing_ceremony
                .distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
                .await;

            signing_ceremony
                .complete_with_error(
                    &[non_sending_party_id_1],
                    CeremonyFailureReason::BroadcastFailure(
                        BroadcastFailureReason::InsufficientMessages,
                        CeremonyStageName::VerifyLocalSigsBroadcastStage4,
                    ),
                )
                .await
        }

        // ======================
    }
}
