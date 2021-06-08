mod helpers;

use tokio::sync::mpsc::UnboundedReceiver;

use lazy_static::lazy_static;

use crate::{
    p2p::{P2PMessage, ValidatorId},
    signing::{
        client::{
            client_inner::{
                keygen_state::KeygenStage,
                signing_state::SigningStage,
                tests::helpers::{
                    bc1_to_p2p_signing, generate_valid_keygen_data,
                    recv_next_signal_message_skipping, sec2_to_p2p_keygen, sec2_to_p2p_signing,
                    sig_to_p2p,
                },
                InnerSignal,
            },
            MultisigInstruction, PHASE_TIMEOUT,
        },
        crypto::{Keys, Parameters},
    },
};

use super::{
    client_inner::{KeyGenMessage, MultisigClientInner, MultisigMessage, SigningDataWrapper},
    InnerEvent,
};

fn create_bc1(signer_idx: usize) -> Broadcast1 {
    let key = Keys::phase1_create(signer_idx);

    let (bc1, blind) = key.phase1_broadcast();

    let y_i = key.y_i;

    // Q: can we distribute bc1 and blind at the same time?
    Broadcast1 { bc1, blind, y_i }
}

use std::{sync::Once, time::Duration};

use super::{client_inner::Broadcast1, signing_state_manager::SigningStateManager};

static INIT: Once = Once::new();

/// Initializes the logger and does only once
/// (doing otherwise would result in error)
fn init_logs_once() {
    INIT.call_once(|| {
        env_logger::builder()
            .format_timestamp(None)
            .format_module_path(false)
            .init();
    })
}

struct TestContext {
    ssm: SigningStateManager,
    _event_receiver: UnboundedReceiver<InnerEvent>,
}

impl TestContext {
    fn new(t: usize, n: usize) -> Self {
        let params = Parameters {
            threshold: t,
            share_count: n,
        };

        let signer_idx = 1;

        let (p2p_sender, p2p_receiver) = tokio::sync::mpsc::unbounded_channel();

        let phase_timeout = std::time::Duration::from_secs(10);

        let ssm = SigningStateManager::new(params, signer_idx, p2p_sender, phase_timeout);

        TestContext {
            ssm,
            _event_receiver: p2p_receiver,
        }
    }
}

/// After we've received a request to sign, we should immediately be able
/// to receive Broadcast1 messages
#[test]
fn should_await_bc1_after_rts() {
    init_logs_once();

    let mut ctx = TestContext::new(1, 2);

    let msg = "Message".as_bytes().to_vec();

    ctx.ssm.on_request_to_sign(msg.clone(), &[1, 2]);

    let state = ctx.ssm.get_state_for(&msg).unwrap();

    assert_eq!(state.get_stage(), SigningStage::AwaitingBroadcast1);
}

/// BC1 messages get processed if we receive RTS shortly after
#[test]
fn should_process_delayed_bc1_after_rts() {
    init_logs_once();

    let ctx = TestContext::new(1, 2);

    let mut ssm = ctx.ssm;

    let msg = "Message".as_bytes().to_vec();

    assert!(ssm.get_state_for(&msg).is_none());

    let signer_idx = 2;
    let data = create_bc1(signer_idx).into();

    let wdata = SigningDataWrapper {
        data,
        message: msg.clone(),
    };

    ssm.maybe_process_signing_data(signer_idx, wdata);

    let state = ssm.get_state_for(&msg);

    assert!(state.is_some());

    let state = state.unwrap();

    assert_eq!(state.get_stage(), SigningStage::Idle);

    assert_eq!(state.delayed_data.len(), 1);

    ssm.on_request_to_sign(msg.clone(), &[1, 2]);

    let state = ssm.get_state_for(&msg).unwrap();

    assert_eq!(state.delayed_data.len(), 0);
    // Already at Phase2, because share_count is 2
    assert_eq!(state.get_stage(), SigningStage::AwaitingSecret2);
}

/// Test that we don't proceed with the protocol
/// unless we've received a request to sign the
/// corresponding message (even if we've received
/// t+1 components from other parties)
#[test]
fn sign_request_is_required_to_proceed() {
    let ctx = TestContext::new(1, 3);
    let mut ssm = ctx.ssm;
    let msg = "Message".as_bytes().to_vec();

    {
        let signer_idx = 2;
        let data = create_bc1(signer_idx).into();
        let wdata = SigningDataWrapper {
            data,
            message: msg.clone(),
        };

        ssm.maybe_process_signing_data(signer_idx, wdata);
    }

    {
        let signer_idx = 3;
        let data = create_bc1(signer_idx).into();
        let wdata = SigningDataWrapper {
            data,
            message: msg.clone(),
        };

        ssm.maybe_process_signing_data(signer_idx, wdata);
    }

    let state = ssm.get_state_for(&msg).unwrap();

    assert_eq!(state.delayed_data.len(), 2);
    assert_eq!(state.get_stage(), SigningStage::Idle);
}

#[test]
#[ignore = "unimplemented"]
fn signing_data_expire() {
    todo!();
}

fn create_keygen_p2p_message(sender_id: ValidatorId, data: KeyGenMessage) -> P2PMessage {
    let ms_message = MultisigMessage::KeyGenMessage(data);

    let data = serde_json::to_vec(&ms_message).unwrap();

    P2PMessage { sender_id, data }
}

#[test]
fn bc1_gets_delayed_until_keygen_request() {
    let params = Parameters {
        threshold: 1,
        share_count: 3,
    };

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let mut client = MultisigClientInner::new(1, params, tx, PHASE_TIMEOUT);

    assert_eq!(client.keygen_state.stage, KeygenStage::Uninitialized);

    let data = create_bc1(2).into();
    let message = create_keygen_p2p_message(2, data);
    client.process_p2p_mq_message(message);

    assert_eq!(client.keygen_state.stage, KeygenStage::Uninitialized);

    assert_eq!(client.keygen_state.delayed_next_stage_data.len(), 1);

    // Keygen instruction should advance the stage and process delayed messages

    let keygen = MultisigInstruction::KeyGen;

    client.process_multisig_instruction(keygen);

    assert_eq!(client.keygen_state.stage, KeygenStage::AwaitingBroadcast1);
    assert_eq!(client.keygen_state.delayed_next_stage_data.len(), 0);

    // One more message should advance the stage (share_count = 3)
    let data = create_bc1(3).into();
    let message = create_keygen_p2p_message(3, data);
    client.process_p2p_mq_message(message);

    assert_eq!(client.keygen_state.stage, KeygenStage::AwaitingSecret2);
}

/// By sending (signing) BC1, a node is trying to start a signing procedure,
/// but we only process it after we've received a signing instruction from
/// our SC. If we don't receive it after a certain period of time, BC1 should
/// be removed and the sender should be penalised.
#[test]
fn delayed_signing_bc1_gets_removed() {
    init_logs_once();
    // Setup
    let params = Parameters {
        threshold: 1,
        share_count: 3,
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let timeout = Duration::from_millis(1);

    let mut client = MultisigClientInner::new(1, params, tx, timeout);

    // Create delayed BC1
    let bc1 = create_bc1(2).into();
    let m = bc1_to_p2p_signing(bc1, 2, &MESSAGE);
    client.process_p2p_mq_message(m);

    assert_eq!(
        get_stage_for_msg(&client, &MESSAGE),
        Some(SigningStage::Idle)
    );

    // Wait for the data to expire
    std::thread::sleep(timeout);

    client.cleanup();

    assert_eq!(get_stage_for_msg(&client, &MESSAGE), None);
}

#[tokio::test]
async fn keygen_secret2_gets_delayed() {
    init_logs_once();

    let states = generate_valid_keygen_data().await;

    let phase1 = &states.keygen_phase1;
    let phase2 = &states.keygen_phase2;

    // Note the use of phase2 data on a phase1 client
    let mut clients_p1 = phase1.clients.clone();
    let bc1_vec = phase1.bc1_vec.clone();
    let sec2_vec = phase2.sec2_vec.clone();

    let c1 = &mut clients_p1[0];
    assert_eq!(c1.keygen_state.stage, KeygenStage::AwaitingBroadcast1);

    // Secret sent from client 2 to client 1
    let sec2 = sec2_vec[1].get(&1).unwrap().clone();

    // We should not process it immediately
    let message = create_keygen_p2p_message(2, sec2.into());

    c1.process_p2p_mq_message(message);

    assert_eq!(c1.keygen_state.delayed_next_stage_data.len(), 1);
    assert_eq!(c1.keygen_state.stage, KeygenStage::AwaitingBroadcast1);

    // Process incoming bc1_vec, so we can advance to the next phase
    let data = bc1_vec[1].clone().into();
    let message = create_keygen_p2p_message(2, data);
    c1.process_p2p_mq_message(message);

    let data = bc1_vec[2].clone().into();
    let message = create_keygen_p2p_message(3, data);
    c1.process_p2p_mq_message(message);

    assert_eq!(c1.keygen_state.stage, KeygenStage::AwaitingSecret2);
    assert_eq!(c1.keygen_state.delayed_next_stage_data.len(), 0);
}

lazy_static! {
    static ref MESSAGE: Vec<u8> = "Chainflip".as_bytes().to_vec();
}

#[tokio::test]
async fn signing_secret2_gets_delayed() {
    init_logs_once();

    let states = generate_valid_keygen_data().await;

    let phase1 = &states.sign_phase1;
    let phase2 = &states.sign_phase2;

    // Client in phase1 should be able to receive phase2 data (Secret2)

    let mut c1 = phase1.clients[0].clone();

    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE),
        Some(SigningStage::AwaitingBroadcast1)
    );

    let sec2 = phase2.sec2_vec[1].get(&1).unwrap().clone();

    let m = sec2_to_p2p_signing(sec2, 2, &MESSAGE);

    c1.process_p2p_mq_message(m);

    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE),
        Some(SigningStage::AwaitingBroadcast1)
    );

    // Finally c1 receives bc1 and able to advance to phase2
    let bc1 = phase1.bc1_vec[1].clone();

    let m = bc1_to_p2p_signing(bc1, 2, &MESSAGE);

    c1.process_p2p_mq_message(m);

    // We are able to process delayed secret2 and immediately
    // go from phase1 to phase3
    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE),
        Some(SigningStage::AwaitingLocalSig3)
    );
}

#[tokio::test]
async fn signing_local_sig_gets_delayed() {
    init_logs_once();

    let mut states = generate_valid_keygen_data().await;

    let phase2 = &states.sign_phase2;
    let phase3 = &states.sign_phase3;

    let mut c1_p2 = phase2.clients[0].clone();
    let local_sig = phase3.local_sigs[1].clone();

    let m = sig_to_p2p(local_sig, 2, &MESSAGE);

    c1_p2.process_p2p_mq_message(m);

    assert_eq!(
        get_stage_for_msg(&c1_p2, &MESSAGE),
        Some(SigningStage::AwaitingSecret2)
    );

    // Send Secret2 to be able to process delayed LocalSig
    let sec2 = phase2.sec2_vec[1].get(&1).unwrap().clone();

    let m = sec2_to_p2p_signing(sec2, 2, &MESSAGE);

    c1_p2.process_p2p_mq_message(m);

    let s = recv_next_signal_message_skipping(&mut states.rxs[0]).await;

    assert_eq!(Some(InnerSignal::MessageSigned(MESSAGE.clone())), s);
}

fn get_stage_for_msg(c: &MultisigClientInner, msg: &[u8]) -> Option<SigningStage> {
    c.signing_manager.get_state_for(msg).map(|s| s.get_stage())
}

/// Request to sign should be delayed until the key is ready
#[tokio::test]
async fn request_to_sign_before_key_ready() {
    init_logs_once();

    let states = generate_valid_keygen_data().await;

    let mut c1 = states.keygen_phase2.clients[0].clone();

    assert_eq!(c1.keygen_state.stage, KeygenStage::AwaitingSecret2);

    // BC1 for siging arrives before the key is ready
    let bc1_sign = states.sign_phase1.bc1_vec[1].clone();

    let m = bc1_to_p2p_signing(bc1_sign, 2, &MESSAGE);

    c1.process_p2p_mq_message(m);

    assert_eq!(get_stage_for_msg(&c1, &MESSAGE), Some(SigningStage::Idle));

    // Finalize key generation and make sure we can make progress on signing the message

    let sec2_1 = states.keygen_phase2.sec2_vec[1].get(&1).unwrap().clone();
    let m = sec2_to_p2p_keygen(sec2_1, 2);
    c1.process_p2p_mq_message(m);

    let sec2_2 = states.keygen_phase2.sec2_vec[2].get(&1).unwrap().clone();
    let m = sec2_to_p2p_keygen(sec2_2, 3);
    c1.process_p2p_mq_message(m);

    assert_eq!(c1.keygen_state.stage, KeygenStage::KeyReady);

    assert_eq!(get_stage_for_msg(&c1, &MESSAGE), Some(SigningStage::Idle));

    c1.process_multisig_instruction(MultisigInstruction::Sign(MESSAGE.clone(), vec![1, 2]));

    // We only need one BC1 (the delayed one) to proceed
    assert_eq!(
        get_stage_for_msg(&c1, &MESSAGE),
        Some(SigningStage::AwaitingSecret2)
    );
}

// 

// What needs to be tested (unit tests)
// DONE:
// - Delaying works correctly for Keygen::BC1, Keygen::Secret2, Signing:BC1, Signing::Secret2, Signing::LocalSig
// - BC1 messages are processed after a timely RTS (and can lead to phase 2)
// - RTS is required to proceed to the next phase

// TO DO:
// - Delayed data expires on timeout
// - Signing phases do timeout (only tested for BC1 currently)
// - Parties cannot send two messages for the same phase of signing/keygen
// - When unable to make progress, the state (Signing/Keygen) should be correctly reset
// (i.e. past failures don't impact future signing ceremonies)
// - Should be able to generate new signing keys
// - make sure that we don't process p2p data at index signer_id which is our own
