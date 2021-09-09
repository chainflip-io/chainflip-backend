use crate::{logging, signing::db::KeyDBMock};

use super::*;

/// After we've received a request to sign, we should immediately be able
/// to receive Broadcast1 messages
#[tokio::test]
async fn should_await_bc1_after_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id: key_id.clone(),
    };

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    let key = c1.get_key(key_id).expect("no key").to_owned();

    c1.signing_manager
        .on_request_to_sign(MESSAGE_HASH.clone(), key, sign_info.clone());

    assert_eq!(
        get_stage_for_msg(&c1, &message_info),
        Some(SigningStage::AwaitingBroadcast1)
    );
}

/// BC1 messages get processed if we receive RTS shortly after
#[tokio::test]
async fn should_process_delayed_bc1_after_rts() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id: key_id.clone(),
    };
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    assert!(get_stage_for_msg(&c1, &message_info).is_none());

    let bc1 = sign_states.sign_phase1.bc1_vec[1].clone();

    let wdata = SigningDataWrapped::new(bc1, message_info.clone());

    c1.signing_manager
        .process_signing_data(VALIDATOR_IDS[1].clone(), wdata);

    assert_eq!(get_stage_for_msg(&c1, &message_info), None);

    assert_eq!(signing_delayed_count(&c1, &message_info), 1);

    let key = c1.get_key(key_id).expect("no key").to_owned();

    c1.signing_manager
        .on_request_to_sign(MESSAGE_HASH.clone(), key, sign_info.clone());

    assert_eq!(signing_delayed_count(&c1, &message_info), 0);

    assert_eq!(
        get_stage_for_msg(&c1, &message_info),
        Some(SigningStage::AwaitingSecret2)
    );
}

/// By sending (signing) BC1, a node is trying to start a signing procedure,
/// but we only process it after we've received a signing instruction from
/// our SC. If we don't receive it after a certain period of time, BC1 should
/// be removed and the sender should be penalised.
#[tokio::test]
async fn delayed_signing_bc1_gets_removed() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let timeout = Duration::from_secs(0);
    let logger = logging::test_utils::create_test_logger();

    let mut client = MultisigClientInner::new(
        SIGNER_IDS[0].clone(),
        KeyDBMock::new(),
        tx,
        timeout,
        &logger,
    );

    // Create delayed BC1
    let bad_node = SIGNER_IDS[1].clone();
    let bc1 = create_bc1(2).into();
    let key_id: KeyId = KeyId(Vec::default());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id,
    };
    let m = helpers::bc1_to_p2p_signing(bc1, &bad_node, &message_info);
    client.process_p2p_message(m);

    assert_eq!(get_stage_for_msg(&client, &message_info), None);
    assert_eq!(signing_delayed_count(&client, &message_info), 1);

    // Trigger the timeout
    client.set_timeout(timeout);
    client.cleanup();

    assert_eq!(signing_delayed_count(&client, &message_info), 0);

    // check that we get the 'unauthorised' SigningOutcome signal
    assert_eq!(
        helpers::recv_next_signal_message_skipping(&mut rx).await,
        Some(SigningOutcome::unauthorised(
            message_info.clone(),
            vec![bad_node]
        ))
    );
}

#[tokio::test]
async fn signing_secret2_gets_delayed() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let phase1 = &sign_states.sign_phase1;
    let phase2 = &sign_states.sign_phase2;

    // Client in phase1 should be able to receive phase2 data (Secret2)

    let mut c1 = phase1.clients[0].clone();

    assert_eq!(
        get_stage_for_msg(&c1, &message_info),
        Some(SigningStage::AwaitingBroadcast1)
    );

    let sec2 = phase2.sec2_vec[1].get(&VALIDATOR_IDS[0]).unwrap().clone();

    let m = sec2_to_p2p_signing(sec2, &VALIDATOR_IDS[1], &message_info);

    c1.process_p2p_message(m);

    assert_eq!(
        get_stage_for_msg(&c1, &message_info),
        Some(SigningStage::AwaitingBroadcast1)
    );

    // Finally c1 receives bc1 and able to advance to phase2
    let bc1 = phase1.bc1_vec[1].clone();

    let m = helpers::bc1_to_p2p_signing(bc1, &VALIDATOR_IDS[1], &message_info);

    c1.process_p2p_message(m);

    // We are able to process delayed secret2 and immediately
    // go from phase1 to phase3
    assert_eq!(
        get_stage_for_msg(&c1, &message_info),
        Some(SigningStage::AwaitingLocalSig3)
    );
}

#[tokio::test]
async fn signing_local_sig_gets_delayed() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let sign_states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let phase2 = &sign_states.sign_phase2;
    let phase3 = &sign_states.sign_phase3;

    let mut c1_p2 = phase2.clients[0].clone();
    let local_sig = phase3.local_sigs[1].clone();

    let m = sig_to_p2p(local_sig, &VALIDATOR_IDS[1], &message_info);

    c1_p2.process_p2p_message(m);

    assert_eq!(
        get_stage_for_msg(&c1_p2, &message_info),
        Some(SigningStage::AwaitingSecret2)
    );

    // Send Secret2 to be able to process delayed LocalSig
    let sec2 = phase2.sec2_vec[1].get(&VALIDATOR_IDS[0]).unwrap().clone();

    let m = sec2_to_p2p_signing(sec2, &VALIDATOR_IDS[1], &message_info);

    c1_p2.process_p2p_message(m);

    match recv_next_signal_message_skipping(&mut ctx.rxs[0]).await {
        Some(SigningOutcome { result: Ok(_), .. }) => { /* all good */ }
        _ => panic!("Expected MessageSigned signal"),
    }
}

/// Request to sign should be delayed until the key is ready
#[tokio::test]
async fn request_to_sign_before_key_ready() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = keygen_states.keygen_phase2.clients[0].clone();

    assert_eq!(
        keygen_stage_for(&c1, CEREMONY_ID.clone()),
        Some(KeygenStage::AwaitingSecret2)
    );

    // BC1 for signing arrives before the key is ready
    let bc1_sign = states.sign_phase1.bc1_vec[1].clone();

    let m = helpers::bc1_to_p2p_signing(bc1_sign, &VALIDATOR_IDS[1], &message_info);

    c1.process_p2p_message(m);

    assert_eq!(get_stage_for_msg(&c1, &message_info), None);

    // Finalize key generation and make sure we can make progress on signing the message

    let sec2_1 = keygen_states.keygen_phase2.sec2_vec[1]
        .get(&VALIDATOR_IDS[0])
        .unwrap()
        .clone();
    let m = sec2_to_p2p_keygen(sec2_1, &VALIDATOR_IDS[1]);
    c1.process_p2p_message(m);

    let sec2_2 = keygen_states.keygen_phase2.sec2_vec[2]
        .get(&VALIDATOR_IDS[0])
        .unwrap()
        .clone();
    let m = sec2_to_p2p_keygen(sec2_2, &VALIDATOR_IDS[2]);
    c1.process_p2p_message(m);

    assert_eq!(
        keygen_stage_for(&c1, CEREMONY_ID.clone()),
        Some(KeygenStage::KeyReady)
    );

    assert_eq!(get_stage_for_msg(&c1, &message_info), None);

    c1.process_multisig_instruction(MultisigInstruction::Sign(
        MESSAGE_HASH.clone(),
        sign_info.clone(),
    ));

    // We only need one BC1 (the delayed one) to proceed
    assert_eq!(
        get_stage_for_msg(&c1, &message_info),
        Some(SigningStage::AwaitingSecret2)
    );
}

/// Request to sign contains signer ids not associated with the key.
/// Expected outcome: no crash, state not created
#[tokio::test]
async fn unknown_signer_ids_gracefully_handled() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };

    let mut c1 = keygen_states.key_ready.clients[0].clone();

    // Note the unknown validator id
    let signers = vec![VALIDATOR_IDS[0].clone(), ValidatorId([200; 32])];

    let info = SigningInfo { key_id, signers };

    c1.process_multisig_instruction(MultisigInstruction::Sign(MESSAGE_HASH.clone(), info));

    assert_eq!(get_stage_for_msg(&c1, &message_info), None);
}

/// Test that if signing state times out during phase 1 (with sign request present)
#[tokio::test]
async fn phase1_timeout() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };

    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;
    let mut c1 = states.sign_phase1.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );

    // Send nothing to the client, the sign request is already present in phase1

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    assert_eq!(get_stage_for_msg(&c1, &message_info), None);

    let mut rx = &mut ctx.rxs[0];

    let late_node = SIGNER_IDS[1].clone();

    // check that we get the 'timeout' SigningOutcome signal
    assert_eq!(
        helpers::recv_next_signal_message_skipping(&mut rx).await,
        Some(SigningOutcome::timeout(
            message_info.clone(),
            vec![late_node]
        ))
    );

    assert!(c1.signing_manager.get_state_for(&message_info).is_none());
}

/// Test that signing state times out during phase 2
#[tokio::test]
async fn phase2_timeout() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = states.sign_phase2.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingSecret2
    );

    // Because we only have 2 clients in the signing test, we cant test receiving another secret 2 before timeout.

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    let mut rx = &mut ctx.rxs[0];

    let late_node = SIGNER_IDS[1].clone();

    // check that we get the 'timeout' SigningOutcome signal
    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::SigningResult(SigningOutcome::timeout(
            message_info.clone(),
            vec![late_node]
        ))
    );

    assert!(c1.signing_manager.get_state_for(&message_info).is_none());
}

/// Test that signing state times out during phase 3
#[tokio::test]
async fn phase3_timeout() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = states.sign_phase3.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingLocalSig3
    );

    // because we only have 2 clients in the signing test, we cant test receiving a local sig before timeout.

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    let mut rx = &mut ctx.rxs[0];

    let late_node = SIGNER_IDS[1].clone();

    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::SigningResult(SigningOutcome::timeout(
            message_info.clone(),
            vec![late_node]
        ))
    );

    assert!(c1.signing_manager.get_state_for(&message_info).is_none());
}

// test that a request to sign for a message that is already in use
#[tokio::test]
async fn cannot_create_duplicate_sign_request() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = states.sign_phase3.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingLocalSig3
    );

    // send a signing request to a client
    c1.process_multisig_instruction(MultisigInstruction::Sign(
        MessageHash(MESSAGE.clone()),
        sign_info,
    ));

    // Previous state should be unaffected
    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingLocalSig3
    );
}

// test that a sign request from a client that is not in the current selection is ignored
#[tokio::test]
async fn sign_request_from_invalid_validator() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = states.sign_phase1.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
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
    let message = helpers::bc1_to_p2p_signing(bc1, id, &message_info);
    c1.process_p2p_message(message);

    // just check that we didn't advance to the next phase
    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );
    //TODO: report the invalid id.
}

// Test that a bc1 with a different message hash does not effect a sign in progress
#[tokio::test]
async fn bc1_with_different_hash() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = states.sign_phase1.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );

    // send a bc1 to the client with the message hash from message 2 instead
    let bc1 = states.sign_phase1.bc1_vec[1].clone();
    let id = &SIGNER_IDS[1];
    let mi = MessageInfo {
        hash: MessageHash(MESSAGE2.clone()),
        key_id: KeyId(PUB_KEY.into()),
    };
    assert_ne!(
        mi.hash, message_info.hash,
        "MESSAGE and MESSAGE2 need to have different hashes"
    );
    let message = helpers::bc1_to_p2p_signing(bc1, id, &mi);
    c1.process_p2p_message(message);

    // make sure we did not advance the stage of message 1
    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );
}

/// Test that an invalid bc1 is reported
#[tokio::test]
async fn invalid_bc1() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = states.sign_phase1.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingBroadcast1
    );

    // send an invalid bc1
    let bad_node = SIGNER_IDS[1].clone();
    let bc1 = create_invalid_bc1();
    let message = helpers::bc1_to_p2p_signing(bc1, &bad_node, &message_info);
    c1.process_p2p_message(message);

    // make sure we the signing is abandoned
    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::Abandoned
    );

    let mut rx = &mut ctx.rxs[0];

    // check that we got the 'invalid' signal
    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::SigningResult(SigningOutcome::invalid(
            message_info.clone(),
            vec![bad_node]
        ))
    );

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    // check that the signing was cleaned up
    assert!(c1.signing_manager.get_state_for(&message_info).is_none());

    // make sure the timeout is not triggered for the abandoned signing
    assert_eq!(helpers::check_for_inner_event(&mut rx).await, None);
}

/// Test that an invalid secret2 is reported
#[tokio::test]
async fn invalid_secret2() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = states.sign_phase2.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingSecret2
    );

    // send the secret2 from 0->1 back to client 0
    let bad_node = SIGNER_IDS[1].clone();
    let sec2 = states.sign_phase2.sec2_vec[0]
        .get(&bad_node)
        .unwrap()
        .clone();
    let m = sec2_to_p2p_signing(sec2, &bad_node, &message_info);
    c1.process_p2p_message(m);

    // make sure we the signing is abandoned
    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::Abandoned
    );

    let mut rx = &mut ctx.rxs[0];

    // check that we got the 'invalid' signal
    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::SigningResult(SigningOutcome::invalid(
            message_info.clone(),
            vec![bad_node]
        ))
    );

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    // check that the signing was cleaned up
    assert!(c1.signing_manager.get_state_for(&message_info).is_none());

    // make sure the timeout is not triggered for the abandoned signing
    assert_eq!(helpers::check_for_inner_event(&mut rx).await, None);
}

/// Test that we report an invalid local signature and abandon ceremony
#[tokio::test]
async fn invalid_local_sig() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;
    let key_id: KeyId = KeyId(keygen_states.key_ready.pubkey.serialize().into());
    let message_info = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: key_id.clone(),
    };
    let sign_info = SigningInfo {
        signers: SIGNER_IDS.clone(),
        key_id,
    };
    let states = ctx.sign(message_info.clone(), sign_info.clone()).await;

    let mut c1 = states.sign_phase3.clients[0].clone();

    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::AwaitingLocalSig3
    );

    // send the local_sig from 0->1 back to client 0
    let bad_node = SIGNER_IDS[1].clone();
    let local_sig = states.sign_phase3.local_sigs[0].clone();
    let m = sig_to_p2p(local_sig, &bad_node, &message_info);
    c1.process_p2p_message(m);

    // make sure we the signing is abandoned
    assert_eq!(
        c1.signing_manager
            .get_state_for(&message_info)
            .unwrap()
            .get_stage(),
        SigningStage::Abandoned
    );

    let mut rx = &mut ctx.rxs[0];

    // check that we got the 'invalid' signal
    assert_eq!(
        helpers::recv_next_inner_event(&mut rx).await,
        InnerEvent::SigningResult(SigningOutcome::invalid(
            message_info.clone(),
            vec![bad_node]
        ))
    );

    c1.set_timeout(Duration::from_secs(0));
    c1.cleanup();

    // check that the signing was cleaned up
    assert!(c1.signing_manager.get_state_for(&message_info).is_none());

    // make sure the timeout is not triggered for the abandoned signing
    assert_eq!(helpers::check_for_inner_event(&mut rx).await, None);
}
