use std::{collections::HashMap, fmt::Debug, pin::Pin, time::Duration};

use futures::{stream::Peekable, StreamExt};
use itertools::Itertools;
use pallet_cf_vaults::CeremonyId;

use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::multisig::{
    client::{
        keygen::{HashContext, KeygenOptions, SecretShare3},
        signing, CeremonyAbortReason, MultisigData, ThresholdParameters,
    },
    KeyId, MultisigInstruction,
};

use signing::frost::{
    self, LocalSig3, SigningCommitment, SigningData, VerifyComm2, VerifyLocalSig4,
};

use keygen::{generate_shares_and_commitment, DKGUnverifiedCommitment};

use crate::{
    logging::{self, test_utils::TagCache},
    multisig::{
        client::{
            common::KeygenResultInfo,
            keygen::{self, KeygenData},
            KeygenOutcome, MultisigClient, MultisigMessage, MultisigOutcome, SigningOutcome,
        },
        crypto::Point,
        KeyDBMock, KeygenInfo, SigningInfo,
    },
    p2p::{AccountId, P2PMessage},
};

pub type MultisigClientNoDB = MultisigClient<KeyDBMock>;

use super::{
    ACCOUNT_IDS, KEYGEN_CEREMONY_ID, MESSAGE_HASH, SIGNER_IDS, SIGNER_IDXS, SIGN_CEREMONY_ID,
};

macro_rules! recv_data_keygen {
    ($rx:expr, $variant: path) => {{
        let (_, m) = recv_multisig_message($rx).await;

        match m {
            MultisigMessage {
                data: MultisigData::Keygen($variant(inner)),
                ..
            } => inner,
            _ => {
                panic!("Received message is not {}", stringify!($variant));
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

macro_rules! distribute_data_keygen_custom {
    ($clients:expr, $account_ids: expr, $messages: expr, $custom_messages: expr) => {{
        for sender_idx in 0..$account_ids.len() {
            let valid_message = $messages[sender_idx].clone();

            let message = $custom_messages
                .remove(&sender_idx)
                .unwrap_or(valid_message);

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

pub(super) type MultisigOutcomeReceiver =
    Pin<Box<Peekable<UnboundedReceiverStream<MultisigOutcome>>>>;

pub(super) type P2PMessageReceiver = Pin<Box<Peekable<UnboundedReceiverStream<P2PMessage>>>>;

pub struct Stage0Data {
    pub clients: Vec<MultisigClientNoDB>,
}

/// Clients generated comm1, but haven't sent them
pub struct CommStage1Data {
    pub clients: Vec<MultisigClientNoDB>,
    pub comm1_vec: Vec<keygen::Comm1>,
}

/// Clients generated ver2, but haven't sent them
pub struct CommVerStage2Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub ver2_vec: Vec<keygen::VerifyComm2>,
}

/// Clients generated sec3, but haven't sent them
pub struct SecStage3Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub sec3: Vec<HashMap<AccountId, keygen::SecretShare3>>,
}

/// Clients generated complaints, but haven't sent them
pub struct CompStage4Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub comp4s: Vec<keygen::Complaints4>,
}

pub struct VerCompStage5Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub ver5: Vec<keygen::VerifyComplaints5>,
}

pub struct BlameResponses6Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub resp6: Vec<keygen::BlameResponse6>,
}

pub struct VerBlameResponses7Data {
    pub clients: Vec<MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub ver7: Vec<keygen::VerifyBlameResponses7>,
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

pub struct ValidKeygenStates {
    pub stage0: Stage0Data,
    pub comm_stage1: CommStage1Data,
    pub ver_com_stage2: CommVerStage2Data,
    pub sec_stage3: Option<SecStage3Data>,
    pub comp_stage4: Option<CompStage4Data>,
    pub ver_comp_stage5: Option<VerCompStage5Data>,
    pub blame_responses6: Option<BlameResponses6Data>,
    pub ver_blame_responses7: Option<VerBlameResponses7Data>,
    /// Either a valid keygen result or a list of blamed parties
    pub key_ready: Result<KeyReadyData, (CeremonyAbortReason, Vec<AccountId>)>,
}

impl ValidKeygenStates {
    /// Get the key and associated data asserting
    /// that the ceremony has been successful
    pub fn key_ready_data(&self) -> &KeyReadyData {
        self.key_ready.as_ref().expect("successful keygen")
    }

    /// Get a clone of the client at index 0 from the specified stage
    pub fn get_client_at_stage(&self, stage: usize) -> MultisigClientNoDB {
        match stage {
            0 => self.stage0.clients[0].clone(),
            1 => self.comm_stage1.clients[0].clone(),
            2 => self.ver_com_stage2.clients[0].clone(),
            3 => self.sec_stage3.as_ref().expect("No stage 3").clients[0].clone(),
            4 => self.comp_stage4.as_ref().expect("No stage 4").clients[0].clone(),
            5 => self.ver_comp_stage5.as_ref().expect("No stage 5").clients[0].clone(),
            6 => self
                .blame_responses6
                .as_ref()
                .expect("No blaming stage")
                .clients[0]
                .clone(),
            7 => self
                .ver_blame_responses7
                .as_ref()
                .expect("No blaming stage")
                .clients[0]
                .clone(),
            _ => panic!("Invalid stage {}", stage),
        }
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

pub struct ValidSigningStates {
    pub sign_phase1: SigningPhase1Data,
    pub sign_phase2: SigningPhase2Data,
    pub sign_phase3: Option<SigningPhase3Data>,
    pub sign_phase4: Option<SigningPhase4Data>,
    pub outcome: SigningOutcome,
}

pub fn get_stage_for_keygen_ceremony(client: &MultisigClientNoDB) -> Option<String> {
    client
        .ceremony_manager
        .get_keygen_stage_for(KEYGEN_CEREMONY_ID)
}

#[derive(Default)]
struct CustomDataToSend {
    /// If a test requires a local sig different from
    /// the one that would be normally generated, it
    /// will be stored here.  This is different from
    // `sig3_to_send` in that these signatures are
    // treated as having been broadcast consistently
    local_sigs: HashMap<usize, frost::LocalSig3>,
    /// Maps a (sender, receiver) pair to the data that will be
    /// sent (in case it needs to be invalid/different from what
    /// is expected normally)
    comm1_signing: HashMap<(usize, usize), SigningCommitment>,
    comm1_keygen: HashMap<(usize, usize), DKGUnverifiedCommitment>,
    // Sig3 to send between (sender, receiver) in case it
    // needs to be different from the regular (valid) one
    sig3s: HashMap<(usize, usize), LocalSig3>,
    // Secret shares to send between (sender, receiver) in case it
    // need to be different from the regular (valid) one
    secret_shares: HashMap<(usize, usize), SecretShare3>,
    // Secret shares to be broadcast during blaming stage
    secret_shares_blaming: HashMap<usize, keygen::BlameResponse6>,
    // Complaints to be broadcast
    complaints: HashMap<usize, keygen::Complaints4>,
}

// TODO: Merge rxs, p2p_rxs, and account_ids, clients into a single vec (Alastair Holmes 18.11.2021)
/// Contains the states at different points of key generation
/// including the final state, where the key is created
pub struct KeygenContext {
    account_ids: Vec<AccountId>,
    /// Some tests require data sent between some parties
    /// to deviate from the protocol (e.g. for testing
    /// malicious nodes). Such tests can put non-standard
    /// data here before the ceremony is run.
    custom_data: CustomDataToSend,
    pub outcome_receivers: Vec<MultisigOutcomeReceiver>,
    pub p2p_receivers: Vec<P2PMessageReceiver>,
    /// This clients will match the ones in `key_ready`,
    /// but stored separately so we could substitute
    /// them in more advanced tests
    clients: Vec<MultisigClientNoDB>,

    /// The key that was generated
    key_id: Option<KeyId>,
    // Cache of all tags that used in log calls
    pub tag_cache: TagCache,
}

fn gen_invalid_local_sig() -> LocalSig3 {
    use crate::multisig::crypto::{ECScalar, Scalar};
    frost::LocalSig3 {
        response: Scalar::new_random(),
    }
}

fn gen_invalid_keygen_comm1() -> DKGUnverifiedCommitment {
    let (_, fake_comm1) = generate_shares_and_commitment(
        &HashContext([0; 32]),
        0,
        ThresholdParameters {
            share_count: ACCOUNT_IDS.len(),
            threshold: ACCOUNT_IDS.len(),
        },
    );
    fake_comm1
}

async fn collect_all_comm1(rxs: &mut Vec<P2PMessageReceiver>) -> Vec<SigningCommitment> {
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

async fn collect_all_ver2(rxs: &mut Vec<P2PMessageReceiver>) -> Vec<VerifyComm2> {
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
    rxs: &mut Vec<P2PMessageReceiver>,
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

async fn collect_all_ver4(p2p_rxs: &mut Vec<P2PMessageReceiver>) -> Vec<VerifyLocalSig4> {
    let mut ver4_vec = vec![];

    for sender_idx in SIGNER_IDXS.iter() {
        let rx = &mut p2p_rxs[*sender_idx];

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

async fn broadcast_all_signing_comm1(
    clients: &mut Vec<MultisigClientNoDB>,
    comm1_vec: &Vec<SigningCommitment>,
    custom_comm1s: &mut HashMap<(usize, usize), SigningCommitment>,
) {
    for sender_idx in SIGNER_IDXS.iter() {
        for receiver_idx in SIGNER_IDXS.iter() {
            if receiver_idx != sender_idx {
                let valid_comm1 = &comm1_vec[*sender_idx];

                let comm1 = custom_comm1s
                    .remove(&(*sender_idx, *receiver_idx))
                    .unwrap_or(valid_comm1.clone());

                let id = &super::ACCOUNT_IDS[*sender_idx];

                let m = sig_data_to_p2p(comm1, id);

                clients[*receiver_idx].process_p2p_message(m.clone());
            }
        }
    }
}

async fn broadcast_all_keygen_comm1(
    clients: &mut Vec<MultisigClientNoDB>,
    account_ids: &Vec<AccountId>,
    comm1_vec: &Vec<DKGUnverifiedCommitment>,
    custom_comm1s: &mut HashMap<(usize, usize), DKGUnverifiedCommitment>,
) {
    for sender_idx in 0..account_ids.len() {
        for receiver_idx in 0..account_ids.len() {
            if receiver_idx != sender_idx {
                let valid_comm1 = &comm1_vec[sender_idx.clone()];

                let comm1 = custom_comm1s
                    .remove(&(sender_idx.clone(), receiver_idx.clone()))
                    .unwrap_or(valid_comm1.clone());

                let id = &account_ids[sender_idx];

                let m = keygen_data_to_p2p(comm1, id, KEYGEN_CEREMONY_ID);

                clients[receiver_idx].process_p2p_message(m.clone());
            }
        }
    }
}

async fn broadcast_all_ver2(clients: &mut Vec<MultisigClientNoDB>, ver2_vec: &Vec<VerifyComm2>) {
    for sender_idx in SIGNER_IDXS.iter() {
        for receiver_idx in SIGNER_IDXS.iter() {
            if sender_idx != receiver_idx {
                let ver2 = ver2_vec[*sender_idx].clone();

                let id = &super::ACCOUNT_IDS[*sender_idx];

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

            let id = &super::ACCOUNT_IDS[*sender_idx];

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

                let id = &super::ACCOUNT_IDS[*sender_idx];

                let m = sig_data_to_p2p(ver4, id);

                clients[*receiver_idx].process_p2p_message(m);
            }
        }
    }
}

impl KeygenContext {
    /// Generate context without starting the keygen ceremony.
    /// `allowing_high_pubkey` is enabled so tests will not fail.
    pub fn new() -> Self {
        let account_ids = super::ACCOUNT_IDS.clone();
        KeygenContext::inner_new(account_ids, KeygenOptions::allowing_high_pubkey())
    }

    pub fn new_with_account_ids(account_ids: Vec<AccountId>) -> Self {
        KeygenContext::inner_new(account_ids, KeygenOptions::allowing_high_pubkey())
    }

    /// Generate context with the KeygenOptions as default, (No `allowing_high_pubkey`)
    pub fn new_disallow_high_pubkey() -> Self {
        let account_ids = super::ACCOUNT_IDS.clone();
        KeygenContext::inner_new(account_ids, KeygenOptions::default())
    }

    fn inner_new(account_ids: Vec<AccountId>, keygen_options: KeygenOptions) -> Self {
        let (logger, tag_cache) = logging::test_utils::new_test_logger_with_tag_cache();
        let mut p2p_rxs = vec![];
        let (clients, rxs): (Vec<_>, Vec<_>) = account_ids
            .iter()
            .map(|id| {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let (p2p_tx, p2p_rx) = tokio::sync::mpsc::unbounded_channel();
                let c = MultisigClient::new(
                    id.clone(),
                    KeyDBMock::new(),
                    tx,
                    p2p_tx,
                    keygen_options,
                    &logger,
                );
                p2p_rxs.push(Box::pin(UnboundedReceiverStream::new(p2p_rx).peekable())); // See KeygenContext TODO
                (c, Box::pin(UnboundedReceiverStream::new(rx).peekable()))
            })
            .unzip();

        KeygenContext {
            account_ids,
            outcome_receivers: rxs,
            p2p_receivers: p2p_rxs,
            clients,
            custom_data: Default::default(),
            key_id: None,
            tag_cache,
        }
    }

    pub fn key_id(&self) -> KeyId {
        self.key_id.as_ref().expect("must have key").clone()
    }

    pub fn get_client(&self, idx: usize) -> &MultisigClientNoDB {
        &self.clients[idx]
    }

    pub fn use_invalid_local_sig(&mut self, signer_idx: usize) {
        self.custom_data
            .local_sigs
            .insert(signer_idx, gen_invalid_local_sig());
    }

    pub fn use_invalid_secret_share(&mut self, sender_idx: usize, receiver_idx: usize) {
        assert_ne!(sender_idx, receiver_idx);

        let invalid_share = SecretShare3::create_random();

        self.custom_data
            .secret_shares
            .insert((sender_idx, receiver_idx), invalid_share);
    }

    pub fn use_invalid_complaint(&mut self, sender_idx: usize) {
        // This complaint is invalid because it contains an invalid index
        let complaint = keygen::Complaints4(vec![1, usize::MAX]);

        self.custom_data.complaints.insert(sender_idx, complaint);
    }

    pub fn use_invalid_blame_response(&mut self, sender_idx: usize, receiver_idx: usize) {
        assert_ne!(sender_idx, receiver_idx);

        // It does not matter whether this invalid share is the same
        // as the invalid share sent earlier (prior to blaming)
        let invalid_share = SecretShare3::create_random();

        self.custom_data
            .secret_shares_blaming
            .entry(sender_idx)
            .or_insert(keygen::BlameResponse6(Default::default()))
            .0
            .insert(receiver_idx, invalid_share);
    }

    pub fn use_inconsistent_broadcast_for_signing_comm1(
        &mut self,
        sender_idx: usize,
        receiver_idx: usize,
    ) {
        assert_ne!(sender_idx, receiver_idx);

        // It doesn't matter what kind of commitment we create here,
        // the main idea is that the commitment doesn't match what we
        // send to all other parties
        let fake_comm1 = SigningCommitment {
            index: sender_idx,
            d: Point::random_point(),
            e: Point::random_point(),
        };

        self.custom_data
            .comm1_signing
            .insert((sender_idx, receiver_idx), fake_comm1);
    }

    /// Make the specified node send a new random commitment to the receiver
    pub fn use_inconsistent_broadcast_for_keygen_comm1(
        &mut self,
        sender_idx: usize,
        receiver_idx: usize,
    ) {
        assert_ne!(sender_idx, receiver_idx);
        self.custom_data
            .comm1_keygen
            .insert((sender_idx, receiver_idx), gen_invalid_keygen_comm1());
    }

    /// Make the specified node send an invalid commitment to all of the other account_ids
    pub fn use_invalid_keygen_comm1(&mut self, sender_idx: usize) {
        let fake_comm1 = gen_invalid_keygen_comm1();

        (0..self.account_ids.len()).for_each(|receiver_idx| {
            if sender_idx != receiver_idx {
                self.custom_data
                    .comm1_keygen
                    .insert((sender_idx, receiver_idx.clone()), fake_comm1.clone());
            }
        });
    }

    pub fn use_inconsistent_broadcast_for_sig3(&mut self, sender_idx: usize, receiver_idx: usize) {
        assert_ne!(sender_idx, receiver_idx);

        // It doesn't matter what kind of local sig we create here,
        // the main idea is that it doesn't match what we
        // send to all other parties
        let fake_sig3 = gen_invalid_local_sig();

        self.custom_data
            .sig3s
            .insert((sender_idx, receiver_idx), fake_sig3);
    }

    // Generate keygen states for each of the phases,
    // resulting in `KeygenContext` which can be used
    // to sign messages
    pub async fn generate(&mut self) -> ValidKeygenStates {
        let instant = std::time::Instant::now();

        let clients = &mut self.clients;
        let account_ids = &self.account_ids;
        let rxs = &mut self.outcome_receivers;
        let p2p_rxs = &mut self.p2p_receivers;

        let stage0 = Stage0Data {
            clients: clients.clone(),
        };

        // Generate phase 1 data

        let keygen_info = KeygenInfo {
            ceremony_id: KEYGEN_CEREMONY_ID,
            signers: account_ids.clone(),
        };

        for c in clients.iter_mut() {
            c.process_multisig_instruction(MultisigInstruction::Keygen(keygen_info.clone()));
        }

        let comm1_vec = recv_all_data_keygen!(p2p_rxs, KeygenData::Comm1);

        println!("Received all comm1");

        let comm_stage1 = CommStage1Data {
            clients: clients.clone(),
            comm1_vec: comm1_vec.clone(),
        };

        broadcast_all_keygen_comm1(
            clients,
            &self.account_ids,
            &comm1_vec,
            &mut self.custom_data.comm1_keygen,
        )
        .await;

        println!("Distributed all comm1");

        clients
            .iter()
            .for_each(|c| assert!(c.is_at_keygen_stage(2)));

        let ver2_vec = recv_all_data_keygen!(p2p_rxs, KeygenData::Verify2);

        let ver_com_stage2 = CommVerStage2Data {
            clients: clients.clone(),
            ver2_vec: ver2_vec.clone(),
        };

        // *** Distribute VerifyComm2s, so we can advance and generate Secret3 ***

        distribute_data_keygen!(clients, self.account_ids, ver2_vec);

        if !clients[0].is_at_keygen_stage(3) {
            // The ceremony failed early, gather the result and reported_nodes, then return
            let mut results = vec![];
            for mut r in rxs.iter_mut() {
                let result = match expect_next_with_timeout(&mut r).await {
                    MultisigOutcome::Keygen(KeygenOutcome { result, .. }) => result,
                    _ => panic!("Unexpected keygen outcome"),
                };
                results.push(result);
            }

            let reported_nodes: Vec<_> = results
                .iter()
                .map(|res| res.as_ref().unwrap_err())
                .collect();

            assert!(
                check_reported_nodes_consistency(&reported_nodes),
                "Not all nodes reported the same parties"
            );

            return ValidKeygenStates {
                stage0,
                comm_stage1,
                ver_com_stage2,
                sec_stage3: None,
                comp_stage4: None,
                ver_comp_stage5: None,
                blame_responses6: None,
                ver_blame_responses7: None,
                key_ready: Err(reported_nodes[0].clone()),
            };
        }

        clients
            .iter()
            .for_each(|c| assert!(c.is_at_keygen_stage(3)));

        // *** Collect all Secret3

        let mut sec3_vec = vec![];

        for rx in p2p_rxs.iter_mut() {
            let mut sec3_map = HashMap::new();
            for i in 0..self.account_ids.len() - 1 {
                println!("recv secret3 keygen, i: {}", i);
                let (dest, sec3) = recv_secret3_keygen(rx).await;
                sec3_map.insert(dest, sec3);
            }

            sec3_vec.push(sec3_map);
        }

        println!("Received all sec3");

        let sec_stage3 = Some(SecStage3Data {
            clients: clients.clone(),
            sec3: sec3_vec.clone(),
        });

        // Distribute secret 3

        for sender_idx in 0..self.account_ids.len() {
            for receiver_idx in 0..self.account_ids.len() {
                if sender_idx != receiver_idx {
                    let valid_sec3 = {
                        let r_id = &account_ids[receiver_idx];
                        sec3_vec[sender_idx].get(r_id).unwrap()
                    };

                    let sec3 = self
                        .custom_data
                        .secret_shares
                        .remove(&(sender_idx, receiver_idx))
                        .unwrap_or(valid_sec3.clone());

                    let s_id = &account_ids[sender_idx];
                    let m = keygen_data_to_p2p(sec3.clone(), s_id, KEYGEN_CEREMONY_ID);

                    clients[receiver_idx].process_p2p_message(m);
                }
            }
        }

        println!("Distributed all sec3");

        let complaints = recv_all_data_keygen!(p2p_rxs, KeygenData::Complaints4);

        let comp_stage4 = Some(CompStage4Data {
            clients: clients.clone(),
            comp4s: complaints.clone(),
        });

        println!("Collected all complaints");

        distribute_data_keygen_custom!(
            clients,
            self.account_ids,
            complaints,
            self.custom_data.complaints
        );

        println!("Distributed all complaints");

        let ver_complaints = recv_all_data_keygen!(p2p_rxs, KeygenData::VerifyComplaints5);

        let ver_comp_stage5 = Some(VerCompStage5Data {
            clients: clients.clone(),
            ver5: ver_complaints.clone(),
        });

        println!("Collected all verify complaints");

        distribute_data_keygen!(clients, self.account_ids, ver_complaints);

        println!("Distributed all verify complaints");

        // Now we are either done or have to enter the blaming stage
        let nodes_entered_blaming = clients.iter().all(|c| {
            get_stage_for_keygen_ceremony(&c).as_deref()
                == Some("BroadcastStage<BlameResponsesStage6>")
        });

        let (mut blame_responses6, mut ver_blame_responses7) = (None, None);

        if nodes_entered_blaming {
            println!("All clients entered blaming phase!");

            let responses6 = recv_all_data_keygen!(p2p_rxs, KeygenData::BlameResponse6);
            blame_responses6 = Some(BlameResponses6Data {
                clients: clients.clone(),
                resp6: responses6.clone(),
            });

            println!("Collected all blame responses");

            distribute_data_keygen_custom!(
                clients,
                self.account_ids,
                responses6,
                &mut self.custom_data.secret_shares_blaming
            );

            println!("Distributed all blame responses");

            let ver7 = recv_all_data_keygen!(p2p_rxs, KeygenData::VerifyBlameResponses7);
            ver_blame_responses7 = Some(VerBlameResponses7Data {
                clients: clients.clone(),
                ver7: ver7.clone(),
            });

            println!("Collected all blame responses verification");

            distribute_data_keygen!(clients, self.account_ids, ver7);

            println!("Distributed all blame responses verification");
        }

        {
            let mut results = vec![];
            for mut r in rxs.iter_mut() {
                let result = match expect_next_with_timeout(&mut r).await {
                    MultisigOutcome::Keygen(KeygenOutcome { result, .. }) => result,
                    _ => panic!("Unexpected multisig outcome"),
                };
                results.push(result);
            }

            // Check if ceremony is successful for all parties
            let all_successful = results.iter().all(|res| res.is_ok());

            let key_ready = if all_successful {
                let pubkeys: Vec<_> = results.iter().map(|res| res.clone().unwrap()).collect();

                // ensure all participants have the same public key
                assert_eq!(pubkeys[0].serialize(), pubkeys[1].serialize());
                assert_eq!(pubkeys[1].serialize(), pubkeys[2].serialize());

                let mut sec_keys = vec![];

                let key_id = KeyId(pubkeys[0].serialize().into());
                self.key_id = Some(key_id.clone());

                for c in clients.iter() {
                    let key = c.get_key(&key_id).expect("key must be present");
                    sec_keys.push(key.clone());
                }

                Ok(KeyReadyData {
                    clients: clients.clone(),
                    pubkey: pubkeys[0],
                    sec_keys,
                })
            } else {
                // Check that the results from all parties are consistent
                assert!(
                    results.iter().all(|res| res.is_err()),
                    "Ceremony didn't result in an error for all parties"
                );

                let reported_nodes: Vec<_> = results
                    .iter()
                    .map(|res| res.as_ref().unwrap_err())
                    .collect();

                assert!(
                    check_reported_nodes_consistency(&reported_nodes),
                    "Not all nodes reported the same parties"
                );

                Err(reported_nodes[0].clone())
            };

            // Make sure the channel is clean for the unit tests
            for rx in rxs.iter_mut() {
                assert_channel_empty(rx).await;
            }

            println!("Keygen ceremony took: {:?}", instant.elapsed());

            ValidKeygenStates {
                stage0,
                comm_stage1,
                ver_com_stage2,
                sec_stage3,
                comp_stage4,
                ver_comp_stage5,
                blame_responses6,
                ver_blame_responses7,
                key_ready,
            }
        }
    }

    pub fn substitute_client_at(
        &mut self,
        idx: usize,
        client: MultisigClientNoDB,
        rx: MultisigOutcomeReceiver,
        p2p_rx: P2PMessageReceiver,
    ) {
        self.clients[idx] = client;
        self.outcome_receivers[idx] = rx;
        self.p2p_receivers[idx] = p2p_rx;
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
        let rxs = &mut self.outcome_receivers;
        let p2p_rxs = &mut self.p2p_receivers;

        assert_channel_empty(&mut p2p_rxs[0]).await;

        // *** Send a request to sign and generate BC1 to be distributed ***

        // NOTE: only parties 0, 1 and 2 will participate in signing (SIGNER_IDXS)
        for idx in SIGNER_IDXS.iter() {
            let c = &mut clients[*idx];

            c.process_multisig_instruction(MultisigInstruction::Sign(sign_info.clone()));

            assert!(c.is_at_signing_stage(1));
        }

        let comm1_vec = collect_all_comm1(p2p_rxs).await;

        let sign_phase1 = SigningPhase1Data {
            clients: clients.clone(),
            comm1_vec: comm1_vec.clone(),
        };

        // *** Broadcast Comm1 messages to advance to Stage2 ***
        broadcast_all_signing_comm1(
            &mut clients,
            &comm1_vec,
            &mut self.custom_data.comm1_signing,
        )
        .await;

        for idx in SIGNER_IDXS.iter() {
            let c = &mut clients[*idx];
            assert!(c.is_at_signing_stage(2));
        }

        // *** Collect Ver2 messages ***

        let ver2_vec = collect_all_ver2(p2p_rxs).await;

        let sign_phase2 = SigningPhase2Data {
            clients: clients.clone(),
            ver2_vec: ver2_vec.clone(),
        };

        // *** Distribute Ver2 messages ***

        broadcast_all_ver2(&mut clients, &ver2_vec).await;

        // Check if the ceremony was aborted at this stage
        if let Some(outcome) = check_and_get_signing_outcome(rxs).await {
            // The ceremony was aborted early,
            return ValidSigningStates {
                sign_phase1,
                sign_phase2,
                sign_phase3: None,
                sign_phase4: None,
                outcome: outcome,
            };
        }

        for idx in SIGNER_IDXS.iter() {
            let c = &mut clients[*idx];
            assert!(c.is_at_signing_stage(3));
        }

        // *** Collect local sigs ***

        let local_sigs = collect_all_local_sigs3(p2p_rxs, &mut self.custom_data.local_sigs).await;

        let sign_phase3 = SigningPhase3Data {
            clients: clients.clone(),
            local_sigs: local_sigs.clone(),
        };

        // *** Distribute local sigs ***
        broadcast_all_local_sigs(&mut clients, &local_sigs, &mut self.custom_data.sig3s).await;

        // *** Collect Ver4 messages ***
        let ver4_vec = collect_all_ver4(p2p_rxs).await;

        let sign_phase4 = SigningPhase4Data {
            clients: clients.clone(),
            ver4_vec: ver4_vec.clone(),
        };

        // *** Distribute Ver4 messages ***

        broadcast_all_ver4(&mut clients, &ver4_vec).await;

        if let Some(outcome) = check_and_get_signing_outcome(rxs).await {
            println!("Signing ceremony took: {:?}", instant.elapsed());

            // Make sure the channel is clean for the unit tests
            for idx in SIGNER_IDXS.iter() {
                assert_channel_empty(&mut p2p_rxs[idx.clone()]).await;
            }

            ValidSigningStates {
                sign_phase1,
                sign_phase2,
                sign_phase3: Some(sign_phase3),
                sign_phase4: Some(sign_phase4),
                outcome: outcome,
            }
        } else {
            panic!("No Signing Outcome")
        }
    }
}

// Returns true if all of the nodes reported the same parties.
fn check_reported_nodes_consistency(
    reported_nodes: &Vec<&(CeremonyAbortReason, Vec<AccountId>)>,
) -> bool {
    for (_, reported_parties) in reported_nodes.iter() {
        let sorted_reported_parties: Vec<AccountId> =
            reported_parties.iter().sorted().cloned().collect();
        let other_sorted_reported_parties: Vec<AccountId> =
            reported_nodes[0].1.iter().sorted().cloned().collect();
        if sorted_reported_parties != other_sorted_reported_parties {
            return false;
        }
    }
    true
}

// Checks that all signers got the same outcome and returns it
async fn check_and_get_signing_outcome(
    rxs: &mut Vec<MultisigOutcomeReceiver>,
) -> Option<SigningOutcome> {
    let mut outcomes: Vec<SigningOutcome> = Vec::new();
    for idx in SIGNER_IDXS.iter() {
        if let Some(outcome) = peek_with_timeout(&mut rxs[idx.clone()])
            .await
            .and_then(|outcome| {
                if let MultisigOutcome::Signing(outcome) = outcome {
                    Some(outcome)
                } else {
                    None
                }
            })
        {
            // sort the vec of blamed_parties so that we can compare the SigningOutcome's later
            let sorted_outcome = match &outcome.result {
                Ok(_) => outcome.clone(),
                Err((reason, blamed_parties)) => {
                    let sorted_blamed_parties: Vec<AccountId> =
                        blamed_parties.iter().sorted().cloned().collect();
                    SigningOutcome {
                        id: outcome.id,
                        result: Err((reason.clone(), sorted_blamed_parties)),
                    }
                }
            };

            outcomes.push(sorted_outcome);
        }
    }

    if !outcomes.is_empty() {
        assert_eq!(
            outcomes.len(),
            SIGNER_IDXS.len(),
            "Not all signers got an outcome"
        );

        for outcome in outcomes.iter() {
            assert_eq!(outcome, &outcomes[0], "Outcome different between signers");
        }

        // Consume the outcome message if its all good
        for idx in SIGNER_IDXS.iter() {
            next_with_timeout(&mut rxs[idx.clone()]).await;
        }

        return Some(outcomes[0].clone());
    }
    None
}

const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

/// If we timeout, the channel is empty at the time of retrieval
pub async fn assert_channel_empty<I: Debug, S: futures::Stream<Item = I> + Unpin>(rx: &mut S) {
    match tokio::time::timeout(CHANNEL_TIMEOUT, rx.next()).await.ok() {
        None => {}
        Some(event) => {
            // Note we also panic if the channel is closed
            panic!("Channel is not empty: {:?}", event);
        }
    }
}

/// Consume all messages in the channel, then times out
pub async fn clear_channel<I>(rx: &mut Pin<Box<Peekable<UnboundedReceiverStream<I>>>>) {
    while let Some(_) = next_with_timeout(rx).await {}
}

/// Check the next event produced by the receiver if it is SigningOutcome
pub async fn peek_with_timeout<I>(
    rx: &mut Pin<Box<Peekable<UnboundedReceiverStream<I>>>>,
) -> Option<&I> {
    tokio::time::timeout(CHANNEL_TIMEOUT, rx.as_mut().peek())
        .await
        .ok()?
}

/// checks for an item in the queue with a short timeout, returns the item if there is one.
pub async fn next_with_timeout<I>(
    rx: &mut Pin<Box<Peekable<UnboundedReceiverStream<I>>>>,
) -> Option<I> {
    tokio::time::timeout(CHANNEL_TIMEOUT, rx.next())
        .await
        .ok()?
}

pub async fn expect_next_with_timeout<I>(
    rx: &mut Pin<Box<Peekable<UnboundedReceiverStream<I>>>>,
) -> I {
    match next_with_timeout(rx).await {
        Some(i) => i,
        None => panic!("Expected {}", std::any::type_name::<I>()),
    }
}

async fn recv_multisig_message(rx: &mut P2PMessageReceiver) -> (AccountId, MultisigMessage) {
    let m = expect_next_with_timeout(rx).await;

    (
        m.account_id,
        bincode::deserialize(&m.data).expect("Invalid Multisig Message"),
    )
}

async fn recv_comm1_signing(rx: &mut P2PMessageReceiver) -> frost::Comm1 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage {
        data: MultisigData::Signing(data),
        ..
    } = m
    {
        if let SigningData::CommStage1(comm1) = data {
            return comm1;
        }
    }

    eprintln!("Received message is not Comm1 (signing)");
    panic!();
}

async fn recv_local_sig(rx: &mut P2PMessageReceiver) -> frost::LocalSig3 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage {
        data: MultisigData::Signing(data),
        ..
    } = m
    {
        if let SigningData::LocalSigStage3(sig) = data {
            return sig;
        }
    }

    eprintln!("Received message is not LocalSig");
    panic!();
}

async fn recv_secret3_keygen(rx: &mut P2PMessageReceiver) -> (AccountId, keygen::SecretShare3) {
    if let (
        dest,
        MultisigMessage {
            data: MultisigData::Keygen(KeygenData::SecretShares3(sec3)),
            ..
        },
    ) = recv_multisig_message(rx).await
    {
        return (dest, sec3);
    } else {
        panic!("Received message is not Secret3 (keygen)");
    }
}

async fn recv_ver2_signing(rx: &mut P2PMessageReceiver) -> frost::VerifyComm2 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage {
        data: MultisigData::Signing(data),
        ..
    } = m
    {
        if let SigningData::BroadcastVerificationStage2(ver2) = data {
            return ver2;
        }
    }

    eprintln!("Received message is not Secret2 (signing)");
    panic!();
}

async fn recv_ver4_signing(rx: &mut P2PMessageReceiver) -> frost::VerifyLocalSig4 {
    let (_, m) = recv_multisig_message(rx).await;

    if let MultisigMessage {
        data: MultisigData::Signing(data),
        ..
    } = m
    {
        if let SigningData::VerifyLocalSigsStage4(ver4) = data {
            return ver4;
        }
    }

    eprintln!("Received message is not Secret2 (signing)");
    panic!();
}

pub fn sig_data_to_p2p(data: impl Into<SigningData>, sender_id: &AccountId) -> P2PMessage {
    P2PMessage {
        account_id: sender_id.clone(),
        data: bincode::serialize(&MultisigMessage {
            ceremony_id: SIGN_CEREMONY_ID,
            data: MultisigData::Signing(data.into()),
        })
        .unwrap(),
    }
}

pub fn keygen_data_to_p2p(
    data: impl Into<KeygenData>,
    sender_id: &AccountId,
    ceremony_id: CeremonyId,
) -> P2PMessage {
    P2PMessage {
        account_id: sender_id.clone(),
        data: bincode::serialize(&MultisigMessage {
            ceremony_id,
            data: MultisigData::Keygen(data.into()),
        })
        .unwrap(),
    }
}

pub fn get_stage_for_signing_ceremony(c: &MultisigClientNoDB) -> Option<String> {
    c.ceremony_manager.get_signing_stage_for(SIGN_CEREMONY_ID)
}

impl MultisigClientNoDB {
    /// runs the MultisigInstruction::Sign with the default id, message hash
    pub fn send_request_to_sign_default(&mut self, key_id: KeyId, signers: Vec<AccountId>) {
        let sign_info = SigningInfo::new(SIGN_CEREMONY_ID, key_id, MESSAGE_HASH.clone(), signers);
        self.process_multisig_instruction(MultisigInstruction::Sign(sign_info));
    }

    /// Check is the client is at the specified signing BroadcastStage (0-4).
    /// 0 = No Stage
    /// 1 = AwaitCommitments1 ... and so on
    pub fn is_at_signing_stage(&self, stage_number: usize) -> bool {
        let stage = get_stage_for_signing_ceremony(self);
        match stage_number {
            0 => stage == None,
            1 => stage.as_deref() == Some("BroadcastStage<AwaitCommitments1>"),
            2 => stage.as_deref() == Some("BroadcastStage<VerifyCommitmentsBroadcast2>"),
            3 => stage.as_deref() == Some("BroadcastStage<LocalSigStage3>"),
            4 => stage.as_deref() == Some("BroadcastStage<VerifyLocalSigsBroadcastStage4>"),
            _ => false,
        }
    }

    /// Check is the client is at the specified keygen BroadcastStage (0-5).
    /// 0 = No Stage
    /// 1 = AwaitCommitments1 ... and so on
    pub fn is_at_keygen_stage(&self, stage_number: usize) -> bool {
        let stage = get_stage_for_keygen_ceremony(self);
        match stage_number {
            0 => stage == None,
            1 => stage.as_deref() == Some("BroadcastStage<AwaitCommitments1>"),
            2 => stage.as_deref() == Some("BroadcastStage<VerifyCommitmentsBroadcast2>"),
            3 => stage.as_deref() == Some("BroadcastStage<SecretSharesStage3>"),
            4 => stage.as_deref() == Some("BroadcastStage<ComplaintsStage4>"),
            5 => stage.as_deref() == Some("BroadcastStage<VerifyComplaintsBroadcastStage5>"),
            6 => stage.as_deref() == Some("BroadcastStage<BlameResponsesStage6>"),
            7 => stage.as_deref() == Some("BroadcastStage<VerifyBlameResponsesBroadcastStage7>"),
            _ => false,
        }
    }

    /// Sends the correct keygen data from the `ACCOUNT_IDS[sender_idx]` to the client via `process_p2p_message`
    pub fn receive_keygen_stage_data(
        &mut self,
        stage: usize,
        keygen_states: &ValidKeygenStates,
        sender_idx: usize,
    ) {
        let message = self.get_keygen_p2p_message_for_stage(
            stage,
            keygen_states,
            sender_idx,
            &ACCOUNT_IDS[sender_idx],
        );
        self.process_p2p_message(message);
    }

    /// Makes a P2PMessage using the keygen data for the specified stage
    pub fn get_keygen_p2p_message_for_stage(
        &mut self,
        stage: usize,
        keygen_states: &ValidKeygenStates,
        sender_idx: usize,
        sender_id: &AccountId,
    ) -> P2PMessage {
        match stage {
            1 => keygen_data_to_p2p(
                keygen_states.comm_stage1.comm1_vec[sender_idx].clone(),
                sender_id,
                KEYGEN_CEREMONY_ID,
            ),
            2 => keygen_data_to_p2p(
                keygen_states.ver_com_stage2.ver2_vec[sender_idx].clone(),
                sender_id,
                KEYGEN_CEREMONY_ID,
            ),
            3 => {
                let sec3 = keygen_states.sec_stage3.as_ref().expect("No stage 3").sec3[sender_idx]
                    .get(&self.get_my_account_id())
                    .unwrap();
                keygen_data_to_p2p(sec3.clone(), sender_id, KEYGEN_CEREMONY_ID)
            }
            4 => keygen_data_to_p2p(
                keygen_states
                    .comp_stage4
                    .as_ref()
                    .expect("No stage 4")
                    .comp4s[sender_idx]
                    .clone(),
                sender_id,
                KEYGEN_CEREMONY_ID,
            ),
            5 => keygen_data_to_p2p(
                keygen_states
                    .ver_comp_stage5
                    .as_ref()
                    .expect("No stage 5")
                    .ver5[sender_idx]
                    .clone(),
                sender_id,
                KEYGEN_CEREMONY_ID,
            ),
            6 => keygen_data_to_p2p(
                keygen_states
                    .blame_responses6
                    .as_ref()
                    .expect("No blaming stage 6")
                    .resp6[sender_idx]
                    .clone(),
                sender_id,
                KEYGEN_CEREMONY_ID,
            ),
            7 => keygen_data_to_p2p(
                keygen_states
                    .ver_blame_responses7
                    .as_ref()
                    .expect("No blaming stage 7")
                    .ver7[sender_idx]
                    .clone(),
                sender_id,
                KEYGEN_CEREMONY_ID,
            ),
            _ => panic!("Invalid stage to receive message, stage: {}", stage),
        }
    }
}

pub async fn check_blamed_paries(rx: &mut MultisigOutcomeReceiver, expected: &[usize]) {
    let blamed_parties = match peek_with_timeout(rx)
        .await
        .as_ref()
        .expect("expected multisig_outcome")
    {
        MultisigOutcome::Signing(outcome) => &outcome.result.as_ref().unwrap_err().1,
        MultisigOutcome::Keygen(outcome) => &outcome.result.as_ref().unwrap_err().1,
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
