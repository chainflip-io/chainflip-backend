use super::*;
use crate::{
    constants::CEREMONY_ID_WINDOW,
    logging::test_utils::new_test_logger,
    multisig::{
        client::{
            self,
            ceremony_manager::CeremonyManager,
            common::BroadcastVerificationMessage,
            keygen::KeygenData,
            signing::frost::SigningData,
            tests::helpers::{gen_invalid_signing_comm1, get_invalid_hash_comm},
        },
        crypto::Rng,
        eth::EthSigning,
    },
};
use cf_traits::AuthorityCount;
use rand_legacy::SeedableRng;

#[test]
fn should_ignore_non_first_stage_keygen_data_before_request() {
    let mut rng = Rng::from_seed([0; 32]);

    // Create a new ceremony manager
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        0,
        &new_test_logger(),
    );

    // Process a stage 2 message
    ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[0].clone(),
        DEFAULT_KEYGEN_CEREMONY_ID,
        KeygenData::VerifyHashComm2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len())
                .map(|i| (i as AuthorityCount, Some(get_invalid_hash_comm(&mut rng))))
                .collect(),
        }),
    );

    // Check that the message was ignored and no unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 0);

    // Process a stage 1 message
    ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[0].clone(),
        DEFAULT_KEYGEN_CEREMONY_ID,
        KeygenData::HashComm1(client::keygen::HashComm1(sp_core::H256::default())),
    );

    // Check that the message was not ignored and an unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 1);

    // Process a stage 2 message
    ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[0].clone(),
        DEFAULT_KEYGEN_CEREMONY_ID,
        KeygenData::VerifyHashComm2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len())
                .map(|i| (i as AuthorityCount, Some(get_invalid_hash_comm(&mut rng))))
                .collect(),
        }),
    );

    // Check that the message was ignored and not added to the delayed messages of the unauthorised ceremony.
    // Only 1 first stage message should be in the delayed messages.
    assert_eq!(
        ceremony_manager.get_delayed_keygen_messages_len(&DEFAULT_KEYGEN_CEREMONY_ID),
        1
    )
}

#[test]
fn should_ignore_non_first_stage_signing_data_before_request() {
    let mut rng = Rng::from_seed([0; 32]);

    // Create a new ceremony manager
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        0,
        &new_test_logger(),
    );

    // Process a stage 2 message
    ceremony_manager.process_signing_data(
        ACCOUNT_IDS[0].clone(),
        DEFAULT_KEYGEN_CEREMONY_ID,
        SigningData::BroadcastVerificationStage2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len())
                .map(|i| {
                    (
                        i as AuthorityCount,
                        Some(gen_invalid_signing_comm1(&mut rng)),
                    )
                })
                .collect(),
        }),
    );

    // Check that the message was ignored and no unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_signing_states_len(), 0);

    // Process a stage 1 message
    ceremony_manager.process_signing_data(
        ACCOUNT_IDS[0].clone(),
        DEFAULT_KEYGEN_CEREMONY_ID,
        SigningData::CommStage1(gen_invalid_signing_comm1(&mut rng)),
    );

    // Check that the message was not ignored and an unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_signing_states_len(), 1);

    // Process a stage 2 message
    ceremony_manager.process_signing_data(
        ACCOUNT_IDS[0].clone(),
        DEFAULT_KEYGEN_CEREMONY_ID,
        SigningData::BroadcastVerificationStage2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len())
                .map(|i| {
                    (
                        i as AuthorityCount,
                        Some(gen_invalid_signing_comm1(&mut rng)),
                    )
                })
                .collect(),
        }),
    );

    // Check that the message was ignored and not added to the delayed messages of the unauthorised ceremony.
    // Only 1 first stage message should be in the delayed messages.
    assert_eq!(
        ceremony_manager.get_delayed_signing_messages_len(&DEFAULT_KEYGEN_CEREMONY_ID),
        1
    )
}

#[test]
fn should_not_create_unauthorized_ceremony_with_invalid_ceremony_id() {
    let latest_ceremony_id = 1;

    // Create a new ceremony manager and set the latest_ceremony_id to something larger then 0
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        latest_ceremony_id,
        &new_test_logger(),
    );

    // Process a stage 1 message with a ceremony id that is in the past
    ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[0].clone(),
        (latest_ceremony_id as u64)
            .checked_sub(1)
            .expect("latest_ceremony_id must be larger then 0 for this test"),
        KeygenData::HashComm1(client::keygen::HashComm1(sp_core::H256::default())),
    );

    // Process a stage 1 message with a ceremony id that is too far in the future
    ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[0].clone(),
        latest_ceremony_id + CEREMONY_ID_WINDOW + 1,
        KeygenData::HashComm1(client::keygen::HashComm1(sp_core::H256::default())),
    );

    // Check that the messages were ignored and no unauthorised ceremonies were created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 0);
}

#[test]
fn should_track_ceremony_id_on_ceremony_request() {
    // TODO: This test does not cover `on_request_to_sign`, because the task
    // `Simplify ceremony manager unit tests #1731` will factor out the need to do so.

    let test_ceremony_id = 2;
    let test_account_id = ACCOUNT_IDS[0].clone();
    let rng = Rng::from_seed([0; 32]);

    // Create a new ceremony manager with the latest_ceremony_id at 2 below the test_ceremony_id
    let (outgoing_p2p_message_sender, _outgoing_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        test_account_id.clone(),
        outgoing_p2p_message_sender,
        (test_ceremony_id as u64)
            .checked_sub(2)
            .expect("test_ceremony_id must be larger then 1 for this test"),
        &new_test_logger(),
    );

    // Send a keygen request with the test_ceremony_id, skipping forward by 2
    let (result_sender, _) = tokio::sync::oneshot::channel();
    ceremony_manager.on_keygen_request(
        test_ceremony_id,
        vec![test_account_id.clone()],
        rng,
        result_sender,
    );
    assert_eq!(ceremony_manager.get_keygen_states_len(), 1);

    // Process a stage 1 message with a ceremony id 1 less then the test_ceremony_id
    ceremony_manager.process_keygen_data(
        test_account_id,
        (test_ceremony_id as u64)
            .checked_sub(1)
            .expect("test_ceremony_id must be larger then 0 for this test"),
        KeygenData::HashComm1(client::keygen::HashComm1(sp_core::H256::default())),
    );

    // Check that the message was ignored and no unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 1);
}
