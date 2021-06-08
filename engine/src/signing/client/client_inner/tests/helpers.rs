use std::{collections::HashMap, time::Duration};

use log::{error, info};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    p2p::{P2PMessage, P2PMessageCommand},
    signing::{
        client::{
            client_inner::{
                client_inner::{
                    Broadcast1, KeyGenMessage, MultisigMessage, Secret2, SigningData,
                    SigningDataWrapper,
                },
                keygen_state::KeygenStage,
                signing_state::SigningStage,
                InnerEvent, InnerSignal, MultisigClientInner,
            },
            MultisigInstruction,
        },
        crypto::{LocalSig, Parameters},
    },
};

use lazy_static::lazy_static;

/// Clients generated bc1, but haven't sent them
pub(super) struct KeygenPhase1Data {
    pub(super) clients: Vec<MultisigClientInner>,
    pub(super) bc1_vec: Vec<Broadcast1>,
}

/// Clients generated sec2, but haven't sent them
pub(super) struct KeygenPhase2Data {
    pub(super) clients: Vec<MultisigClientInner>,
    /// The key in the map is the index of the desitnation node
    pub(super) sec2_vec: Vec<HashMap<usize, Secret2>>,
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
    pub(super) sec2_vec: Vec<HashMap<usize, Secret2>>,
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
    pub(super) sign_phase1: SigningPhase1Data,
    pub(super) sign_phase2: SigningPhase2Data,
    pub(super) sign_phase3: SigningPhase3Data,
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

pub(super) async fn generate_valid_keygen_data() -> ValidKeygenStates {
    let instant = std::time::Instant::now();

    let params = Parameters {
        threshold: 1,
        share_count: 3,
    };

    let (mut clients, mut rxs): (Vec<_>, Vec<_>) = (1..=3)
        .map(|idx| {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let c = MultisigClientInner::new(idx, params, tx, TEST_PHASE_TIMEOUT);
            (c, rx)
        })
        .unzip();

    // Generate phase 1 data

    for c in &mut clients {
        c.process_multisig_instruction(MultisigInstruction::KeyGen);
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

    for sender_idx in 1..=3 {
        let bc1 = bc1_vec[sender_idx - 1].clone();
        let m = bc1_to_p2p_keygen(bc1, sender_idx);

        for receiver_idx in 1..=3 {
            if receiver_idx != sender_idx {
                clients[receiver_idx - 1].process_p2p_mq_message(m.clone());
            }
        }
    }

    for c in &clients {
        assert_eq!(c.keygen_state.stage, KeygenStage::AwaitingSecret2);
    }

    let mut sec2_vec = vec![];

    for rx in &mut rxs {
        let mut sec2_map = HashMap::new();

        // Should generate two messages
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

            let r_idx = receiver_idx + 1;
            let sec2 = sec2_vec[sender_idx].get(&r_idx).unwrap();

            let m = sec2_to_p2p_keygen(sec2.clone(), sender_idx + 1);

            clients[receiver_idx].process_p2p_mq_message(m);
        }
    }

    for r in &mut rxs {
        assert_eq!(
            Some(InnerEvent::InnerSignal(InnerSignal::KeyReady)),
            r.recv().await
        );
    }

    // *** Send a request to sign and generate BC1 to be distributed ***

    let message_to_sign = "Chainflip".as_bytes().to_vec();

    // NOTE: only parties 1 and 2 will participate in signing
    let active_parties = vec![1, 2];

    for idx in &active_parties {
        let c = &mut clients[idx - 1];

        c.process_multisig_instruction(MultisigInstruction::Sign(
            message_to_sign.clone(),
            active_parties.clone(),
        ));

        assert_eq!(
            c.signing_manager
                .get_state_for(&message_to_sign)
                .unwrap()
                .get_stage(),
            SigningStage::AwaitingBroadcast1
        );
    }

    let mut bc1_vec = vec![];

    for idx in &active_parties {
        let rx = &mut rxs[idx - 1];

        let bc1 = recv_bc1_signing(rx).await;
        bc1_vec.push(bc1);
    }

    let sign_phase1 = SigningPhase1Data {
        clients: clients.clone(),
        bc1_vec: bc1_vec.clone(),
    };

    assert_channel_empty(&mut rxs[0]).await;

    // *** Broadcast BC1 messages to advance to Phase2 ***

    for sender_idx in &active_parties {
        let bc1 = bc1_vec[sender_idx - 1].clone();

        let m = bc1_to_p2p_signing(bc1, *sender_idx, &message_to_sign);

        for receiver_idx in &active_parties {
            if receiver_idx != sender_idx {
                clients[receiver_idx - 1].process_p2p_mq_message(m.clone());
            }
        }
    }

    // *** Collect Secret2 messages ***

    let mut sec2_vec = vec![];

    for idx in &active_parties {
        let rx = &mut rxs[idx - 1];

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

    for sender_idx in &active_parties {
        for receiver_idx in &active_parties {
            if sender_idx != receiver_idx {
                let sec2 = sec2_vec[sender_idx - 1].get(receiver_idx).unwrap().clone();

                let m = sec2_to_p2p_signing(sec2, *sender_idx, &message_to_sign);

                clients[receiver_idx - 1].process_p2p_mq_message(m);
            }
        }
    }

    for idx in &active_parties {
        let c = &mut clients[idx - 1];
        assert_eq!(
            c.signing_manager
                .get_state_for(&message_to_sign)
                .unwrap()
                .get_stage(),
            SigningStage::AwaitingLocalSig3
        );
    }

    // *** Collect local signatures ***

    let mut local_sigs = vec![];

    for idx in &active_parties {
        let rx = &mut rxs[idx - 1];

        let sig = recv_local_sig(rx).await;
        local_sigs.push(sig);
    }

    assert_channel_empty(&mut rxs[0]).await;

    let sign_phase3 = SigningPhase3Data {
        clients: clients.clone(),
        local_sigs: local_sigs.clone(),
    };

    info!("Elapsed: {}", instant.elapsed().as_millis());

    ValidKeygenStates {
        keygen_phase1,
        keygen_phase2,
        sign_phase1,
        sign_phase2,
        sign_phase3,
        rxs,
    }
}

async fn assert_channel_empty(rx: &mut UnboundedReceiver<InnerEvent>) {
    let fut = rx.recv();
    let dur = std::time::Duration::from_millis(10);

    assert!(tokio::time::timeout(dur, fut).await.is_err());
}

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

pub async fn recv_p2p_message(rx: &mut UnboundedReceiver<InnerEvent>) -> P2PMessageCommand {
    let dur = std::time::Duration::from_millis(10);

    let res = tokio::time::timeout(dur, rx.recv())
        .await
        .ok()
        .expect("timeout")
        .unwrap();

    match res {
        InnerEvent::InnerSignal(_) => {
            error!("Unexpected InnerSignal");
            panic!();
        }
        InnerEvent::P2PMessageCommand(m) => m,
    }
}

async fn recv_multisig_message(rx: &mut UnboundedReceiver<InnerEvent>) -> (usize, MultisigMessage) {
    let m = recv_p2p_message(rx).await;

    (
        m.destination,
        serde_json::from_slice(&m.data).expect("Invalid Multisig Message"),
    )
}

async fn recv_bc1_keygen(rx: &mut UnboundedReceiver<InnerEvent>) -> Broadcast1 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::KeyGenMessage(KeyGenMessage::Broadcast1(bc1)) = m {
        return bc1;
    }

    error!("Received message is not Broadcast1 (keygen)");
    panic!();
}

async fn recv_bc1_signing(rx: &mut UnboundedReceiver<InnerEvent>) -> Broadcast1 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapper { data, .. }) = m {
        if let SigningData::Broadcast1(bc1) = data {
            return bc1;
        }
    }

    error!("Received message is not Broadcast1 (signing)");
    panic!();
}

async fn recv_local_sig(rx: &mut UnboundedReceiver<InnerEvent>) -> LocalSig {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapper { data, .. }) = m {
        if let SigningData::LocalSig(sig) = data {
            return sig;
        }
    }

    error!("Received message is not LocalSig");
    panic!();
}

async fn recv_secret2_keygen(rx: &mut UnboundedReceiver<InnerEvent>) -> (usize, Secret2) {
    let (dest, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::KeyGenMessage(KeyGenMessage::Secret2(sec2)) = m {
        return (dest, sec2);
    }

    error!("Received message is not Secret2 (keygen)");
    panic!();
}

async fn recv_secret2_signing(rx: &mut UnboundedReceiver<InnerEvent>) -> (usize, Secret2) {
    let (dest, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapper { data, .. }) = m {
        if let SigningData::Secret2(sec2) = data {
            return (dest, sec2);
        }
    }

    error!("Received message is not Secret2 (signing)");
    panic!();
}

// Do the necessary wrapping so Secret2 can be sent
// via the clients interface
pub fn sec2_to_p2p_signing(sec2: Secret2, sender_idx: usize, msg: &[u8]) -> P2PMessage {
    let sec2 = SigningData::Secret2(sec2);

    let wrapped = SigningDataWrapper {
        data: sec2,
        message: msg.to_owned(),
    };

    let data = MultisigMessage::SigningMessage(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_idx,
        data,
    }
}

// Do the necessary wrapping so Secret2 can be sent
// via the clients interface
pub fn sec2_to_p2p_keygen(sec2: Secret2, sender_idx: usize) -> P2PMessage {
    let sec2 = KeyGenMessage::Secret2(sec2);

    let data = MultisigMessage::KeyGenMessage(sec2);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_idx,
        data,
    }
}

fn bc1_to_p2p_keygen(bc1: Broadcast1, sender_idx: usize) -> P2PMessage {
    let data = MultisigMessage::KeyGenMessage(KeyGenMessage::Broadcast1(bc1));
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_idx,
        data,
    }
}

pub fn bc1_to_p2p_signing(bc1: Broadcast1, sender_idx: usize, msg: &[u8]) -> P2PMessage {
    let bc1 = SigningData::Broadcast1(bc1);

    let wrapped = SigningDataWrapper {
        data: bc1,
        message: msg.to_owned(),
    };

    let data = MultisigMessage::SigningMessage(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_idx,
        data,
    }
}

pub fn sig_to_p2p(sig: LocalSig, sender_idx: usize, msg: &[u8]) -> P2PMessage {
    let data = SigningData::LocalSig(sig);

    let wrapped = SigningDataWrapper {
        data,
        message: msg.to_owned(),
    };

    let data = MultisigMessage::SigningMessage(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_idx,
        data,
    }
}
