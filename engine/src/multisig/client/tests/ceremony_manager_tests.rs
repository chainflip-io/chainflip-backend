use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use super::{
    helpers::gen_invalid_keygen_comm1, keygen_data_tests::gen_keygen_data_verify_hash_comm2, *,
};
use crate::{
    logging::test_utils::new_test_logger,
    multisig::{
        client::{
            self,
            ceremony_manager::CeremonyManager,
            common::{
                broadcast::BroadcastStage, BroadcastVerificationMessage, CeremonyCommon,
                SigningFailureReason,
            },
            keygen::{
                get_key_data_for_test, HashContext, KeygenData, OutgoingShares,
                VerifyHashCommitmentsBroadcast2,
            },
            state_runner::StateRunner,
            tests::helpers::get_invalid_hash_comm,
            CeremonyFailureReason, MultisigData, PartyIdxMapping,
        },
        crypto::Rng,
        eth::EthSigning,
        tests::fixtures::MESSAGE_HASH,
    },
    testing::assert_ok,
};
use cf_traits::AuthorityCount;
use client::MultisigMessage;
use rand_legacy::SeedableRng;
use tokio::sync::oneshot;
use utilities::threshold_from_share_count;

// This test is for MultisigData::Keygen but also covers the test for signing
// because the code is common.
#[test]
fn should_ignore_non_first_stage_data_before_request() {
    let mut rng = Rng::from_seed(DEFAULT_KEYGEN_SEED);

    // Create a new ceremony manager
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Process a stage 2 message
    let stage_2_data =
        MultisigData::Keygen(KeygenData::VerifyHashComm2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len())
                .map(|i| (i as AuthorityCount, Some(get_invalid_hash_comm(&mut rng))))
                .collect(),
        }));
    ceremony_manager.process_p2p_message(
        ACCOUNT_IDS[0].clone(),
        MultisigMessage {
            ceremony_id: DEFAULT_KEYGEN_CEREMONY_ID,
            data: stage_2_data.clone(),
        },
    );

    // Check that the message was ignored and no unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 0);

    // Process a stage 1 message
    ceremony_manager.process_p2p_message(
        ACCOUNT_IDS[0].clone(),
        MultisigMessage {
            ceremony_id: DEFAULT_KEYGEN_CEREMONY_ID,
            data: MultisigData::Keygen(KeygenData::HashComm1(client::keygen::HashComm1(
                sp_core::H256::default(),
            ))),
        },
    );

    // Check that the message was not ignored and an unauthorised ceremony was created
    assert_eq!(ceremony_manager.get_keygen_states_len(), 1);

    // Process a stage 2 message
    ceremony_manager.process_p2p_message(
        ACCOUNT_IDS[0].clone(),
        MultisigMessage {
            ceremony_id: DEFAULT_KEYGEN_CEREMONY_ID,
            data: stage_2_data,
        },
    );

    // Check that the stage 2 message was ignored and not added to the delayed messages of the unauthorised ceremony.
    // Only 1 first stage message should be in the delayed messages.
    assert_eq!(
        ceremony_manager.get_delayed_keygen_messages_len(&DEFAULT_KEYGEN_CEREMONY_ID),
        1
    )
}

#[tokio::test]
#[should_panic]
async fn should_panic_keygen_request_if_not_participating() {
    let non_participating_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&non_participating_id));

    // Create a new ceremony manager with the non_participating_id
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        non_participating_id,
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Send a keygen request where participants doesn't include non_participating_id
    let (result_sender, _result_receiver) = oneshot::channel();
    ceremony_manager.on_keygen_request(
        DEFAULT_KEYGEN_CEREMONY_ID,
        ACCOUNT_IDS.clone(),
        Rng::from_seed(DEFAULT_KEYGEN_SEED),
        result_sender,
    );
}

#[tokio::test]
#[should_panic]
async fn should_panic_rts_if_not_participating() {
    let non_participating_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&non_participating_id));

    // Generate a key to use in this test
    let keygen_result_info = get_key_data_for_test(&ACCOUNT_IDS);

    // Create a new ceremony manager with the non_participating_id
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        non_participating_id,
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Send a signing request where participants doesn't include non_participating_id
    let (result_sender, _result_receiver) = oneshot::channel();
    ceremony_manager.on_request_to_sign(
        DEFAULT_SIGNING_CEREMONY_ID,
        ACCOUNT_IDS.clone(),
        MESSAGE_HASH.clone(),
        keygen_result_info,
        Rng::from_seed(DEFAULT_SIGNING_SEED),
        result_sender,
    );
}

#[tokio::test]
async fn should_ignore_duplicate_keygen_request() {
    // Create a new ceremony manager
    let (p2p_sender, _p2p_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut ceremony_manager =
        CeremonyManager::<EthSigning>::new(ACCOUNT_IDS[0].clone(), p2p_sender, &new_test_logger());

    // Send a keygen request with the DEFAULT_KEYGEN_CEREMONY_ID
    ceremony_manager.on_keygen_request(
        DEFAULT_KEYGEN_CEREMONY_ID,
        ACCOUNT_IDS.clone(),
        Rng::from_seed(DEFAULT_KEYGEN_SEED),
        oneshot::channel().0,
    );

    // Check that the ceremony started
    assert_ok!(ceremony_manager.ensure_ceremony_at_keygen_stage(1, DEFAULT_KEYGEN_CEREMONY_ID));

    // Send another keygen request with the same ceremony id (DEFAULT_KEYGEN_CEREMONY_ID)
    let (result_sender, mut result_receiver) = oneshot::channel();
    ceremony_manager.on_keygen_request(
        DEFAULT_KEYGEN_CEREMONY_ID,
        ACCOUNT_IDS.clone(),
        Rng::from_seed(DEFAULT_KEYGEN_SEED),
        result_sender,
    );

    // Receive the DuplicateCeremonyId error result
    assert_eq!(
        result_receiver
            .try_recv()
            .expect("Failed to receive ceremony result"),
        Err((
            BTreeSet::default(),
            CeremonyFailureReason::DuplicateCeremonyId
        ))
    );
}

#[tokio::test]
async fn should_ignore_duplicate_rts() {
    // Generate a key to use in this test
    let keygen_result_info = get_key_data_for_test(&ACCOUNT_IDS);

    // Create a new ceremony manager
    let (p2p_sender, _p2p_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut ceremony_manager =
        CeremonyManager::<EthSigning>::new(ACCOUNT_IDS[0].clone(), p2p_sender, &new_test_logger());

    // Send a signing request with the DEFAULT_SIGNING_CEREMONY_ID
    ceremony_manager.on_request_to_sign(
        DEFAULT_SIGNING_CEREMONY_ID,
        ACCOUNT_IDS.clone(),
        MESSAGE_HASH.clone(),
        keygen_result_info.clone(),
        Rng::from_seed(DEFAULT_SIGNING_SEED),
        oneshot::channel().0,
    );

    // Check that the ceremony started
    assert_ok!(ceremony_manager.ensure_ceremony_at_signing_stage(1, DEFAULT_SIGNING_CEREMONY_ID));

    // Send another signing request with the same ceremony id (DEFAULT_SIGNING_CEREMONY_ID)
    let (result_sender, mut result_receiver) = oneshot::channel();
    ceremony_manager.on_request_to_sign(
        DEFAULT_SIGNING_CEREMONY_ID,
        ACCOUNT_IDS.clone(),
        MESSAGE_HASH.clone(),
        keygen_result_info,
        Rng::from_seed(DEFAULT_SIGNING_SEED),
        result_sender,
    );
    // Receive the DuplicateCeremonyId error result
    assert_eq!(
        result_receiver
            .try_recv()
            .expect("Failed to receive ceremony result"),
        Err((
            BTreeSet::default(),
            CeremonyFailureReason::DuplicateCeremonyId
        ))
    );
}

#[tokio::test]
async fn should_ignore_keygen_request_with_duplicate_signer() {
    // Create a list of participants with a duplicate id
    let mut participants = ACCOUNT_IDS.clone();
    participants[1] = participants[2].clone();

    // Create a new ceremony manager
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Send a keygen request with the duplicate id
    let (result_sender, mut result_receiver) = oneshot::channel();
    ceremony_manager.on_keygen_request(
        DEFAULT_KEYGEN_CEREMONY_ID,
        participants,
        Rng::from_seed(DEFAULT_KEYGEN_SEED),
        result_sender,
    );

    // Receive the InvalidParticipants error result
    assert_eq!(
        result_receiver
            .try_recv()
            .expect("Failed to receive ceremony result"),
        Err((
            BTreeSet::default(),
            CeremonyFailureReason::InvalidParticipants
        ))
    );
}

#[tokio::test]
async fn should_ignore_rts_with_duplicate_signer() {
    // Generate a key to use in this test
    let keygen_result_info = get_key_data_for_test(&ACCOUNT_IDS);

    // Create a list of signers with a duplicate id
    let mut participants = ACCOUNT_IDS.clone();
    participants[1] = participants[2].clone();

    // Create a new ceremony manager
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Send a signing request with the duplicate id
    let (result_sender, mut result_receiver) = oneshot::channel();
    ceremony_manager.on_request_to_sign(
        DEFAULT_SIGNING_CEREMONY_ID,
        participants,
        MESSAGE_HASH.clone(),
        keygen_result_info,
        Rng::from_seed(DEFAULT_SIGNING_SEED),
        result_sender,
    );

    // Receive the InvalidParticipants error result
    assert_eq!(
        result_receiver
            .try_recv()
            .expect("Failed to receive ceremony result"),
        Err((
            BTreeSet::default(),
            CeremonyFailureReason::InvalidParticipants
        ))
    );
}

#[tokio::test]
async fn should_ignore_rts_with_insufficient_number_of_signers() {
    // Generate a key to use in this test
    let keygen_result_info = get_key_data_for_test(&ACCOUNT_IDS);

    // Create a list of signers that is equal to the threshold (not enough to generate a signature)
    let threshold = threshold_from_share_count(ACCOUNT_IDS.len() as u32) as usize;
    let not_enough_participants = ACCOUNT_IDS[0..threshold].to_vec();

    // Create a new ceremony manager
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Send a signing request with not enough participants
    let (result_sender, mut result_receiver) = oneshot::channel();
    ceremony_manager.on_request_to_sign(
        DEFAULT_SIGNING_CEREMONY_ID,
        not_enough_participants,
        MESSAGE_HASH.clone(),
        keygen_result_info,
        Rng::from_seed(DEFAULT_SIGNING_SEED),
        result_sender,
    );

    // Receive the NotEnoughSigners error result
    assert_eq!(
        result_receiver
            .try_recv()
            .expect("Failed to receive ceremony result"),
        Err((
            BTreeSet::default(),
            CeremonyFailureReason::Other(SigningFailureReason::NotEnoughSigners),
        ))
    );
}

#[tokio::test]
async fn should_ignore_rts_with_unknown_signer_id() {
    // Generate a key to use in this test using the ACCOUNT_IDS
    let keygen_result_info = get_key_data_for_test(&ACCOUNT_IDS);

    // Create a new ceremony manager with an account id that is in ACCOUNT_IDS
    let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
        ACCOUNT_IDS[0].clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        &new_test_logger(),
    );

    // Replace one of the signers with an unknown id
    let unknown_signer_id = AccountId::new([0; 32]);
    assert!(!ACCOUNT_IDS.contains(&unknown_signer_id));
    let mut participants = ACCOUNT_IDS.clone();
    participants[1] = unknown_signer_id;

    // Send a signing request with the modified participants
    let (result_sender, mut result_receiver) = oneshot::channel();
    ceremony_manager.on_request_to_sign(
        DEFAULT_SIGNING_CEREMONY_ID,
        participants,
        MESSAGE_HASH.clone(),
        keygen_result_info,
        Rng::from_seed(DEFAULT_SIGNING_SEED),
        result_sender,
    );

    // Receive the InvalidParticipants error result
    assert_eq!(
        result_receiver
            .try_recv()
            .expect("Failed to receive ceremony result"),
        Err((
            BTreeSet::default(),
            CeremonyFailureReason::InvalidParticipants,
        ))
    );
}

