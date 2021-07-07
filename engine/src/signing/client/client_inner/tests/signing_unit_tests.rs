use super::*;

/// After we've received a request to sign, we should immediately be able
/// to receive Broadcast1 messages
#[tokio::test]
async fn should_await_bc1_after_rts() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.key_ready.clients[0].clone();

    let key = c1
        .get_keygen()
        .get_key_info_by_id(KEY_ID)
        .expect("no key")
        .to_owned();

    c1.signing_manager
        .on_request_to_sign(MESSAGE_HASH.clone(), key, SIGN_INFO.clone());

    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE_INFO),
        Some(SigningStage::AwaitingBroadcast1)
    );
}

/// BC1 messages get processed if we receive RTS shortly after
#[tokio::test]
async fn should_process_delayed_bc1_after_rts() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.key_ready.clients[0].clone();

    assert!(get_stage_for_msg(&c1, &MESSAGE_INFO).is_none());

    let bc1 = states.sign_phase1.bc1_vec[1].clone();

    let wdata = SigningDataWrapped::new(bc1, MESSAGE_INFO.clone());

    c1.signing_manager
        .process_signing_data(VALIDATOR_IDS[1].clone(), wdata);

    assert_eq!(get_stage_for_msg(&c1, &MESSAGE_INFO), None);

    assert_eq!(signing_delayed_count(&c1, &MESSAGE_INFO), 1);

    let key = c1
        .get_keygen()
        .get_key_info_by_id(KEY_ID)
        .expect("no key")
        .to_owned();

    c1.signing_manager
        .on_request_to_sign(MESSAGE_HASH.clone(), key, SIGN_INFO.clone());

    assert_eq!(signing_delayed_count(&c1, &MESSAGE_INFO), 0);

    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE_INFO),
        Some(SigningStage::AwaitingSecret2)
    );
}

/// By sending (signing) BC1, a node is trying to start a signing procedure,
/// but we only process it after we've received a signing instruction from
/// our SC. If we don't receive it after a certain period of time, BC1 should
/// be removed and the sender should be penalised.
#[test]
fn delayed_signing_bc1_gets_removed() {
    // Setup
    let params = Parameters {
        threshold: 1,
        share_count: 3,
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let timeout = Duration::from_millis(1);

    let mut client = MultisigClientInner::new(VALIDATOR_IDS[0].clone(), params, tx, timeout);

    // Create delayed BC1
    let bc1 = create_bc1(2).into();
    let m = helpers::bc1_to_p2p_signing(bc1, &VALIDATOR_IDS[1], &MESSAGE_INFO);
    client.process_p2p_mq_message(m);

    assert_eq!(get_stage_for_msg(&client, &MESSAGE_INFO), None);
    assert_eq!(signing_delayed_count(&client, &MESSAGE_INFO), 1);

    // Wait for the data to expire
    std::thread::sleep(timeout);

    client.cleanup();

    assert_eq!(signing_delayed_count(&client, &MESSAGE_INFO), 0);
}

#[tokio::test]
async fn signing_secret2_gets_delayed() {
    let states = generate_valid_keygen_data().await;

    let phase1 = &states.sign_phase1;
    let phase2 = &states.sign_phase2;

    // Client in phase1 should be able to receive phase2 data (Secret2)

    let mut c1 = phase1.clients[0].clone();

    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE_INFO),
        Some(SigningStage::AwaitingBroadcast1)
    );

    let sec2 = phase2.sec2_vec[1].get(&VALIDATOR_IDS[0]).unwrap().clone();

    let m = sec2_to_p2p_signing(sec2, &VALIDATOR_IDS[1], &MESSAGE_INFO);

    c1.process_p2p_mq_message(m);

    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE_INFO),
        Some(SigningStage::AwaitingBroadcast1)
    );

    // Finally c1 receives bc1 and able to advance to phase2
    let bc1 = phase1.bc1_vec[1].clone();

    let m = helpers::bc1_to_p2p_signing(bc1, &VALIDATOR_IDS[1], &MESSAGE_INFO);

    c1.process_p2p_mq_message(m);

    // We are able to process delayed secret2 and immediately
    // go from phase1 to phase3
    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE_INFO),
        Some(SigningStage::AwaitingLocalSig3)
    );
}

#[tokio::test]
async fn signing_local_sig_gets_delayed() {
    let mut states = generate_valid_keygen_data().await;

    let phase2 = &states.sign_phase2;
    let phase3 = &states.sign_phase3;

    let mut c1_p2 = phase2.clients[0].clone();
    let local_sig = phase3.local_sigs[1].clone();

    let m = sig_to_p2p(local_sig, &VALIDATOR_IDS[1], &MESSAGE_INFO);

    c1_p2.process_p2p_mq_message(m);

    assert_eq!(
        get_stage_for_msg(&c1_p2, &MESSAGE_INFO),
        Some(SigningStage::AwaitingSecret2)
    );

    // Send Secret2 to be able to process delayed LocalSig
    let sec2 = phase2.sec2_vec[1].get(&VALIDATOR_IDS[0]).unwrap().clone();

    let m = sec2_to_p2p_signing(sec2, &VALIDATOR_IDS[1], &MESSAGE_INFO);

    c1_p2.process_p2p_mq_message(m);

    match recv_next_signal_message_skipping(&mut states.rxs[0]).await {
        Some(InnerSignal::MessageSigned(_, _)) => { /* all good */ }
        _ => panic!("Expected MessageSigned signal"),
    }
}

/// Request to sign should be delayed until the key is ready
#[tokio::test]
async fn request_to_sign_before_key_ready() {
    let key_id = KeyId(0);

    let states = generate_valid_keygen_data().await;

    let mut c1 = states.keygen_phase2.clients[0].clone();

    assert_eq!(
        keygen_stage_for(&c1, key_id),
        Some(KeygenStage::AwaitingSecret2)
    );

    // BC1 for siging arrives before the key is ready
    let bc1_sign = states.sign_phase1.bc1_vec[1].clone();

    let m = helpers::bc1_to_p2p_signing(bc1_sign, &VALIDATOR_IDS[1], &MESSAGE_INFO);

    c1.process_p2p_mq_message(m);

    assert_eq!(get_stage_for_msg(&c1, &MESSAGE_INFO), None);

    // Finalize key generation and make sure we can make progress on signing the message

    let sec2_1 = states.keygen_phase2.sec2_vec[1]
        .get(&VALIDATOR_IDS[0])
        .unwrap()
        .clone();
    let m = sec2_to_p2p_keygen(sec2_1, &VALIDATOR_IDS[1]);
    c1.process_p2p_mq_message(m);

    let sec2_2 = states.keygen_phase2.sec2_vec[2]
        .get(&VALIDATOR_IDS[0])
        .unwrap()
        .clone();
    let m = sec2_to_p2p_keygen(sec2_2, &VALIDATOR_IDS[2]);
    c1.process_p2p_mq_message(m);

    assert_eq!(keygen_stage_for(&c1, key_id), Some(KeygenStage::KeyReady));

    assert_eq!(get_stage_for_msg(&c1, &MESSAGE_INFO), None);

    c1.process_multisig_instruction(MultisigInstruction::Sign(
        MESSAGE_HASH.clone(),
        SIGN_INFO.clone(),
    ));

    // We only need one BC1 (the delayed one) to proceed
    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE_INFO),
        Some(SigningStage::AwaitingSecret2)
    );
}

/// Request to sign contains signer ids not associated with the key.
/// Expected outcome: no crash, state not created
#[tokio::test]
async fn unknown_signer_ids_gracefully_handled() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.key_ready.clients[0].clone();

    // Note the unknown validator id
    let signers = vec![VALIDATOR_IDS[0].clone(), ValidatorId::new(200)];

    let info = SigningInfo {
        id: KeyId(0),
        signers,
    };

    c1.process_multisig_instruction(MultisigInstruction::Sign(MESSAGE_HASH.clone(), info));

    assert_eq!(get_stage_for_msg(&c1, &MESSAGE_INFO), None);
}

// Test that the sign state times out without a sign request
#[tokio::test]
async fn no_sign_request() {
    init_logs_once();

    let states = generate_valid_keygen_data().await;

    let mut c1 = states.key_ready.clients[0].clone();

    let bc1 = create_bc1(2);

    // We have not received a sign request for KeyId 1
    let key_id = KeyId(1);
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id,
    };
    assert_ne!(
        KEY_ID, key_id,
        "Signing request key must be different from one used in generate_valid_keygen_data"
    );
    let message = helpers::bc1_to_p2p_signing(bc1, &SIGNER_IDS[1], &message_info);

    c1.process_p2p_mq_message(message);

    // check that the message is in the delay buffer
    assert_eq!(c1.signing_manager.get_delayed_count(&message_info), 1);

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    //let mut rx = &mut states.rxs[0];
    // TODO: Cleanup needs to send signal and we detect it here.
    // assert_eq!(
    //     helpers::recv_next_inner_event(&mut rx).await,
    //     InnerEvent::KeygenResult(KeygenOutcome::unauthorised(KeyId(1), vec![]))
    // );

    // we check to see that it was dropped from the buffer
    assert_eq!(c1.signing_manager.get_delayed_count(&message_info), 0);
}

/// Test that if signing state times out during phase 1 (with sign request present)
#[tokio::test]
async fn phase1_timeout() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.sign_phase1.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );

    // send a bc1 to a client
    let bc1 = states.sign_phase1.bc1_vec[1].clone();
    let id = &SIGNER_IDS[1];
    let message = helpers::bc1_to_p2p_signing(bc1, id, &MESSAGE_INFO);
    c1.process_p2p_mq_message(message);

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    // TODO: Cleanup needs to send signal and we detect it here.

    assert!(c1.signing_manager.get_state_for(&MESSAGE_INFO).is_none());
}

/// Test that signing state times out during phase 2
#[tokio::test]
async fn phase2_timeout() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.sign_phase2.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingSecret2
    );

    // Because we only have 2 clients in the signing test, we cant test receiving another secret 2 before timeout.

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    // TODO: Cleanup needs to send signal and we detect it here.

    assert!(c1.signing_manager.get_state_for(&MESSAGE_INFO).is_none());
}

/// Test that signing state times out during phase 3
#[tokio::test]
async fn phase3_timeout() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.sign_phase3.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingLocalSig3
    );

    // because we only have 2 clients in the signing test, we cant test receiving a local sig before timeout.

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    // TODO: Cleanup needs to send signal and we detect it here.

    assert!(c1.signing_manager.get_state_for(&MESSAGE_INFO).is_none());
}

// test that a request to sign for a message that is already in use
#[tokio::test]
async fn cannot_create_duplicate_sign_request() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.sign_phase3.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingLocalSig3
    );

    // send a signing request to a client
    c1.process_multisig_instruction(MultisigInstruction::Sign(
        MessageHash(MESSAGE.clone()),
        SigningInfo {
            id: KEY_ID,
            signers: SIGNER_IDS.clone(),
        },
    ));

    // Previous state should be unaffected
    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingLocalSig3
    );
}

// test that a sign request from a client that is not in the current selection is ignored
#[tokio::test]
async fn sign_request_from_invalid_validator() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.sign_phase1.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );

    let invalid_validator = VALIDATOR_IDS[2].clone();
    // make sure that the id is indeed invalid
    assert!(
        !SIGNER_IDS.contains(&invalid_validator),
        "invalid_validator id {}, must not be in the SIGNER_IDS",
        invalid_validator
    );

    // send the bc1 with the invalid ID
    let bc1 = states.sign_phase1.bc1_vec[1].clone();
    let id = &invalid_validator;
    let message = helpers::bc1_to_p2p_signing(bc1, id, &MESSAGE_INFO);
    c1.process_p2p_mq_message(message);

    // just check that we didn't advance to the next phase
    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );
    //TODO: report the invalid id.
}

// Test that a bc1 with an incorrect hash is ignored
#[tokio::test]
async fn bc1_with_invalid_hash() {
    let states = generate_valid_keygen_data().await;

    let mut c1 = states.sign_phase1.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );

    // send a bc1 to the client with the message hash from message 2 instead
    let bc1 = states.sign_phase1.bc1_vec[1].clone();
    let id = &SIGNER_IDS[1];
    let mi = MessageInfo {
        hash: MessageHash(MESSAGE2.clone()),
        key_id: KEY_ID,
    };
    let message = helpers::bc1_to_p2p_signing(bc1, id, &mi);
    c1.process_p2p_mq_message(message);

    // make sure we did not advance the stage
    assert_eq!(
        c1.signing_manager
            .get_state_for(&MESSAGE_INFO)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );
}
