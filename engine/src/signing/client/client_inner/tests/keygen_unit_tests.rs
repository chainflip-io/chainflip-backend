use super::*;

use std::time::Duration;

#[test]
fn bc1_gets_delayed_until_keygen_request() {
    let params = Parameters {
        threshold: 1,
        share_count: 3,
    };

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let mut client = MultisigClientInner::new(VALIDATOR_IDS[0].clone(), params, tx, PHASE_TIMEOUT);

    assert_eq!(keygen_stage_for(&client, KEY_ID), None);

    let message = create_keygen_p2p_message(&VALIDATOR_IDS[1], create_bc1(2));
    client.process_p2p_mq_message(message);

    assert_eq!(keygen_stage_for(&client, KEY_ID), None);
    assert_eq!(keygen_delayed_count(&client, KEY_ID), 1);

    // Keygen instruction should advance the stage and process delayed messages

    let keygen = MultisigInstruction::KeyGen(KEYGEN_INFO.clone());

    client.process_multisig_instruction(keygen);

    assert_eq!(
        keygen_stage_for(&client, KEY_ID),
        Some(KeygenStage::AwaitingBroadcast1)
    );
    assert_eq!(keygen_delayed_count(&client, KEY_ID), 0);

    // One more message should advance the stage (share_count = 3)
    let message = create_keygen_p2p_message(&VALIDATOR_IDS[2], create_bc1(3));
    client.process_p2p_mq_message(message);

    assert_eq!(
        keygen_stage_for(&client, KEY_ID),
        Some(KeygenStage::AwaitingSecret2)
    );
}

// Simply test the we don't crash when we receive unexpected validator id
#[tokio::test]
async fn keygen_message_from_invalid_validator() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.keygen_phase1.clients[0].clone();

    assert_eq!(
        keygen_stage_for(&c1, KEY_ID),
        Some(KeygenStage::AwaitingBroadcast1)
    );

    let invalid_validator = &UNEXPECTED_VALIDATOR_ID;

    let msg = create_keygen_p2p_message(invalid_validator, create_bc1(2));

    c1.process_p2p_mq_message(msg);
}

#[tokio::test]
async fn keygen_secret2_gets_delayed() {
    let states = generate_valid_keygen_data().await;

    // auciton id is always 0 for generate_valid_keygen_data
    let key_id = KeyId(0);

    let phase1 = &states.keygen_phase1;
    let phase2 = &states.keygen_phase2;

    // Note the use of phase2 data on a phase1 client
    let mut clients_p1 = phase1.clients.clone();
    let bc1_vec = phase1.bc1_vec.clone();
    let sec2_vec = phase2.sec2_vec.clone();

    let c1 = &mut clients_p1[0];
    assert_eq!(
        keygen_stage_for(&c1, key_id),
        Some(KeygenStage::AwaitingBroadcast1)
    );

    // Secret sent from client 2 to client 1
    let sec2 = sec2_vec[1].get(&VALIDATOR_IDS[0]).unwrap().clone();

    // We should not process it immediately
    let message = create_keygen_p2p_message(&VALIDATOR_IDS[1].clone(), sec2);

    c1.process_p2p_mq_message(message);

    assert_eq!(keygen_delayed_count(&c1, key_id), 1);
    assert_eq!(
        keygen_stage_for(&c1, key_id),
        Some(KeygenStage::AwaitingBroadcast1)
    );

    // Process incoming bc1_vec, so we can advance to the next phase
    let message = create_keygen_p2p_message(&VALIDATOR_IDS[1], bc1_vec[1].clone());
    c1.process_p2p_mq_message(message);

    let message = create_keygen_p2p_message(&VALIDATOR_IDS[2], bc1_vec[2].clone());
    c1.process_p2p_mq_message(message);

    assert_eq!(
        keygen_stage_for(&c1, key_id),
        Some(KeygenStage::AwaitingSecret2)
    );
    assert_eq!(keygen_delayed_count(&c1, key_id), 0);
}

/// Test that we can have more than one key simultaneously
#[tokio::test]
async fn can_have_multiple_keys() {
    let states = generate_valid_keygen_data().await;

    // Start with clients that already have an aggregate key
    let mut c1 = states.key_ready.clients[0].clone();

    let next_key_id = KeyId(1);

    let keygen_info = KeygenInfo {
        id: next_key_id,
        signers: KEYGEN_INFO.signers.clone(),
    };

    c1.process_multisig_instruction(MultisigInstruction::KeyGen(keygen_info));

    assert_eq!(keygen_stage_for(&c1, KEY_ID), Some(KeygenStage::KeyReady));
    assert_eq!(
        keygen_stage_for(&c1, next_key_id),
        Some(KeygenStage::AwaitingBroadcast1)
    );
}

#[tokio::test]
async fn cannot_create_key_for_known_id() {
    let mut states = generate_valid_keygen_data().await;

    let mut c1 = states.key_ready.clients[0].clone();

    assert_eq!(keygen_stage_for(&c1, KEY_ID), Some(KeygenStage::KeyReady));

    // Send a new keygen request for the same key id
    let next_key_id = KEY_ID;

    let keygen_info = KeygenInfo {
        id: next_key_id,
        signers: KEYGEN_INFO.signers.clone(),
    };
    c1.process_multisig_instruction(MultisigInstruction::KeyGen(keygen_info));

    // Previous state should be unaffected
    assert_eq!(keygen_stage_for(&c1, KEY_ID), Some(KeygenStage::KeyReady));

    // No message should be sent as a result
    helpers::assert_channel_empty(&mut states.rxs[0]).await;
}

/// Test that if keygen state times out (without keygen request), we slash senders
#[tokio::test]
async fn no_keygen_request() {
    let mut states = generate_valid_keygen_data().await;

    let mut c1 = states.keygen_phase1.clients[0].clone();

    let bc1 = create_bc1(2);

    let bad_validator = &VALIDATOR_IDS[1];

    // We have not received a keygen request for KeyId 1
    let message = helpers::bc1_to_p2p_keygen(bc1, KeyId(1), bad_validator);

    c1.process_p2p_mq_message(message);

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    let mut rx = &mut states.rxs[0];

    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::KeygenResult(KeygenOutcome::unauthorised(
            KeyId(1),
            vec![bad_validator.clone()]
        ))
    );
}

/// Test that if keygen state times out during phase 1 (with keygen request present), we slash non-senders
#[tokio::test]
async fn phase1_timeout() {
    let mut states = generate_valid_keygen_data().await;

    let mut c1 = states.keygen_phase1.clients[0].clone();

    assert_eq!(
        helpers::keygen_stage_for(&c1, KEY_ID),
        Some(KeygenStage::AwaitingBroadcast1)
    );

    let bc1 = states.keygen_phase1.bc1_vec[1].clone();

    let message = helpers::bc1_to_p2p_keygen(bc1, KEY_ID, &VALIDATOR_IDS[1]);

    c1.process_p2p_mq_message(message);

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    let mut rx = &mut states.rxs[0];

    let late_node = VALIDATOR_IDS[2].clone();

    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::KeygenResult(KeygenOutcome::timeout(KEY_ID, vec![late_node]))
    );

    assert_eq!(helpers::keygen_stage_for(&c1, KEY_ID), None);
}

/// Test that if keygen state times out during phase 2 (with keygen request present), we slash non-senders
#[tokio::test]
async fn phase2_timeout() {
    let mut states = generate_valid_keygen_data().await;

    let mut c1 = states.keygen_phase2.clients[0].clone();

    assert_eq!(
        helpers::keygen_stage_for(&c1, KEY_ID),
        Some(KeygenStage::AwaitingSecret2)
    );

    let sec2 = states.keygen_phase2.sec2_vec[1]
        .get(&VALIDATOR_IDS[0])
        .unwrap()
        .clone();

    let message = helpers::sec2_to_p2p_keygen(sec2, &VALIDATOR_IDS[1]);

    c1.process_p2p_mq_message(message);

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    let mut rx = &mut states.rxs[0];

    let late_node = VALIDATOR_IDS[2].clone();

    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::KeygenResult(KeygenOutcome::timeout(KEY_ID, vec![late_node]))
    );

    assert_eq!(helpers::keygen_stage_for(&c1, KEY_ID), None);
}

/// That that parties that send invalid bc1s get reported
#[tokio::test]
async fn invalid_bc1() {
    let mut states = generate_valid_keygen_data().await;

    let mut c1 = states.keygen_phase1.clients[0].clone();

    // This BC1 is valid
    let bc1_a = states.keygen_phase1.bc1_vec[1].clone();
    let message_a = helpers::bc1_to_p2p_keygen(bc1_a.clone(), KEY_ID, &VALIDATOR_IDS[1]);
    c1.process_p2p_mq_message(message_a);

    // This BC1 is invalid
    let bad_node = VALIDATOR_IDS[2].clone();
    let bc1_b = helpers::create_invalid_bc1();
    let message_b = helpers::bc1_to_p2p_keygen(bc1_b, KEY_ID, &bad_node);
    c1.process_p2p_mq_message(message_b);

    let mut rx = &mut states.rxs[0];

    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::KeygenResult(KeygenOutcome::invalid(KEY_ID, vec![bad_node]))
    );

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    assert_eq!(helpers::keygen_stage_for(&c1, KEY_ID), None);
}

/// That that parties that send invalid sec2s get reported
#[tokio::test]
async fn invalid_sec2() {
    let mut states = generate_valid_keygen_data().await;

    let mut c1 = states.keygen_phase2.clients[0].clone();

    // This Sec2 is valid
    let sec2_a = states.keygen_phase2.sec2_vec[1]
        .get(&VALIDATOR_IDS[0])
        .unwrap()
        .clone();
    let message_a = helpers::sec2_to_p2p_keygen(sec2_a.clone(), &VALIDATOR_IDS[1]);
    c1.process_p2p_mq_message(message_a);

    let bad_node = VALIDATOR_IDS[2].clone();
    // This Sec2 is not for us, so it is invalid
    let sec2_b = states.keygen_phase2.sec2_vec[1]
        .get(&VALIDATOR_IDS[2])
        .unwrap()
        .clone();
    let message_b = helpers::sec2_to_p2p_keygen(sec2_b, &bad_node);
    c1.process_p2p_mq_message(message_b);

    let mut rx = &mut states.rxs[0];

    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::KeygenResult(KeygenOutcome::invalid(KEY_ID, vec![bad_node]))
    );

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    assert_eq!(helpers::keygen_stage_for(&c1, KEY_ID), None);
}
