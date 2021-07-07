use std::{collections::HashMap, time::Duration};

use itertools::Itertools;
use log::*;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    p2p::{P2PMessage, P2PMessageCommand, ValidatorId},
    signing::{
        client::{
            client_inner::{
                client_inner::{
                    Broadcast1, KeyGenMessageWrapped, KeygenData, MultisigMessage, Secret2,
                    SigningData, SigningDataWrapped,
                },
                keygen_state::KeygenStage,
                signing_state::SigningStage,
                InnerEvent, InnerSignal, KeygenOutcome, MultisigClientInner,
            },
            KeyId, KeygenInfo, MultisigInstruction, SigningInfo,
        },
        crypto::{Keys, LocalSig, Parameters, Signature},
        MessageHash, MessageInfo,
    },
};

use lazy_static::lazy_static;

use super::{KEY_ID, MESSAGE_HASH, MESSAGE_INFO, SIGNER_IDXS, SIGN_INFO};

/// Clients generated bc1, but haven't sent them
pub(super) struct KeygenPhase1Data {
    pub(super) clients: Vec<MultisigClientInner>,
    pub(super) bc1_vec: Vec<Broadcast1>,
}

/// Clients generated sec2, but haven't sent them
pub(super) struct KeygenPhase2Data {
    pub(super) clients: Vec<MultisigClientInner>,
    /// The key in the map is the index of the desitnation node
    pub(super) sec2_vec: Vec<HashMap<ValidatorId, Secret2>>,
}

pub(super) struct KeygenPhase3Data {
    pub(super) clients: Vec<MultisigClientInner>,
    pub(super) pubkey: secp256k1::PublicKey,
}

/// Clients received a request to sign and generated BC1, not broadcast yet
pub(super) struct SigningPhase1Data {
    pub(super) clients: Vec<MultisigClientInner>,
    pub(super) bc1_vec: Vec<Broadcast1>,
}

/// Clients generated Secret2, not sent yet
pub(super) struct SigningPhase2Data {
    pub(super) clients: Vec<MultisigClientInner>,
    /// The key in the map is the index of the desitnation node
    pub(super) sec2_vec: Vec<HashMap<ValidatorId, Secret2>>,
}

/// Clients generated Secret2, not sent yet
pub(super) struct SigningPhase3Data {
    pub(super) clients: Vec<MultisigClientInner>,
    /// The key in the map is the index of the desitnation node
    pub(super) local_sigs: Vec<LocalSig>,
}

pub(super) struct ValidKeygenStates {
    pub(super) keygen_phase1: KeygenPhase1Data,
    pub(super) keygen_phase2: KeygenPhase2Data,
    pub(super) key_ready: KeygenPhase3Data,
    pub(super) sign_phase1: SigningPhase1Data,
    pub(super) sign_phase2: SigningPhase2Data,
    pub(super) sign_phase3: SigningPhase3Data,
    pub(super) signature: Signature,
    pub(super) rxs: Vec<UnboundedReceiver<InnerEvent>>,
}

lazy_static! {

    pub(super) static ref VALID_KEYGEN_STATES : ValidKeygenStates = {

        // Tokio does not allow nested runtimes, so we use futures' runtime
        // for this one-off future
        futures::executor::block_on(async {generate_valid_keygen_data().await })
    };
}

const TEST_PHASE_TIMEOUT: Duration = Duration::from_secs(5);

pub fn keygen_stage_for(client: &MultisigClientInner, key_id: KeyId) -> Option<KeygenStage> {
    client.get_keygen().get_stage_for(key_id)
}

pub fn keygen_delayed_count(client: &MultisigClientInner, key_id: KeyId) -> usize {
    client.get_keygen().get_delayed_count(key_id)
}

pub fn signing_delayed_count(client: &MultisigClientInner, mi: &MessageInfo) -> usize {
    client.signing_manager.get_delayed_count(mi)
}

pub(super) async fn generate_valid_keygen_data() -> ValidKeygenStates {
    let instant = std::time::Instant::now();

    let params = Parameters {
        threshold: 1,
        share_count: 3,
    };

    let validator_ids = (1..=3).map(|idx| ValidatorId::new(idx)).collect_vec();

    let (mut clients, mut rxs): (Vec<_>, Vec<_>) = validator_ids
        .iter()
        .map(|id| {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let c = MultisigClientInner::new(id.clone(), params, tx, TEST_PHASE_TIMEOUT);
            (c, rx)
        })
        .unzip();

    // Generate phase 1 data

    let key_id = KeyId(0);

    let auction_info = KeygenInfo {
        id: key_id,
        signers: validator_ids.clone(),
    };

    for c in &mut clients {
        c.process_multisig_instruction(MultisigInstruction::KeyGen(auction_info.clone()));
    }

    let mut bc1_vec = vec![];

    for rx in &mut rxs {
        let bc1 = recv_bc1_keygen(rx).await;
        bc1_vec.push(bc1);

        // ignore the next message
        let _ = recv_bc1_keygen(rx).await;
    }

    let phase1_clients = clients.clone();

    // *** Distribute BC1, so we can advance and generate Secret2 ***

    for sender_idx in 0..=2 {
        let bc1 = bc1_vec[sender_idx].clone();
        let id = &validator_ids[sender_idx];
        let m = bc1_to_p2p_keygen(bc1, KEY_ID, id);

        for receiver_idx in 0..=2 {
            if receiver_idx != sender_idx {
                clients[receiver_idx].process_p2p_mq_message(m.clone());
            }
        }
    }

    for c in &clients {
        assert_eq!(
            keygen_stage_for(c, key_id),
            Some(KeygenStage::AwaitingSecret2)
        );
    }

    let mut sec2_vec = vec![];

    for rx in &mut rxs {
        let mut sec2_map = HashMap::new();

        // Should generate two messages (one for each of the other two parties)
        for _ in 0u32..2 {
            let (dest, sec2) = recv_secret2_keygen(rx).await;
            sec2_map.insert(dest, sec2);
        }

        sec2_vec.push(sec2_map);
    }

    let phase2_clients = clients.clone();

    let keygen_phase1 = KeygenPhase1Data {
        clients: phase1_clients,
        bc1_vec,
    };

    let keygen_phase2 = KeygenPhase2Data {
        clients: phase2_clients,
        sec2_vec: sec2_vec.clone(),
    };

    // *** Distribute Secret2s, so we can advance and generate Signing Key ***

    for sender_idx in 0..3 {
        for receiver_idx in 0..3 {
            if sender_idx == receiver_idx {
                continue;
            }

            let r_id = &validator_ids[receiver_idx];
            let sec2 = sec2_vec[sender_idx].get(r_id).unwrap();

            let s_id = &validator_ids[sender_idx];
            let m = sec2_to_p2p_keygen(sec2.clone(), s_id);

            clients[receiver_idx].process_p2p_mq_message(m);
        }
    }

    let pubkey = match recv_next_inner_event(&mut rxs[0]).await {
        InnerEvent::KeygenResult(KeygenOutcome::Success(key_data)) => key_data.key,
        _ => panic!("Unexpected inner event"),
    };

    for r in &mut rxs {
        assert_eq!(
            recv_next_signal_message_skipping(r).await,
            Some(InnerSignal::KeyReady)
        );
    }

    let keygen_phase3 = KeygenPhase3Data {
        clients: clients.clone(),
        pubkey,
    };

    // *** Send a request to sign and generate BC1 to be distributed ***

    // NOTE: only parties 1 and 2 will participate in signing (SIGNER_IDXS)
    for idx in SIGNER_IDXS.iter() {
        let c = &mut clients[*idx];

        c.process_multisig_instruction(MultisigInstruction::Sign(
            MESSAGE_HASH.clone(),
            SIGN_INFO.clone(),
        ));

        assert_eq!(
            c.signing_manager
                .get_state_for(&MESSAGE_INFO)
                .unwrap()
                .get_stage(),
            SigningStage::AwaitingBroadcast1
        );
    }

    let mut bc1_vec = vec![];

    for idx in SIGNER_IDXS.iter() {
        let rx = &mut rxs[*idx];

        let bc1 = recv_bc1_signing(rx).await;
        bc1_vec.push(bc1);
    }

    let sign_phase1 = SigningPhase1Data {
        clients: clients.clone(),
        bc1_vec: bc1_vec.clone(),
    };

    assert_channel_empty(&mut rxs[0]).await;

    // *** Broadcast BC1 messages to advance to Phase2 ***
    for sender_idx in SIGNER_IDXS.iter() {
        let bc1 = bc1_vec[*sender_idx].clone();
        let id = &validator_ids[*sender_idx];

        let m = bc1_to_p2p_signing(bc1, id, &MESSAGE_INFO);

        for receiver_idx in SIGNER_IDXS.iter() {
            if receiver_idx != sender_idx {
                clients[*receiver_idx].process_p2p_mq_message(m.clone());
            }
        }
    }

    // *** Collect Secret2 messages ***

    let mut sec2_vec = vec![];

    for idx in SIGNER_IDXS.iter() {
        let rx = &mut rxs[*idx];

        let mut sec2_map = HashMap::new();

        let (dest, sec2) = recv_secret2_signing(rx).await;

        sec2_map.insert(dest, sec2);

        sec2_vec.push(sec2_map);
    }

    assert_channel_empty(&mut rxs[0]).await;

    assert_eq!(sec2_vec.len(), 2);
    assert_eq!(sec2_vec[0].len(), 1);
    assert_eq!(sec2_vec[1].len(), 1);

    let sign_phase2 = SigningPhase2Data {
        clients: clients.clone(),
        sec2_vec: sec2_vec.clone(),
    };

    // *** Distribute Secret2 messages ***

    for sender_idx in SIGNER_IDXS.iter() {
        for receiver_idx in SIGNER_IDXS.iter() {
            if sender_idx != receiver_idx {
                let receiver_id = &validator_ids[*receiver_idx];

                let sec2 = sec2_vec[*sender_idx].get(receiver_id).unwrap().clone();

                let id = &validator_ids[*sender_idx];
                let m = sec2_to_p2p_signing(sec2, id, &MESSAGE_INFO);

                clients[*receiver_idx].process_p2p_mq_message(m);
            }
        }
    }

    for idx in SIGNER_IDXS.iter() {
        let c = &mut clients[*idx];
        assert_eq!(
            c.signing_manager
                .get_state_for(&MESSAGE_INFO)
                .unwrap()
                .get_stage(),
            SigningStage::AwaitingLocalSig3
        );
    }

    // *** Collect local signatures ***

    let mut local_sigs = vec![];

    for idx in SIGNER_IDXS.iter() {
        let rx = &mut rxs[*idx];

        let sig = recv_local_sig(rx).await;
        local_sigs.push(sig);
    }

    assert_channel_empty(&mut rxs[0]).await;

    let sign_phase3 = SigningPhase3Data {
        clients: clients.clone(),
        local_sigs: local_sigs.clone(),
    };

    for sender_idx in SIGNER_IDXS.iter() {
        let local_sig = local_sigs[*sender_idx].clone();
        let id = &validator_ids[*sender_idx];

        let m = sig_to_p2p(local_sig, id, &MESSAGE_INFO);

        for receiver_idx in SIGNER_IDXS.iter() {
            if receiver_idx != sender_idx {
                clients[*receiver_idx].process_p2p_mq_message(m.clone());
            }
        }
    }

    let event = recv_next_inner_event(&mut rxs[0]).await;

    let signature = match event {
        InnerEvent::InnerSignal(InnerSignal::MessageSigned(_message_info, sig)) => sig,
        _ => panic!("Unexpected event"),
    };

    info!("generate_valid_keygen_data took: {:?}", instant.elapsed());

    ValidKeygenStates {
        keygen_phase1,
        keygen_phase2,
        key_ready: keygen_phase3,
        sign_phase1,
        sign_phase2,
        sign_phase3,
        signature,
        rxs,
    }
}

pub async fn assert_channel_empty(rx: &mut UnboundedReceiver<InnerEvent>) {
    let fut = rx.recv();
    let dur = std::time::Duration::from_millis(10);

    assert!(tokio::time::timeout(dur, fut).await.is_err());
}

#[allow(dead_code)]
pub async fn print_next_message(rx: &mut UnboundedReceiver<InnerEvent>) {
    let dur = std::time::Duration::from_millis(10);

    let future = async {
        let m = rx.recv().await.unwrap();

        match m {
            InnerEvent::P2PMessageCommand(P2PMessageCommand { destination, .. }) => {
                eprintln!("P2PMessageCommand [ destination: {} ]", destination);
            }
            InnerEvent::InnerSignal(s) => {
                eprintln!("{:?}", s);
            }
            InnerEvent::KeygenResult(res) => {
                eprintln!("{:?}", res);
            }
        }
    };

    match tokio::time::timeout(dur, future).await {
        Err(err) => {
            eprintln!("Timeout: {}", err);
        }
        _ => {}
    }
}

/// Skip all non-signal messages
pub async fn recv_next_signal_message_skipping(
    rx: &mut UnboundedReceiver<InnerEvent>,
) -> Option<InnerSignal> {
    let dur = std::time::Duration::from_millis(10);

    loop {
        let res = tokio::time::timeout(dur, rx.recv()).await.ok()??;

        if let InnerEvent::InnerSignal(s) = res {
            return Some(s);
        }
    }
}

/// Asserts that InnerEvent is in the queue and returns it
pub async fn recv_next_inner_event(rx: &mut UnboundedReceiver<InnerEvent>) -> InnerEvent {
    let dur = std::time::Duration::from_millis(10);

    let res = tokio::time::timeout(dur, rx.recv())
        .await
        .ok()
        .expect("timeout");

    if let Some(event) = res {
        return event;
    }
    panic!("Expected Inner Event");
}

pub async fn recv_p2p_message(rx: &mut UnboundedReceiver<InnerEvent>) -> P2PMessageCommand {
    let dur = std::time::Duration::from_millis(10);

    let res = tokio::time::timeout(dur, rx.recv())
        .await
        .ok()
        .expect("timeout")
        .unwrap();

    match res {
        InnerEvent::P2PMessageCommand(m) => m,
        _ => {
            error!("Unexpected InnerEvent");
            panic!();
        }
    }
}

async fn recv_multisig_message(
    rx: &mut UnboundedReceiver<InnerEvent>,
) -> (ValidatorId, MultisigMessage) {
    let m = recv_p2p_message(rx).await;

    (
        m.destination,
        serde_json::from_slice(&m.data).expect("Invalid Multisig Message"),
    )
}

async fn recv_bc1_keygen(rx: &mut UnboundedReceiver<InnerEvent>) -> Broadcast1 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::KeyGenMessage(wrapped) = m {
        let KeyGenMessageWrapped { message, .. } = wrapped;

        if let KeygenData::Broadcast1(bc1) = message {
            return bc1;
        }
    }

    error!("Received message is not Broadcast1 (keygen)");
    panic!();
}

async fn recv_bc1_signing(rx: &mut UnboundedReceiver<InnerEvent>) -> Broadcast1 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::Broadcast1(bc1) = data {
            return bc1;
        }
    }

    error!("Received message is not Broadcast1 (signing)");
    panic!();
}

async fn recv_local_sig(rx: &mut UnboundedReceiver<InnerEvent>) -> LocalSig {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::LocalSig(sig) = data {
            return sig;
        }
    }

    error!("Received message is not LocalSig");
    panic!();
}

async fn recv_secret2_keygen(rx: &mut UnboundedReceiver<InnerEvent>) -> (ValidatorId, Secret2) {
    let (dest, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::KeyGenMessage(wrapped) = m {
        let KeyGenMessageWrapped { message, .. } = wrapped;

        if let KeygenData::Secret2(sec2) = message {
            return (dest, sec2);
        }
    }

    error!("Received message is not Secret2 (keygen)");
    panic!();
}

async fn recv_secret2_signing(rx: &mut UnboundedReceiver<InnerEvent>) -> (ValidatorId, Secret2) {
    let (dest, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::Secret2(sec2) = data {
            return (dest, sec2);
        }
    }

    error!("Received message is not Secret2 (signing)");
    panic!();
}

// Do the necessary wrapping so Secret2 can be sent
// via the clients interface
pub fn sec2_to_p2p_signing(sec2: Secret2, sender_id: &ValidatorId, mi: &MessageInfo) -> P2PMessage {
    let wrapped = SigningDataWrapped::new(sec2, mi.clone());

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

// Do the necessary wrapping so Secret2 can be sent
// via the clients interface
pub fn sec2_to_p2p_keygen(sec2: Secret2, sender_id: &ValidatorId) -> P2PMessage {
    let wrapped = KeyGenMessageWrapped::new(KEY_ID, sec2);

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn bc1_to_p2p_keygen(bc1: Broadcast1, key_id: KeyId, sender_id: &ValidatorId) -> P2PMessage {
    let wrapped = KeyGenMessageWrapped::new(key_id, bc1);

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn bc1_to_p2p_signing(
    bc1: Broadcast1,
    sender_id: &ValidatorId,
    mi: &MessageInfo,
) -> P2PMessage {
    let bc1 = SigningData::Broadcast1(bc1);

    let wrapped = SigningDataWrapped::new(bc1, mi.clone());

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn sig_to_p2p(sig: LocalSig, sender_id: &ValidatorId, mi: &MessageInfo) -> P2PMessage {
    let wrapped = SigningDataWrapped::new(sig, mi.clone());

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn create_keygen_p2p_message<M>(sender_id: &ValidatorId, message: M) -> P2PMessage
where
    M: Into<KeygenData>,
{
    let wrapped = KeyGenMessageWrapped::new(KEY_ID, message.into());

    let ms_message = MultisigMessage::from(wrapped);

    let data = serde_json::to_vec(&ms_message).unwrap();

    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub(super) fn get_stage_for_msg(
    c: &MultisigClientInner,
    message_info: &MessageInfo,
) -> Option<SigningStage> {
    c.signing_manager
        .get_state_for(message_info)
        .map(|s| s.get_stage())
}

pub fn create_bc1(signer_idx: usize) -> Broadcast1 {
    let key = Keys::phase1_create(signer_idx);

    let (bc1, blind) = key.phase1_broadcast();

    let y_i = key.y_i;

    Broadcast1 { bc1, blind, y_i }
}

pub fn create_invalid_bc1() -> Broadcast1 {
    let key = Keys::phase1_create(0);

    let key2 = Keys::phase1_create(0);

    let (_, blind) = key.phase1_broadcast();

    let (bc1, _) = key2.phase1_broadcast();

    let y_i = key.y_i;

    Broadcast1 { bc1, blind, y_i }
}
