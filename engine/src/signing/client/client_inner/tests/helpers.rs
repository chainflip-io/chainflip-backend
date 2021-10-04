use std::{collections::HashMap, fmt::Debug, time::Duration};

use itertools::Itertools;
use log::*;
use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    logging,
    p2p::{AccountId, P2PMessage, P2PMessageCommand},
    signing::{
        client::{
            client_inner::{
                client_inner::{
                    Broadcast1, KeyGenMessageWrapped, KeygenData, MultisigMessage,
                    SchnorrSignature, Secret2, SigningData, SigningDataWrapped,
                },
                common::KeygenResultInfo,
                keygen_state::KeygenStage,
                signing_state::SigningStage,
                InnerEvent, KeygenOutcome, MultisigClientInner, SigningOutcome,
            },
            KeyId, KeygenInfo, MessageHash, MultisigInstruction,
        },
        crypto::{Keys, LocalSig},
        MessageInfo,
    },
    signing::{db::KeyDBMock, SigningInfo},
};

type MultisigClientInnerNoDB = MultisigClientInner<KeyDBMock>;

use super::{CEREMONY_ID, MESSAGE_HASH, SIGNER_IDS, SIGNER_IDXS};

type InnerEventReceiver = UnboundedReceiver<InnerEvent>;

/// Clients generated bc1, but haven't sent them
pub struct KeygenPhase1Data {
    pub clients: Vec<MultisigClientInnerNoDB>,
    pub bc1_vec: Vec<Broadcast1>,
}

/// Clients generated sec2, but haven't sent them
pub struct KeygenPhase2Data {
    pub clients: Vec<MultisigClientInnerNoDB>,
    /// The key in the map is the index of the desitnation node
    pub sec2_vec: Vec<HashMap<AccountId, Secret2>>,
}

pub struct KeygenPhase3Data {
    pub clients: Vec<MultisigClientInnerNoDB>,
    pub pubkey: secp256k1::PublicKey,

    // These are indexed by signer_idx ( -1 )
    pub sec_keys: Vec<KeygenResultInfo>,
}

impl Debug for KeygenPhase3Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeygenPhase3Data")
            .field("pubkey", &self.pubkey)
            .finish()
    }
}

/// Clients received a request to sign and generated BC1, not broadcast yet
pub struct SigningPhase1Data {
    pub clients: Vec<MultisigClientInnerNoDB>,
    pub bc1_vec: Vec<Broadcast1>,
}

/// Clients generated Secret2, not sent yet
pub struct SigningPhase2Data {
    pub clients: Vec<MultisigClientInnerNoDB>,
    /// The key in the map is the index of the desitnation node
    pub sec2_vec: Vec<HashMap<AccountId, Secret2>>,
}

/// Clients generated Secret2, not sent yet
pub struct SigningPhase3Data {
    pub clients: Vec<MultisigClientInnerNoDB>,
    /// The key in the map is the index of the desitnation node
    pub local_sigs: Vec<LocalSig>,
}

pub struct ValidKeygenStates {
    pub keygen_phase1: KeygenPhase1Data,
    pub keygen_phase2: KeygenPhase2Data,
    pub key_ready: KeygenPhase3Data,
}

pub struct ValidSigningStates {
    pub sign_phase1: SigningPhase1Data,
    pub sign_phase2: SigningPhase2Data,
    pub sign_phase3: SigningPhase3Data,
    pub signature: SchnorrSignature,
}

const TEST_PHASE_TIMEOUT: Duration = Duration::from_secs(5);

pub fn keygen_stage_for(
    client: &MultisigClientInnerNoDB,
    ceremony_id: CeremonyId,
) -> Option<KeygenStage> {
    client.get_keygen().get_stage_for(ceremony_id)
}

pub fn keygen_delayed_count(client: &MultisigClientInnerNoDB, ceremony_id: CeremonyId) -> usize {
    client.get_keygen().get_delayed_count(ceremony_id)
}

pub fn signing_delayed_count(client: &MultisigClientInnerNoDB, mi: &MessageInfo) -> usize {
    client.signing_manager.get_delayed_count(mi)
}

/// Contains the states at different points of key generation
/// including the final state, where the key is created
pub struct KeygenContext {
    account_ids: Vec<AccountId>,

    pub rxs: Vec<InnerEventReceiver>,
    /// This clients will match the ones in `key_ready`,
    /// but stored separately so we could substitute
    /// them in more advanced tests
    clients: Vec<MultisigClientInnerNoDB>,
}

impl KeygenContext {
    /// Generate context without starting the
    /// keygen ceremony
    pub fn new() -> Self {
        let account_ids = (1..=3).map(|idx| AccountId([idx; 32])).collect_vec();
        KeygenContext::inner_new(account_ids)
    }

    pub fn new_with_account_ids(account_ids: Vec<AccountId>) -> Self {
        KeygenContext::inner_new(account_ids)
    }

    fn inner_new(account_ids: Vec<AccountId>) -> Self {
        let logger = logging::test_utils::create_test_logger();
        let (clients, rxs): (Vec<_>, Vec<_>) = account_ids
            .iter()
            .map(|id| {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let c = MultisigClientInner::new(
                    id.clone(),
                    KeyDBMock::new(),
                    tx,
                    TEST_PHASE_TIMEOUT,
                    &logger,
                );
                (c, rx)
            })
            .unzip();

        KeygenContext {
            account_ids,
            rxs,
            clients,
        }
    }

    pub fn get_client(&self, idx: usize) -> &MultisigClientInnerNoDB {
        &self.clients[idx]
    }

    // Generate keygen states for each of the phases,
    // resulting in `KeygenContext` which can be used
    // to sign messages
    pub async fn generate(&mut self) -> ValidKeygenStates {
        let instant = std::time::Instant::now();

        let clients = &mut self.clients;
        let account_ids = &self.account_ids;
        let rxs = &mut self.rxs;

        // Generate phase 1 data

        let keygen_info = KeygenInfo {
            ceremony_id: *CEREMONY_ID,
            signers: account_ids.clone(),
        };

        for c in clients.iter_mut() {
            c.process_multisig_instruction(MultisigInstruction::KeyGen(keygen_info.clone()));
        }

        let mut bc1_vec = vec![];

        for rx in rxs.iter_mut() {
            let bc1 = recv_bc1_keygen(rx).await;
            bc1_vec.push(bc1);

            // ignore the next message
            let _ = recv_bc1_keygen(rx).await;
        }

        let phase1_clients = clients.clone();

        // *** Distribute BC1, so we can advance and generate Secret2 ***

        for sender_idx in 0..=2 {
            let bc1 = bc1_vec[sender_idx].clone();
            let id = &account_ids[sender_idx];
            let m = bc1_to_p2p_keygen(bc1, *CEREMONY_ID, id);

            for receiver_idx in 0..=2 {
                if receiver_idx != sender_idx {
                    clients[receiver_idx].process_p2p_message(m.clone());
                }
            }
        }

        for c in clients.iter() {
            assert_eq!(
                keygen_stage_for(c, *CEREMONY_ID),
                Some(KeygenStage::AwaitingSecret2)
            );
        }

        let mut sec2_vec = vec![];

        for rx in rxs.iter_mut() {
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

        for sender_idx in 0..=2 {
            for receiver_idx in 0..=2 {
                if sender_idx == receiver_idx {
                    continue;
                }

                let r_id = &account_ids[receiver_idx];
                let sec2 = sec2_vec[sender_idx].get(r_id).unwrap();

                let s_id = &account_ids[sender_idx];
                let m = sec2_to_p2p_keygen(sec2.clone(), s_id);

                clients[receiver_idx].process_p2p_message(m);
            }
        }

        let mut pubkeys = vec![];
        for mut r in rxs.iter_mut() {
            let pubkey = match recv_next_inner_event(&mut r).await {
                InnerEvent::KeygenResult(KeygenOutcome {
                    result: Ok(key), ..
                }) => key,
                _ => panic!("Unexpected inner event"),
            };
            pubkeys.push(pubkey);
        }

        // ensure all participants have the same idea of the public key
        assert_eq!(pubkeys[0].serialize(), pubkeys[1].serialize());
        assert_eq!(pubkeys[1].serialize(), pubkeys[2].serialize());

        let mut sec_keys = vec![];

        let key_id = KeyId(pubkeys[0].serialize().into());

        for c in clients.iter() {
            let key = c.get_key(key_id.clone()).expect("key must be present");
            sec_keys.push(key.clone());
        }

        let keygen_phase3 = KeygenPhase3Data {
            clients: clients.clone(),
            pubkey: pubkeys[0],
            sec_keys,
        };

        info!("Keygen ceremony took: {:?}", instant.elapsed());

        ValidKeygenStates {
            keygen_phase1,
            keygen_phase2,
            key_ready: keygen_phase3,
        }
    }

    pub fn substitute_client_at(
        &mut self,
        idx: usize,
        client: MultisigClientInnerNoDB,
        rx: InnerEventReceiver,
    ) {
        self.clients[idx] = client;
        self.rxs[idx] = rx;
    }

    // Use the generated key and the clients participating
    // in the ceremony and sign a message producing state
    // for each of the signing phases
    pub async fn sign(
        &mut self,
        message_info: MessageInfo,
        sign_info: SigningInfo,
    ) -> ValidSigningStates {
        let instant = std::time::Instant::now();

        let account_ids = &self.account_ids;
        let mut clients = self.clients.clone();
        let rxs = &mut self.rxs;

        // *** Send a request to sign and generate BC1 to be distributed ***

        // NOTE: only parties 1 and 2 will participate in signing (SIGNER_IDXS)
        for idx in SIGNER_IDXS.iter() {
            let c = &mut clients[*idx];

            c.process_multisig_instruction(MultisigInstruction::Sign(
                MESSAGE_HASH.clone(),
                sign_info.clone(),
            ));

            assert_eq!(
                c.signing_manager
                    .get_state_for(&message_info)
                    .unwrap()
                    .get_stage(),
                SigningStage::AwaitingBroadcast1
            );
        }

        let mut bc1_vec = vec![];

        for idx in SIGNER_IDXS.iter() {
            let bc1 = recv_bc1_signing(&mut rxs[*idx]).await;
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
            let id = &account_ids[*sender_idx];

            let m = bc1_to_p2p_signing(bc1, id, &message_info);

            for receiver_idx in SIGNER_IDXS.iter() {
                if receiver_idx != sender_idx {
                    clients[*receiver_idx].process_p2p_message(m.clone());
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
                    let receiver_id = &account_ids[*receiver_idx];

                    let sec2 = sec2_vec[*sender_idx].get(receiver_id).unwrap().clone();

                    let id = &account_ids[*sender_idx];
                    let m = sec2_to_p2p_signing(sec2, id, &message_info);

                    clients[*receiver_idx].process_p2p_message(m);
                }
            }
        }

        for idx in SIGNER_IDXS.iter() {
            let c = &mut clients[*idx];
            assert_eq!(
                c.signing_manager
                    .get_state_for(&message_info)
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
            let id = &account_ids[*sender_idx];

            let m = sig_to_p2p(local_sig, id, &message_info);

            for receiver_idx in SIGNER_IDXS.iter() {
                if receiver_idx != sender_idx {
                    clients[*receiver_idx].process_p2p_message(m.clone());
                }
            }
        }

        let signature = match recv_next_inner_event(&mut rxs[0]).await {
            InnerEvent::SigningResult(SigningOutcome {
                result: Ok(sig), ..
            }) => sig,
            _ => panic!("Unexpected event"),
        };

        info!("Signing ceremony took: {:?}", instant.elapsed());

        ValidSigningStates {
            sign_phase1,
            sign_phase2,
            sign_phase3,
            signature,
        }
    }
}

// If we timeout, the channel is empty at the time of retrieval
pub async fn assert_channel_empty(rx: &mut InnerEventReceiver) {
    let fut = rx.recv();
    let dur = std::time::Duration::from_millis(10);

    assert!(tokio::time::timeout(dur, fut).await.is_err());
}

/// Skip all non-signal messages
pub async fn recv_next_signal_message_skipping(
    rx: &mut InnerEventReceiver,
) -> Option<SigningOutcome> {
    let dur = std::time::Duration::from_millis(10);

    loop {
        let res = tokio::time::timeout(dur, rx.recv()).await.ok()??;

        if let InnerEvent::SigningResult(s) = res {
            return Some(s);
        }
    }
}

/// Asserts that InnerEvent is in the queue and returns it
pub async fn recv_next_inner_event(rx: &mut InnerEventReceiver) -> InnerEvent {
    let res = check_for_inner_event(rx).await;

    if let Some(event) = res {
        return event;
    }
    panic!("Expected Inner Event");
}

/// checks for an InnerEvent in the queue with a short timeout, returns the InnerEvent if there is one.
pub async fn check_for_inner_event(rx: &mut InnerEventReceiver) -> Option<InnerEvent> {
    let dur = std::time::Duration::from_millis(10);
    let res = tokio::time::timeout(dur, rx.recv()).await;
    let opt = res.ok()?;
    opt
}

pub async fn recv_p2p_message(rx: &mut InnerEventReceiver) -> P2PMessageCommand {
    let dur = std::time::Duration::from_millis(10);

    let res = tokio::time::timeout(dur, rx.recv())
        .await
        .ok()
        .expect("timeout")
        .unwrap();

    match res {
        InnerEvent::P2PMessageCommand(m) => m,
        e => {
            error!("Unexpected InnerEvent: {:?}", e);
            panic!();
        }
    }
}

async fn recv_multisig_message(rx: &mut InnerEventReceiver) -> (AccountId, MultisigMessage) {
    let m = recv_p2p_message(rx).await;

    (
        m.destination,
        serde_json::from_slice(&m.data).expect("Invalid Multisig Message"),
    )
}

async fn recv_bc1_keygen(rx: &mut InnerEventReceiver) -> Broadcast1 {
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

async fn recv_bc1_signing(rx: &mut InnerEventReceiver) -> Broadcast1 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::Broadcast1(bc1) = data {
            return bc1;
        }
    }

    error!("Received message is not Broadcast1 (signing)");
    panic!();
}

async fn recv_local_sig(rx: &mut InnerEventReceiver) -> LocalSig {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::LocalSig(sig) = data {
            return sig;
        }
    }

    error!("Received message is not LocalSig");
    panic!();
}

async fn recv_secret2_keygen(rx: &mut InnerEventReceiver) -> (AccountId, Secret2) {
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

async fn recv_secret2_signing(rx: &mut InnerEventReceiver) -> (AccountId, Secret2) {
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
pub fn sec2_to_p2p_signing(sec2: Secret2, sender_id: &AccountId, mi: &MessageInfo) -> P2PMessage {
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
pub fn sec2_to_p2p_keygen(sec2: Secret2, sender_id: &AccountId) -> P2PMessage {
    let wrapped = KeyGenMessageWrapped::new(*CEREMONY_ID, sec2);

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn bc1_to_p2p_keygen(
    bc1: Broadcast1,
    ceremony_id: CeremonyId,
    sender_id: &AccountId,
) -> P2PMessage {
    let wrapped = KeyGenMessageWrapped::new(ceremony_id, bc1);

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn bc1_to_p2p_signing(bc1: Broadcast1, sender_id: &AccountId, mi: &MessageInfo) -> P2PMessage {
    let bc1 = SigningData::Broadcast1(bc1);

    let wrapped = SigningDataWrapped::new(bc1, mi.clone());

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn sig_to_p2p(sig: LocalSig, sender_id: &AccountId, mi: &MessageInfo) -> P2PMessage {
    let wrapped = SigningDataWrapped::new(sig, mi.clone());

    let data = MultisigMessage::from(wrapped);
    let data = serde_json::to_vec(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn create_keygen_p2p_message<M>(sender_id: &AccountId, message: M) -> P2PMessage
where
    M: Into<KeygenData>,
{
    let wrapped = KeyGenMessageWrapped::new(0, message.into());

    let ms_message = MultisigMessage::from(wrapped);

    let data = serde_json::to_vec(&ms_message).unwrap();

    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn get_stage_for_msg(
    c: &MultisigClientInnerNoDB,
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

pub fn message_and_sign_info(hash: MessageHash, key_id: KeyId) -> (MessageInfo, SigningInfo) {
    (
        MessageInfo {
            hash,
            key_id: key_id.clone(),
        },
        SigningInfo {
            signers: SIGNER_IDS.clone(),
            key_id,
        },
    )
}
