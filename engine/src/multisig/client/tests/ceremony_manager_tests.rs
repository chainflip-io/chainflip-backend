use std::collections::BTreeMap;

use super::*;
use crate::{
    logging::test_utils::new_test_logger,
    multisig::{
        client::{
            self,
            ceremony_manager::CeremonyManager,
            common::BroadcastVerificationMessage,
            keygen::KeygenData,
            signing::frost::{SigningCommitment, SigningData},
        },
        crypto::Rng,
        eth::{EthSigning, Point},
    },
};
use rand_legacy::SeedableRng;

#[test]
fn should_ignore_non_first_stage_keygen_data_before_request() {
    let ceremony_id = 0_u64;
    let account_id = AccountId::new([1; 32]);

    // Create a new ceremony manager
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        account_id.clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Process a stage 2 message
    ceremony_manager.process_keygen_data(
        account_id.clone(),
        ceremony_id,
        KeygenData::VerifyHashComm2(BroadcastVerificationMessage {
            data: BTreeMap::new(),
        }),
    );

    // Check that the message was ignored and no unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 0);

    // Process a stage 1 message
    ceremony_manager.process_keygen_data(
        account_id.clone(),
        ceremony_id,
        KeygenData::HashComm1(client::keygen::HashComm1(sp_core::H256::default())),
    );

    // Check that the message was not ignored and an unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 1);

    // Process a stage 2 message
    ceremony_manager.process_keygen_data(
        account_id,
        ceremony_id,
        KeygenData::VerifyHashComm2(BroadcastVerificationMessage {
            data: BTreeMap::new(),
        }),
    );

    // Check that the message was ignored and not added to the delayed messages of the unauthorised ceremony.
    // Only 1 first stage message should be in the delayed messages.
    assert_eq!(
        ceremony_manager.get_delayed_keygen_messages_len(&ceremony_id),
        1
    )
}

#[test]
fn should_ignore_non_first_stage_signing_data_before_request() {
    let ceremony_id = 0_u64;
    let account_id = AccountId::new([1; 32]);

    // Create a new ceremony manager
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        account_id.clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Process a stage 2 message
    ceremony_manager.process_signing_data(
        account_id.clone(),
        ceremony_id,
        SigningData::BroadcastVerificationStage2(BroadcastVerificationMessage {
            data: BTreeMap::new(),
        }),
    );

    // Check that the message was ignored and no unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_signing_states_len(), 0);

    // Process a stage 1 message
    let mut rng = Rng::from_seed([0; 32]);
    ceremony_manager.process_signing_data(
        account_id.clone(),
        ceremony_id,
        SigningData::CommStage1(SigningCommitment {
            d: Point::random(&mut rng),
            e: Point::random(&mut rng),
        }),
    );

    // Check that the message was not ignored and an unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_signing_states_len(), 1);

    // Process a stage 2 message
    ceremony_manager.process_signing_data(
        account_id,
        ceremony_id,
        SigningData::BroadcastVerificationStage2(BroadcastVerificationMessage {
            data: BTreeMap::new(),
        }),
    );

    // Check that the message was ignored and not added to the delayed messages of the unauthorised ceremony.
    // Only 1 first stage message should be in the delayed messages.
    assert_eq!(
        ceremony_manager.get_delayed_signing_messages_len(&ceremony_id),
        1
    )
}
