use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    pin::Pin,
    time::Duration,
};

use futures::StreamExt;
use itertools::Itertools;
use pallet_cf_vaults::CeremonyId;

use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::multisig::{
    client::{
        keygen::{KeygenOptions, SecretShare3},
        signing, CeremonyAbortReason,
    },
    KeyId, MultisigInstruction,
};

use signing::frost::{
    self, LocalSig3, SigningCommitment, SigningData, SigningDataWrapped, VerifyComm2,
    VerifyLocalSig4,
};

use crate::{
    logging::{self, test_utils::TagCache},
    multisig::{
        client::{
            common::KeygenResultInfo,
            keygen::{self, KeygenData},
            InnerEvent, KeygenOutcome, MultisigClient, SigningOutcome,
            {KeygenDataWrapped, MultisigMessage},
        },
        crypto::Point,
        KeyDBMock, KeygenInfo, SigningInfo,
    },
    p2p::{AccountId, P2PMessage, P2PMessageCommand},
};

pub type MultisigClientNoDB = MultisigClient<KeyDBMock>;

use super::{KEYGEN_CEREMONY_ID, MESSAGE_HASH, SIGNER_IDS, SIGNER_IDXS, SIGN_CEREMONY_ID};

macro_rules! recv_data_keygen {
    ($rx:expr, $variant: path) => {{
        let (_, m) = recv_multisig_message($rx).await;

        match m {
            MultisigMessage::KeyGenMessage(KeygenDataWrapped {
                data: $variant(data),
                ..
            }) => data,
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

pub(super) type InnerEventReceiver = Pin<
    Box<futures::stream::Peekable<tokio_stream::wrappers::UnboundedReceiverStream<InnerEvent>>>,
>;

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
    pub sec_stage3: SecStage3Data,
    pub comp_stage4: CompStage4Data,
    pub ver_comp_stage5: VerCompStage5Data,
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

/// Contains the states at different points of key generation
/// including the final state, where the key is created
pub struct KeygenContext {
    account_ids: Vec<AccountId>,
    /// Some tests require data sent between some parties
    /// to deviate from the protocol (e.g. for testing
    /// malicious nodes). Such tests can put non-standard
    /// data here before the ceremony is run.
    custom_data: CustomDataToSend,
    pub rxs: Vec<InnerEventReceiver>,
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
                let valid_comm1 = &comm1_vec[*sender_idx];

                let comm1 = custom_comm1s
                    .remove(&(*sender_idx, *receiver_idx))
                    .unwrap_or(valid_comm1.clone());

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
        let (logger, tag_cache) = logging::test_utils::new_test_logger_with_tag_cache();
        let (clients, rxs): (Vec<_>, Vec<_>) = account_ids
            .iter()
            .map(|id| {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let c = MultisigClient::new(
                    id.clone(),
                    KeyDBMock::new(),
                    tx,
                    KeygenOptions::allowing_high_pubkey(),
                    &logger,
                );
                (c, Box::pin(UnboundedReceiverStream::new(rx).peekable()))
            })
            .unzip();

        KeygenContext {
            account_ids,
            rxs,
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

        self.custom_data
            .comm1_signing
            .insert((sender_idx, receiver_idx), fake_comm1);
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

        clients
            .iter()
            .for_each(|c| assert!(c.is_at_keygen_stage(2)));

        let ver2_vec = recv_all_data_keygen!(rxs, KeygenData::Verify2);

        let ver_com_stage2 = CommVerStage2Data {
            clients: clients.clone(),
            ver2_vec: ver2_vec.clone(),
        };

        // *** Distribute VerifyComm2s, so we can advance and generate Secret3 ***

        distribute_data_keygen!(clients, self.account_ids, ver2_vec);

        clients
            .iter()
            .for_each(|c| assert!(c.is_at_keygen_stage(3)));

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

        let complaints = recv_all_data_keygen!(rxs, KeygenData::Complaints4);

        let comp_stage4 = CompStage4Data {
            clients: clients.clone(),
            comp4s: complaints.clone(),
        };

        println!("Collected all complaints");

        distribute_data_keygen_custom!(
            clients,
            self.account_ids,
            complaints,
            self.custom_data.complaints
        );

        println!("Distributed all complaints");

        let ver_complaints = recv_all_data_keygen!(rxs, KeygenData::VerifyComplaints5);

        let ver_comp_stage5 = VerCompStage5Data {
            clients: clients.clone(),
            ver5: ver_complaints.clone(),
        };

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

            let responses6 = recv_all_data_keygen!(rxs, KeygenData::BlameResponse6);
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

            let ver7 = recv_all_data_keygen!(rxs, KeygenData::VerifyBlameResponses7);
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
                let result = match recv_next_inner_event(&mut r).await {
                    InnerEvent::KeygenResult(KeygenOutcome { result, .. }) => result,
                    _ => panic!("Unexpected inner event"),
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

                for i in 0..(reported_nodes.len() - 1) {
                    let lhs = &reported_nodes[i];
                    let rhs = &reported_nodes[i + 1];

                    assert_eq!(lhs.0, rhs.0);

                    let nodes_lhs: HashSet<_> = lhs.1.iter().cloned().collect();
                    let nodes_rhs: HashSet<_> = rhs.1.iter().cloned().collect();

                    assert_eq!(nodes_lhs, nodes_rhs);
                }

                Err(reported_nodes[0].clone())
            };

            for rx in rxs.iter_mut() {
                assert_channel_empty(rx).await;
            }

            println!("Keygen ceremony took: {:?}", instant.elapsed());

            ValidKeygenStates {
                stage0,
                comm_stage1: com_stage1,
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

        // NOTE: only parties 0, 1 and 2 will participate in signing (SIGNER_IDXS)
        for idx in SIGNER_IDXS.iter() {
            let c = &mut clients[*idx];

            c.process_multisig_instruction(MultisigInstruction::Sign(sign_info.clone()));

            assert!(c.is_at_signing_stage(1));
        }

        let comm1_vec = collect_all_comm1(rxs).await;

        let sign_phase1 = SigningPhase1Data {
            clients: clients.clone(),
            comm1_vec: comm1_vec.clone(),
        };

        // *** Broadcast Comm1 messages to advance to Stage2 ***
        broadcast_all_comm1(
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

        let ver2_vec = collect_all_ver2(rxs).await;

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

        let local_sigs = collect_all_local_sigs3(rxs, &mut self.custom_data.local_sigs).await;

        let sign_phase3 = SigningPhase3Data {
            clients: clients.clone(),
            local_sigs: local_sigs.clone(),
        };

        // *** Distribute local sigs ***
        broadcast_all_local_sigs(&mut clients, &local_sigs, &mut self.custom_data.sig3s).await;

        // *** Collect Ver4 messages ***
        let ver4_vec = collect_all_ver4(rxs).await;

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
                assert_channel_empty(&mut rxs[idx.clone()]).await;
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

// Checks that all signers got the same outcome and returns it
async fn check_and_get_signing_outcome(
    rxs: &mut Vec<InnerEventReceiver>,
) -> Option<SigningOutcome> {
    let mut outcomes: Vec<SigningOutcome> = Vec::new();
    for idx in SIGNER_IDXS.iter() {
        if let Some(outcome) = check_sig_outcome(&mut rxs[idx.clone()]).await {
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
            recv_next_inner_event_opt(&mut rxs[idx.clone()]).await;
        }

        return Some(outcomes[0].clone());
    }
    None
}

const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

/// If we timeout, the channel is empty at the time of retrieval
pub async fn assert_channel_empty(rx: &mut InnerEventReceiver) {
    match recv_next_inner_event_opt(rx).await {
        None => {}
        Some(event) => {
            panic!("Channel is not empty: {:?}", event);
        }
    }
}

/// Consume all messages in the channel, then times out
pub async fn clear_channel(rx: &mut InnerEventReceiver) {
    while let Some(_) = recv_next_inner_event_opt(rx).await {}
}

/// Check the next event produced by the receiver if it is SigningOutcome
pub async fn check_sig_outcome(rx: &mut InnerEventReceiver) -> Option<&SigningOutcome> {
    let event: &InnerEvent = check_inner_event(rx).await?;

    if let InnerEvent::SigningResult(outcome) = event {
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

async fn recv_secret3_keygen(rx: &mut InnerEventReceiver) -> (AccountId, keygen::SecretShare3) {
    let (dest, m) = recv_multisig_message(rx).await;

    if let MultisigMessage::KeyGenMessage(wrapped) = m {
        let KeygenDataWrapped { data: message, .. } = wrapped;

        if let KeygenData::SecretShares3(sec3) = message {
            return (dest, sec3);
        }
    }

    panic!("Received message is not Secret3 (keygen)");
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
    let wrapped = KeygenDataWrapped::new(ceremony_id, data);

    let data = MultisigMessage::from(wrapped);
    let data = bincode::serialize(&data).unwrap();

    P2PMessage {
        sender_id: sender_id.clone(),
        data,
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
    pub fn is_at_signing_stage(&self, stage_number: u32) -> bool {
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
    pub fn is_at_keygen_stage(&self, stage_number: u32) -> bool {
        let stage = get_stage_for_keygen_ceremony(self);
        match stage_number {
            0 => stage == None,
            1 => stage.as_deref() == Some("BroadcastStage<AwaitCommitments1>"),
            2 => stage.as_deref() == Some("BroadcastStage<VerifyCommitmentsBroadcast2>"),
            3 => stage.as_deref() == Some("BroadcastStage<SecretSharesStage3>"),
            4 => stage.as_deref() == Some("BroadcastStage<ComplaintsStage4>"),
            5 => stage.as_deref() == Some("BroadcastStage<VerifyComplaintsBroadcastStage5>"),
            _ => false,
        }
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
