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
        0, /* latest_ceremony_id */
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
        0, /* latest_ceremony_id */
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
#[ignore = "temporarily disabled - see issue #1972"]
fn should_not_create_unauthorized_ceremony_with_invalid_ceremony_id() {
    let latest_ceremony_id = 1; // Invalid, because the CeremonyManager starts with this value as the latest
    let past_ceremony_id = latest_ceremony_id - 1; // Invalid, because it was used in the past
    let future_ceremony_id = latest_ceremony_id + CEREMONY_ID_WINDOW; // Valid, because its within the window
    let future_ceremony_id_too_large = latest_ceremony_id + CEREMONY_ID_WINDOW + 1; // Invalid, because its too far in the future

    // Junk stage 1 data to use for the test
    let stage_1_data = KeygenData::HashComm1(client::keygen::HashComm1(sp_core::H256::default()));

    // Create a new ceremony manager and set the latest_ceremony_id
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        latest_ceremony_id,
        &new_test_logger(),
    );

    // Process a stage 1 message with a ceremony id that is in the past
    ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[0].clone(),
        past_ceremony_id,
        stage_1_data.clone(),
    );

    // Process a stage 1 message with a ceremony id that is too far in the future
    ceremony_manager.process_keygen_data(
        ACCOUNT_IDS[0].clone(),
        future_ceremony_id_too_large,
        stage_1_data.clone(),
    );

    // Check that the messages were ignored and no unauthorised ceremonies were created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 0);

    // Process a stage 1 message with a ceremony id that in the future but still within the window
    ceremony_manager.process_keygen_data(ACCOUNT_IDS[0].clone(), future_ceremony_id, stage_1_data);

    // Check that the message was not ignored and unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 1);
}
