use std::{collections::HashMap, fmt::Debug, pin::Pin, time::Duration};

use futures::StreamExt;
use itertools::Itertools;
use pallet_cf_vaults::CeremonyId;

use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::signing::client::client_inner::{
    frost::{self, VerifyComm2, VerifyLocalSig4},
    keygen_data::{self, KeygenData},
};

use frost::{LocalSig3, SigningCommitment, SigningData, SigningDataWrapped};

use crate::{
    logging,
    p2p::{AccountId, P2PMessage, P2PMessageCommand},
    signing::{
        client::{
            client_inner::{
                client_inner::{Broadcast1, KeyGenMessageWrapped, MultisigMessage},
                common::KeygenResultInfo,
                InnerEvent, KeygenOutcome, MultisigClient, SigningOutcome,
            },
            KeyId, KeygenInfo, MultisigInstruction,
        },
        crypto::{Keys, Point},
        KeyDBMock, SigningInfo,
    },
};

type MultisigClientNoDB = MultisigClient<KeyDBMock>;

use super::{KEYGEN_CEREMONY_ID, MESSAGE_HASH, SIGNER_IDS, SIGNER_IDXS, SIGN_CEREMONY_ID};

macro_rules! recv_data_keygen {
    ($rx:expr, $variant: path) => {{
        let (_, m) = recv_multisig_message($rx).await;

        match m {
            MultisigMessage::KeyGenMessage(KeyGenMessageWrapped {
                data: $variant(data),
                ..
            }) => data,
            _ => {
                eprintln!("Received message is not {}", stringify!($variant));
                panic!();
            }
        }
    }};
}

macro_rules! recv_all_data_keygen {
    ($rxs:expr, $variant: path) => {{
        let mut messages = vec![];

        let count = $rxs.len();

        for rx in $rxs.iter_mut() {
            let comm1 = recv_data_keygen!(rx, $variant);
            messages.push(comm1);

            // ignore (count(other nodes) - 1) messages
            for _ in 0..count - 2 {
                let _ = recv_data_keygen!(rx, $variant);
            }
        }

        messages
    }};
}

macro_rules! distribute_data_keygen {
    ($clients:expr, $account_ids: expr, $messages: expr) => {{
        for sender_idx in 0..$account_ids.len() {
            let message = $messages[sender_idx].clone();
            let id = &$account_ids[sender_idx];

            let m = keygen_data_to_p2p(message, id, KEYGEN_CEREMONY_ID);

            for receiver_idx in 0..$account_ids.len() {
                if receiver_idx != sender_idx {
                    $clients[receiver_idx].process_p2p_message(m.clone());
                }
            }
        }
    }};
}

pub(super) type InnerEventReceiver = Pin<
    Box<futures::stream::Peekable<tokio_stream::wrappers::UnboundedReceiverStream<InnerEvent>>>,
>;

pub struct Stage0Data {
    pub clients: Vec<MultisigClientNoDB>,
}

/// Clients generated comm1, but haven't sent them
pub struct CommStage1Data {
    pub clients: Vec<MultisigClientNoDB>,
    pub comm1_vec: Vec<keygen_data::Comm1>,
}

/// Clients generated ver2, but haven't sent them
pub struct CommVerStage2Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the desitnation node
    pub ver2_vec: Vec<keygen_data::VerifyComm2>,
}

/// Clients generated sec3, but haven't sent them
pub struct SecStage3Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the desitnation node
    pub sec3: Vec<HashMap<AccountId, keygen_data::SecretShare3>>,
}

/// Clients generated copmlaints, but haven't sent them
pub struct CompStage4Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the desitnation node
    pub comp4s: Vec<keygen_data::Complaints4>,
}

pub struct VerCompStage5Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the desitnation node
    pub ver5: Vec<keygen_data::VerifyComplaints5>,
}

pub struct KeyReadyData {
    pub clients: Vec<MultisigClientNoDB>,
    pub pubkey: secp256k1::PublicKey,

    // These are indexed by signer_idx ( -1 )
    pub sec_keys: Vec<KeygenResultInfo>,
}

impl Debug for KeyReadyData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyReadyData")
            .field("pubkey", &self.pubkey)
            .finish()
    }
}

/// Clients received a request to sign and generated (but haven't broadcast) Comm1
pub struct SigningPhase1Data {
    pub clients: Vec<MultisigClientNoDB>,
    pub comm1_vec: Vec<frost::Comm1>,
}

/// Clients generated (but haven't broadcast) VerifyComm2
pub struct SigningPhase2Data {
    pub clients: Vec<MultisigClientNoDB>,
    pub ver2_vec: Vec<frost::VerifyComm2>,
}

/// Clients generated (but haven't broadcast) LocalSig3
pub struct SigningPhase3Data {
    pub clients: Vec<MultisigClientNoDB>,
    pub local_sigs: Vec<frost::LocalSig3>,
}

/// Clients generated (but haven't broadcast) VerifyLocalSig4
pub struct SigningPhase4Data {
    pub clients: Vec<MultisigClientNoDB>,
    pub ver4_vec: Vec<frost::VerifyLocalSig4>,
}

pub struct ValidKeygenStates {
    pub stage0: Stage0Data,
    pub comm_stage1: CommStage1Data,
    pub ver_com_stage2: CommVerStage2Data,
    pub sec_stage3: SecStage3Data,
    pub comp_stage4: CompStage4Data,
    pub ver_comp_stage5: VerCompStage5Data,
    pub key_ready: KeyReadyData,
}

pub struct ValidSigningStates {
    pub sign_phase1: SigningPhase1Data,
    pub sign_phase2: SigningPhase2Data,
    pub sign_phase3: Option<SigningPhase3Data>,
    pub sign_phase4: Option<SigningPhase4Data>,
    pub outcome: SigningOutcome,
}

const TEST_PHASE_TIMEOUT: Duration = Duration::from_secs(5);

pub fn keygen_stage_for(client: &MultisigClientNoDB, ceremony_id: CeremonyId) -> Option<String> {
    client.get_keygen().get_stage_for(ceremony_id)
}

/// Contains the states at different points of key generation
/// including the final state, where the key is created
pub struct KeygenContext {
    account_ids: Vec<AccountId>,

    pub rxs: Vec<InnerEventReceiver>,
    /// This clients will match the ones in `key_ready`,
    /// but stored separately so we could substitute
    /// them in more advanced tests
    clients: Vec<MultisigClientNoDB>,
    /// If a test requires a local sig different from
    /// the one that would be normally generated, it
    /// will be stored here.  This is different from
    // `sig3_to_send` in that these signatures are
    // treated as having been broadcast consistently
    custom_local_sigs: HashMap<usize, frost::LocalSig3>,
    /// Maps a (sender, receiver) pair to the data that will be
    /// sent (in case it needs to be invalid/different from what
    /// is expected normally)
    comm1_to_send: HashMap<(usize, usize), SigningCommitment>,
    // TODO: Sig3 to send between (sender, receiver) in case they
    // needs to be different from the regular, valid ones
    sig3_to_send: HashMap<(usize, usize), LocalSig3>,
    /// The key that was generated
    key_id: Option<KeyId>,
}

fn gen_invalid_local_sig() -> LocalSig3 {
    use crate::signing::crypto::{ECScalar, Scalar};
    frost::LocalSig3 {
        response: Scalar::new_random(),
    }
}

async fn collect_all_comm1(rxs: &mut Vec<InnerEventReceiver>) -> Vec<SigningCommitment> {
    let mut comm1_vec = vec![];

    for idx in SIGNER_IDXS.iter() {
        let rx = &mut rxs[*idx];

        let comm1 = recv_comm1_signing(rx).await;

        // Make sure that messages to other parties are
        // consistent
        for _ in 0..SIGNER_IDXS.len() - 2 {
            assert_eq!(comm1, recv_comm1_signing(rx).await);
        }

        assert_channel_empty(rx).await;

        comm1_vec.push(comm1);
    }

    comm1_vec
}

async fn collect_all_ver2(rxs: &mut Vec<InnerEventReceiver>) -> Vec<VerifyComm2> {
    let mut ver2_vec = vec![];

    for sender_idx in SIGNER_IDXS.iter() {
        let rx = &mut rxs[*sender_idx];

        let ver2 = recv_ver2_signing(rx).await;

        // Ignore all other (same) messages
        for _ in 0..SIGNER_IDXS.len() - 2 {
            let _ = recv_ver2_signing(rx).await;
        }

        assert_channel_empty(rx).await;

        ver2_vec.push(ver2);
    }

    ver2_vec
}

async fn collect_all_local_sigs3(
    rxs: &mut Vec<InnerEventReceiver>,
    custom_sigs: &mut HashMap<usize, frost::LocalSig3>,
) -> Vec<frost::LocalSig3> {
    let mut local_sigs = vec![];

    for idx in SIGNER_IDXS.iter() {
        let rx = &mut rxs[*idx];

        let valid_sig = recv_local_sig(rx).await;

        // Check if the test requested a custom local sig
        // to be emitted by party idx
        let sig = custom_sigs.remove(idx).unwrap_or(valid_sig);

        // Ignore all other (same) messages
        for _ in 0..SIGNER_IDXS.len() - 2 {
            let _ = recv_local_sig(rx).await;
        }

        assert_channel_empty(rx).await;

        local_sigs.push(sig);
    }

    local_sigs
}

async fn collect_all_ver4(rxs: &mut Vec<InnerEventReceiver>) -> Vec<VerifyLocalSig4> {
    let mut ver4_vec = vec![];

    for sender_idx in SIGNER_IDXS.iter() {
        let rx = &mut rxs[*sender_idx];

        let ver4 = recv_ver4_signing(rx).await;

        // Ignore all other (same) messages
        for _ in 0..SIGNER_IDXS.len() - 2 {
            let _ = recv_ver4_signing(rx).await;
        }

        assert_channel_empty(rx).await;

        ver4_vec.push(ver4);
    }

    ver4_vec
}

async fn broadcast_all_comm1(
    clients: &mut Vec<MultisigClientNoDB>,
    comm1_vec: &Vec<SigningCommitment>,
    custom_comm1s: &mut HashMap<(usize, usize), SigningCommitment>,
) {
    for sender_idx in SIGNER_IDXS.iter() {
        for receiver_idx in SIGNER_IDXS.iter() {
            if receiver_idx != sender_idx {
                let valid_comm1 = comm1_vec[*sender_idx].clone();

                let comm1 = custom_comm1s
                    .remove(&(*sender_idx, *receiver_idx))
                    .unwrap_or(valid_comm1);

                let id = &super::VALIDATOR_IDS[*sender_idx];

                let m = sig_data_to_p2p(comm1, id);

                clients[*receiver_idx].process_p2p_message(m.clone());
            }
        }
    }
}

async fn broadcast_all_ver2(clients: &mut Vec<MultisigClientNoDB>, ver2_vec: &Vec<VerifyComm2>) {
    for sender_idx in SIGNER_IDXS.iter() {
        for receiver_idx in SIGNER_IDXS.iter() {
            if sender_idx != receiver_idx {
                let ver2 = ver2_vec[*sender_idx].clone();

                let id = &super::VALIDATOR_IDS[*sender_idx];

                let m = sig_data_to_p2p(ver2, id);

                clients[*receiver_idx].process_p2p_message(m);
            }
        }
    }
}

async fn broadcast_all_local_sigs(
    clients: &mut Vec<MultisigClientNoDB>,
    valid_sigs: &Vec<LocalSig3>,
    custom_sigs: &mut HashMap<(usize, usize), LocalSig3>,
) {
    for sender_idx in SIGNER_IDXS.iter() {
        for receiver_idx in SIGNER_IDXS.iter() {
            let valid_sig = valid_sigs[*sender_idx].clone();
            let sig3 = custom_sigs
                .remove(&(*sender_idx, *receiver_idx))
                .unwrap_or(valid_sig);

            let id = &super::VALIDATOR_IDS[*sender_idx];

            let m = sig_data_to_p2p(sig3, id);

            if receiver_idx != sender_idx {
                clients[*receiver_idx].process_p2p_message(m.clone());
            }
        }
    }
}

async fn broadcast_all_ver4(
    clients: &mut Vec<MultisigClientNoDB>,
    ver4_vec: &Vec<VerifyLocalSig4>,
) {
    for sender_idx in SIGNER_IDXS.iter() {
        for receiver_idx in SIGNER_IDXS.iter() {
            if sender_idx != receiver_idx {
                let ver4 = ver4_vec[*sender_idx].clone();

                let id = &super::VALIDATOR_IDS[*sender_idx];

                let m = sig_data_to_p2p(ver4, id);

                clients[*receiver_idx].process_p2p_message(m);
            }
        }
    }
}

impl KeygenContext {
    /// Generate context without starting the
    /// keygen ceremony
    pub fn new() -> Self {
        let account_ids = super::VALIDATOR_IDS.clone();
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
                let c = MultisigClient::new(
                    id.clone(),
                    KeyDBMock::new(),
                    tx,
                    TEST_PHASE_TIMEOUT,
                    &logger,
                );
                (c, Box::pin(UnboundedReceiverStream::new(rx).peekable()))
            })
            .unzip();

        KeygenContext {
            account_ids,
            rxs,
            clients,
            custom_local_sigs: HashMap::new(),
            comm1_to_send: HashMap::new(),
            sig3_to_send: HashMap::new(),
            key_id: None,
        }
    }

    pub fn key_id(&self) -> KeyId {
        self.key_id.as_ref().expect("must have key").clone()
    }

    pub fn get_client(&self, idx: usize) -> &MultisigClientNoDB {
        &self.clients[idx]
    }

    pub fn use_invalid_local_sig(&mut self, signer_idx: usize) {
        self.custom_local_sigs
            .insert(signer_idx, gen_invalid_local_sig());
    }

    pub fn use_inconsistent_broadcast_for_comm1(&mut self, sender_idx: usize, receiver_idx: usize) {
        assert_ne!(sender_idx, receiver_idx);

        // It doesn't matter what kind of commitment we create here,
        // the main idea is that the commitment doesn't match what we
        // send to all other parties
        let fake_comm1 = SigningCommitment {
            index: sender_idx,
            d: Point::random_point(),
            e: Point::random_point(),
        };

        self.comm1_to_send
            .insert((sender_idx, receiver_idx), fake_comm1);
    }

    pub fn use_inconsistent_broadcast_for_sig3(&mut self, sender_idx: usize, receiver_idx: usize) {
        assert_ne!(sender_idx, receiver_idx);

        // It doesn't matter what kind of local sig we create here,
        // the main idea is that it doesn't match what we
        // send to all other parties
        let fake_sig3 = gen_invalid_local_sig();

        self.sig3_to_send
            .insert((sender_idx, receiver_idx), fake_sig3);
    }

    // Generate keygen states for each of the phases,
    // resulting in `KeygenContext` which can be used
    // to sign messages
    pub async fn generate(&mut self) -> ValidKeygenStates {
        let instant = std::time::Instant::now();

        let clients = &mut self.clients;
        let account_ids = &self.account_ids;
        let rxs = &mut self.rxs;

        let stage0 = Stage0Data {
            clients: clients.clone(),
        };

        // Generate phase 1 data

        let keygen_info = KeygenInfo {
            ceremony_id: KEYGEN_CEREMONY_ID,
            signers: account_ids.clone(),
        };

        for c in clients.iter_mut() {
            c.process_multisig_instruction(MultisigInstruction::KeyGen(keygen_info.clone()));
        }

        let comm1_vec = recv_all_data_keygen!(rxs, KeygenData::Comm1);

        println!("Received all comm1");

        let com_stage1 = CommStage1Data {
            clients: clients.clone(),
            comm1_vec: comm1_vec.clone(),
        };

        distribute_data_keygen!(clients, self.account_ids, comm1_vec);

        println!("Distributed all comm1");

        // TODO: fix this:

        // for c in clients.iter() {
        //     assert_eq!(
        //         keygen_stage_for(c, KEYGEN_CEREMONY_ID),
        //         Some(KeygenStage::AwaitingSecret2)
        //     );
        // }

        let ver2_vec = recv_all_data_keygen!(rxs, KeygenData::Verify2);

        let ver_com_stage2 = CommVerStage2Data {
            clients: clients.clone(),
            ver2_vec: ver2_vec.clone(),
        };

        // *** Distribute VerifyComm2s, so we can advance and generate Secret3 ***

        distribute_data_keygen!(clients, self.account_ids, ver2_vec);

        // *** Collect all Secret3

        let mut sec3_vec = vec![];

        for rx in rxs.iter_mut() {
            let mut sec3_map = HashMap::new();
            for i in 0..self.account_ids.len() - 1 {
                println!("recv secret3 keygen, i: {}", i);
                let (dest, sec3) = recv_secret3_keygen(rx).await;
                sec3_map.insert(dest, sec3);
            }

            sec3_vec.push(sec3_map);
        }

        println!("Received all sec3");

        let sec_stage3 = SecStage3Data {
            clients: clients.clone(),
            sec3: sec3_vec.clone(),
        };

        // Distribute secret 3

        for sender_idx in 0..self.account_ids.len() {
            for receiver_idx in 0..self.account_ids.len() {
                if sender_idx != receiver_idx {
                    let r_id = &account_ids[receiver_idx];

                    let sec3_map = &sec3_vec[sender_idx];
                    let sec3 = sec3_map.get(r_id).unwrap();

                    let s_id = &account_ids[sender_idx];
                    let m = keygen_data_to_p2p(sec3.clone(), s_id, KEYGEN_CEREMONY_ID);

                    clients[receiver_idx].process_p2p_message(m);
                }
            }
        }

        println!("Distributed all sec3");

        let complaints = recv_all_data_keygen!(rxs, KeygenData::Complaints4);

        let comp_stage4 = CompStage4Data {
            clients: clients.clone(),
            comp4s: complaints.clone(),
        };

        println!("Collected all complaints");

        distribute_data_keygen!(clients, self.account_ids, complaints);

        println!("Distributed all complaints");

        let ver_complaints = recv_all_data_keygen!(rxs, KeygenData::VerifyComplaints5);

        let ver_comp_stage5 = VerCompStage5Data {
            clients: clients.clone(),
            ver5: ver_complaints.clone(),
        };

        println!("Collected all verify complaints");

        distribute_data_keygen!(clients, self.account_ids, ver_complaints);

        println!("Distributed all verify complaints");

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
        self.key_id = Some(key_id.clone());

        for c in clients.iter() {
            let key = c.get_key(&key_id).expect("key must be present");
            sec_keys.push(key.clone());
        }

        let key_ready = KeyReadyData {
            clients: clients.clone(),
            pubkey: pubkeys[0],
            sec_keys,
        };

        println!("Keygen ceremony took: {:?}", instant.elapsed());

        for rx in rxs.iter_mut() {
            assert_channel_empty(rx).await;
        }

        ValidKeygenStates {
            stage0,
            comm_stage1: com_stage1,
            ver_com_stage2,
            sec_stage3,
            comp_stage4,
            ver_comp_stage5,
            key_ready,
        }
    }

    pub fn substitute_client_at(
        &mut self,
        idx: usize,
        client: MultisigClientNoDB,
        rx: InnerEventReceiver,
    ) {
        self.clients[idx] = client;
        self.rxs[idx] = rx;
    }

    // Use the generated key and the clients participating
    // in the ceremony and sign a message producing state
    // for each of the signing phases
    pub async fn sign(&mut self) -> ValidSigningStates {
        let instant = std::time::Instant::now();

        let sign_info = SigningInfo::new(
            SIGN_CEREMONY_ID,
            self.key_id(),
            MESSAGE_HASH.clone(),
            SIGNER_IDS.clone(),
        );

        let mut clients = self.clients.clone();
        let rxs = &mut self.rxs;

        assert_channel_empty(&mut rxs[0]).await;

        // *** Send a request to sign and generate BC1 to be distributed ***

        // NOTE: only parties 1 and 2 will participate in signing (SIGNER_IDXS)
        for idx in SIGNER_IDXS.iter() {
            let c = &mut clients[*idx];

            c.process_multisig_instruction(MultisigInstruction::Sign(sign_info.clone()));

            assert_eq!(
                get_stage_for_ceremony(&c, SIGN_CEREMONY_ID),
                Some("BroadcastStage<AwaitCommitments1>".to_string())
            );
        }

        let comm1_vec = collect_all_comm1(rxs).await;

        let sign_phase1 = SigningPhase1Data {
            clients: clients.clone(),
            comm1_vec: comm1_vec.clone(),
        };

        // *** Broadcast Comm1 messages to advance to Stage2 ***
        broadcast_all_comm1(&mut clients, &comm1_vec, &mut self.comm1_to_send).await;

        // TODO: check stage
        // *** Collect Ver2 messages ***

        let ver2_vec = collect_all_ver2(rxs).await;

        let sign_phase2 = SigningPhase2Data {
            clients: clients.clone(),
            ver2_vec: ver2_vec.clone(),
        };

        // *** Distribute Ver2 messages ***

        broadcast_all_ver2(&mut clients, &ver2_vec).await;

        // Check if the ceremony was aborted at this stage
        if let Some(outcome) = check_sig_outcome(&mut rxs[0]).await {
            // TODO: check that the outcome is the same for all parties
            return ValidSigningStates {
                sign_phase1,
                sign_phase2,
                sign_phase3: None,
                sign_phase4: None,
                outcome: outcome.clone(),
            };
        }

        for idx in SIGNER_IDXS.iter() {
            let c = &mut clients[*idx];

            assert_eq!(
                get_stage_for_ceremony(&c, SIGN_CEREMONY_ID),
                Some("BroadcastStage<LocalSigStage3>".to_string())
            );
        }

        // *** Collect local sigs ***

        let local_sigs = collect_all_local_sigs3(rxs, &mut self.custom_local_sigs).await;

        let sign_phase3 = SigningPhase3Data {
            clients: clients.clone(),
            local_sigs: local_sigs.clone(),
        };

        // *** Distribute local sigs ***
        broadcast_all_local_sigs(&mut clients, &local_sigs, &mut self.sig3_to_send).await;

        // *** Collect Ver4 messages ***
        let ver4_vec = collect_all_ver4(rxs).await;

        let sign_phase4 = SigningPhase4Data {
            clients: clients.clone(),
            ver4_vec: ver4_vec.clone(),
        };

        // *** Distribute Ver4 messages ***

        broadcast_all_ver4(&mut clients, &ver4_vec).await;

        let outcome = match recv_next_inner_event(&mut rxs[0]).await {
            InnerEvent::SigningResult(outcome) => outcome,
            _ => panic!("Unexpected event"),
        };

        println!("Signing ceremony took: {:?}", instant.elapsed());

        ValidSigningStates {
            sign_phase1,
            sign_phase2,
            sign_phase3: Some(sign_phase3),
            sign_phase4: Some(sign_phase4),
            outcome,
        }
    }
}

const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

// If we timeout, the channel is empty at the time of retrieval
pub async fn assert_channel_empty(rx: &mut InnerEventReceiver) {
    match recv_next_inner_event_opt(rx).await {
        None => {}
        Some(event) => {
            panic!("Channel is not empty: {:?}", event);
        }
    }
}

/// Skip all non-signal messages
pub async fn recv_next_signal_message_skipping(
    rx: &mut InnerEventReceiver,
) -> Option<SigningOutcome> {
    loop {
        let res = recv_next_inner_event_opt(rx).await?;

        if let InnerEvent::SigningResult(s) = res {
            return Some(s);
        }
    }
}

/// Check if the next event produced by the receiver is SigningOutcome
pub async fn check_sig_outcome(rx: &mut InnerEventReceiver) -> Option<&SigningOutcome> {
    let event: &InnerEvent = check_inner_event(rx).await?;

    if let InnerEvent::SigningResult(outcome) = event {
        Some(outcome)
    } else {
        None
    }
}

/// Check if the next event produced by the receiver is SigningOutcome
pub async fn check_keygen_outcome(rx: &mut InnerEventReceiver) -> Option<&KeygenOutcome> {
    let event: &InnerEvent = check_inner_event(rx).await?;

    if let InnerEvent::KeygenResult(outcome) = event {
        Some(outcome)
    } else {
        None
    }
}

/// Check the next inner event without consuming
pub async fn check_inner_event(rx: &mut InnerEventReceiver) -> Option<&InnerEvent> {
    tokio::time::timeout(CHANNEL_TIMEOUT, rx.as_mut().peek())
        .await
        .ok()?
}

/// Asserts that InnerEvent is in the queue and returns it
pub async fn recv_next_inner_event(rx: &mut InnerEventReceiver) -> InnerEvent {
    let res = recv_next_inner_event_opt(rx).await;

    if let Some(event) = res {
        return event;
    }
    panic!("Expected Inner Event");
}

/// checks for an InnerEvent in the queue with a short timeout, returns the InnerEvent if there is one.
pub async fn recv_next_inner_event_opt(rx: &mut InnerEventReceiver) -> Option<InnerEvent> {
    tokio::time::timeout(CHANNEL_TIMEOUT, rx.next())
        .await
        .ok()?
}

pub async fn recv_p2p_message(rx: &mut InnerEventReceiver) -> P2PMessageCommand {
    let res = tokio::time::timeout(CHANNEL_TIMEOUT, rx.next())
        .await
        .ok()
        .expect("timeout")
        .unwrap();

    match res {
        InnerEvent::P2PMessageCommand(m) => m,
        e => {
            eprintln!("Unexpected InnerEvent: {:?}", e);
            panic!();
        }
    }
}

async fn recv_multisig_message(rx: &mut InnerEventReceiver) -> (AccountId, MultisigMessage) {
    let m = recv_p2p_message(rx).await;

    (
        m.destination,
        bincode::deserialize(&m.data).expect("Invalid Multisig Message"),
    )
}

async fn recv_comm1_signing(rx: &mut InnerEventReceiver) -> frost::Comm1 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::CommStage1(comm1) = data {
            return comm1;
        }
    }

    eprintln!("Received message is not Comm1 (signing)");
    panic!();
}

async fn recv_local_sig(rx: &mut InnerEventReceiver) -> frost::LocalSig3 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::LocalSigStage3(sig) = data {
            return sig;
        }
    }

    eprintln!("Received message is not LocalSig");
    panic!();
}

async fn recv_secret3_keygen(
    rx: &mut InnerEventReceiver,
) -> (AccountId, keygen_data::SecretShare3) {
    let (dest, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::KeyGenMessage(wrapped) = m {
        let KeyGenMessageWrapped { data: message, .. } = wrapped;

        if let KeygenData::SecretShares3(sec3) = message {
            return (dest, sec3);
        }
    }

    eprintln!("Received message is not Secret3 (keygen)");
    panic!();
}

async fn recv_ver2_signing(rx: &mut InnerEventReceiver) -> frost::VerifyComm2 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::BroadcastVerificationStage2(ver2) = data {
            return ver2;
        }
    }

    eprintln!("Received message is not Secret2 (signing)");
    panic!();
}

async fn recv_ver4_signing(rx: &mut InnerEventReceiver) -> frost::VerifyLocalSig4 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::SigningMessage(SigningDataWrapped { data, .. }) = m {
        if let SigningData::VerifyLocalSigsStage4(ver4) = data {
            return ver4;
        }
    }

    eprintln!("Received message is not Secret2 (signing)");
    panic!();
}

pub fn sig_data_to_p2p(data: impl Into<SigningData>, sender_id: &AccountId) -> P2PMessage {
    let wrapped = SigningDataWrapped::new(data, SIGN_CEREMONY_ID);

    let data = MultisigMessage::from(wrapped);
    let data = bincode::serialize(&data).unwrap();
    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn keygen_data_to_p2p(
    data: impl Into<KeygenData>,
    sender_id: &AccountId,
    ceremony_id: CeremonyId,
) -> P2PMessage {
    let wrapped = KeyGenMessageWrapped::new(ceremony_id, data);

    let data = MultisigMessage::from(wrapped);
    let data = bincode::serialize(&data).unwrap();

    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

pub fn get_stage_for_ceremony(c: &MultisigClientNoDB, id: CeremonyId) -> Option<String> {
    c.signing_manager.get_stage_for(id)
}

pub fn get_stage_for_default_ceremony(c: &MultisigClientNoDB) -> Option<String> {
    get_stage_for_ceremony(c, SIGN_CEREMONY_ID)
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

impl MultisigClientNoDB {
    /// runs the MultisigInstruction::Sign with the default id, message hash
    pub fn send_request_to_sign_default(&mut self, key_id: KeyId, signers: Vec<AccountId>) {
        let sign_info = SigningInfo::new(SIGN_CEREMONY_ID, key_id, MESSAGE_HASH.clone(), signers);
        self.process_multisig_instruction(MultisigInstruction::Sign(sign_info));
    }
}

pub async fn check_blamed_paries(rx: &mut InnerEventReceiver, expected: &[usize]) {
    let blamed_parties = match check_inner_event(rx)
        .await
        .as_ref()
        .expect("expected inner_event")
    {
        InnerEvent::SigningResult(outcome) => &outcome.result.as_ref().unwrap_err().1,
        InnerEvent::KeygenResult(outcome) => &outcome.result.as_ref().unwrap_err().1,
        _ => {
            panic!("expected ceremony outcome");
        }
    };

    assert_eq!(
        blamed_parties,
        &expected
            .iter()
            // Needs +1 to map from array idx to signer idx
            .map(|idx| AccountId([*idx as u8 + 1; 32]))
            .collect_vec()
    );
}
