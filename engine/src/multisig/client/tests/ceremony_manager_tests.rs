use super::*;
use crate::{
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

//TODO: test to see if the size check is ran on both k & s
