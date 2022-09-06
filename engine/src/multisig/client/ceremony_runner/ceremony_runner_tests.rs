use crate::{
    logging::test_utils::new_test_logger,
    multisig::{
        client::{
            ceremony_manager::KeygenCeremony,
            tests::{
                gen_invalid_keygen_stage_2_state, gen_keygen_data_verify_hash_comm2, ACCOUNT_IDS,
                DEFAULT_KEYGEN_CEREMONY_ID, DEFAULT_KEYGEN_SEED,
            },
        },
        crypto::CryptoScheme,
        eth::EthSigning,
        Rng,
    },
};

use rand_legacy::SeedableRng;

use super::*;

#[tokio::test]
async fn should_ignore_stage_data_with_incorrect_size() {
    let logger = new_test_logger();
    let rng = Rng::from_seed(DEFAULT_KEYGEN_SEED);
    let ceremony_id = DEFAULT_KEYGEN_CEREMONY_ID;
    let num_of_participants = ACCOUNT_IDS.len() as u32;

    // This test only works on message stage data that can have incorrect size (ie. not first stage),
    // so we must create a stage 2 state and add it to the ceremony managers keygen states,
    // allowing us to process a stage 2 message.
    let mut stage_2_state = gen_invalid_keygen_stage_2_state::<<EthSigning as CryptoScheme>::Point>(
        ceremony_id,
        BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
        rng,
        logger.clone(),
    );

    // Built a stage 2 message that has the incorrect number of elements
    let stage_2_data = gen_keygen_data_verify_hash_comm2(num_of_participants + 1);

    // Process the bad message and it should get rejected
    assert_eq!(
        stage_2_state
            .process_or_delay_message(ACCOUNT_IDS[0].clone(), stage_2_data)
            .await,
        None
    );

    // Check that the bad message was ignored, so the stage is still awaiting all num_of_participants messages.
    assert_eq!(
        stage_2_state.get_awaited_parties_count(),
        Some(num_of_participants)
    );
}

#[tokio::test]
async fn should_ignore_non_first_stage_data_before_authorised() {
    let num_of_participants = ACCOUNT_IDS.len() as u32;

    // Create an unauthorised ceremony
    let mut unauthorised_ceremony_runner: CeremonyRunner<KeygenCeremony<EthSigning>> =
        CeremonyRunner::new_unauthorised(DEFAULT_KEYGEN_CEREMONY_ID, &new_test_logger());

    // Process a stage 2 message
    assert_eq!(
        unauthorised_ceremony_runner
            .process_or_delay_message(
                ACCOUNT_IDS[0].clone(),
                gen_keygen_data_verify_hash_comm2(num_of_participants)
            )
            .await,
        None
    );

    // Check that the message was ignored and not delayed
    assert_eq!(unauthorised_ceremony_runner.delayed_messages.len(), 0);
}
