use std::{collections::HashMap, convert::TryInto, fmt::Debug, pin::Pin, time::Duration};

use anyhow::Result;
use cf_chains::eth::{AggKey, SchnorrVerificationComponents};
use futures::{stream::Peekable, StreamExt};
use itertools::Itertools;
use pallet_cf_vaults::CeremonyId;

use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::multisig::{
    client::{
        keygen::{HashContext, KeygenOptions, SecretShare3},
        signing,
        utils::PartyIdxMapping,
        CeremonyAbortReason, MultisigData, ThresholdParameters,
    },
    KeyId, MultisigInstruction, SchnorrSignature,
};

use crate::testing::assert_ok;

use signing::frost::{self, LocalSig3, SigningCommitment, SigningData};

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
    multisig_p2p::AccountId,
};

pub type MultisigClientNoDB = MultisigClient<KeyDBMock>;

use super::{ACCOUNT_IDS, KEYGEN_CEREMONY_ID, MESSAGE_HASH, SIGNER_IDS, SIGN_CEREMONY_ID};

macro_rules! recv_data_keygen {
    ($rx:expr, $variant: path) => {{
        let (_, m) = expect_next_with_timeout($rx).await;

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
        let mut messages = HashMap::new();

        let count = $rxs.len();

        for (id, rx) in $rxs.iter_mut() {
            let data = recv_data_keygen!(rx, $variant);
            messages.insert(id.clone(), data);

            // ignore (count(other nodes) - 1) messages
            for _ in 0..count - 2 {
                let _ = recv_data_keygen!(rx, $variant);
            }
        }

        messages
    }};
}

macro_rules! recv_data_signing {
    ($rx:expr, $variant: path) => {{
        let (_, m) = expect_next_with_timeout($rx).await;

        match m {
            MultisigMessage {
                data: MultisigData::Signing($variant(inner)),
                ..
            } => inner,
            _ => {
                panic!("Received message is not {}", stringify!($variant));
            }
        }
    }};
}

macro_rules! recv_all_data_signing {
    ($rxs:expr, $variant: path) => {{
        let mut messages = HashMap::new();

        let signer_count = $rxs.len();

        for (id, rx) in $rxs.iter_mut() {
            let data = recv_data_signing!(rx, $variant);
            messages.insert(id.clone(), data);

            // ignore (count(other nodes) - 1) messages
            for _ in 0..signer_count - 2 {
                let _ = recv_data_signing!(rx, $variant);
            }
            assert_channel_empty(rx).await;
        }

        messages
    }};
}

macro_rules! distribute_data_keygen_custom {
    ($clients:expr, $account_ids: expr, $messages: expr, $custom_messages: expr) => {{
        for sender_id in &$account_ids {
            for receiver_id in &$account_ids {
                if receiver_id != sender_id {
                    let valid_message = $messages[&sender_id].clone();

                    let message = $custom_messages
                        .remove(&(sender_id.clone(), receiver_id.clone()))
                        .unwrap_or(valid_message);

                    let message = keygen_data_to_p2p(message, KEYGEN_CEREMONY_ID);

                    $clients
                        .get_mut(receiver_id)
                        .unwrap()
                        .process_p2p_message(sender_id.clone(), message.clone());
                }
            }
        }
    }};
}

macro_rules! distribute_data_keygen {
    ($clients:expr, $account_ids: expr, $messages: expr) => {{
        for sender_id in &$account_ids {
            let message = keygen_data_to_p2p($messages[sender_id].clone(), KEYGEN_CEREMONY_ID);

            for receiver_id in &$account_ids {
                if receiver_id != sender_id {
                    $clients
                        .get_mut(receiver_id)
                        .unwrap()
                        .process_p2p_message(sender_id.clone(), message.clone());
                }
            }
        }
    }};
}

macro_rules! distribute_data_signing {
    ($clients:expr, $messages: expr) => {{
        let ids = $clients.keys().cloned().collect_vec();

        for sender_id in &ids {
            for receiver_id in &ids {
                let message = sig_data_to_p2p($messages[sender_id].clone());

                if receiver_id != sender_id {
                    $clients
                        .get_mut(receiver_id)
                        .unwrap()
                        .process_p2p_message(sender_id.clone(), message.clone());
                }
            }
        }
    }};
}

macro_rules! distribute_data_signing_custom {
    ($clients:expr, $messages: expr, $custom_messages: expr) => {{
        let ids = $clients.keys().cloned().collect_vec();

        for sender_id in &ids {
            for receiver_id in &ids {
                let valid_message = $messages[sender_id].clone();
                let message = $custom_messages
                    .remove(&(sender_id.clone(), receiver_id.clone()))
                    .unwrap_or(valid_message);

                let message = sig_data_to_p2p(message);

                if receiver_id != sender_id {
                    $clients
                        .get_mut(receiver_id)
                        .unwrap()
                        .process_p2p_message(sender_id.clone(), message.clone());
                }
            }
        }
    }};
}

pub(super) type MultisigOutcomeReceiver =
    Pin<Box<Peekable<UnboundedReceiverStream<MultisigOutcome>>>>;

pub(super) type P2PMessageReceiver =
    Pin<Box<Peekable<UnboundedReceiverStream<(AccountId, MultisigMessage)>>>>;

pub struct Stage0Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
}

/// Clients generated comm1, but haven't sent them
pub struct CommStage1Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    pub comm1s: HashMap<AccountId, keygen::Comm1>,
}

/// Clients generated ver2, but haven't sent them
pub struct CommVerStage2Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub ver2s: HashMap<AccountId, keygen::VerifyComm2>,
}

/// Clients generated sec3, but haven't sent them
pub struct SecStage3Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    // TODO: change this to a flat hash map
    pub sec3: HashMap<AccountId, HashMap<AccountId, keygen::SecretShare3>>,
}

/// Clients generated complaints, but haven't sent them
pub struct CompStage4Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub comp4s: HashMap<AccountId, keygen::Complaints4>,
}

pub struct VerCompStage5Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub ver5: HashMap<AccountId, keygen::VerifyComplaints5>,
}

pub struct BlameResponses6Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub resp6: HashMap<AccountId, keygen::BlameResponse6>,
}

pub struct VerBlameResponses7Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    /// The key in the map is the index of the destination node
    pub ver7: HashMap<AccountId, keygen::VerifyBlameResponses7>,
}

pub struct KeyReadyData {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    pub pubkey: secp256k1::PublicKey,

    pub sec_keys: HashMap<AccountId, KeygenResultInfo>,
}

impl Debug for KeyReadyData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyReadyData")
            .field("pubkey", &self.pubkey)
            .finish()
    }
}

// TODO: Now that most of these are options, we should
// just merge them into one struct
pub struct ValidKeygenStates {
    pub stage0: Stage0Data,
    pub comm_stage1: CommStage1Data,
    pub ver_com_stage2: CommVerStage2Data,
    pub sec_stage3: Option<SecStage3Data>,
    pub comp_stage4: Option<CompStage4Data>,
    pub ver_comp_stage5: Option<VerCompStage5Data>,
    pub blame_responses6: Option<BlameResponses6Data>,
    pub ver_blame_responses7: Option<VerBlameResponses7Data>,
    /// Either a valid keygen result or an abort reason and list of blamed parties
    pub key_ready: Result<KeyReadyData, (CeremonyAbortReason, Vec<AccountId>)>,
}

impl ValidKeygenStates {
    /// Get the key and associated data asserting
    /// that the ceremony has been successful
    pub fn key_ready_data(&self) -> Option<&KeyReadyData> {
        self.key_ready.as_ref().ok()
    }

    /// Get a clone of the client for `account_id` from the specified stage
    pub fn get_client_at_stage(&self, account_id: &AccountId, stage: usize) -> MultisigClientNoDB {
        match stage {
            0 => self.stage0.clients[account_id].clone(),
            1 => self.comm_stage1.clients[account_id].clone(),
            2 => self.ver_com_stage2.clients[account_id].clone(),
            3 => self.sec_stage3.as_ref().expect("No stage 3").clients[account_id].clone(),
            4 => self.comp_stage4.as_ref().expect("No stage 4").clients[account_id].clone(),
            5 => self.ver_comp_stage5.as_ref().expect("No stage 5").clients[account_id].clone(),
            6 => self
                .blame_responses6
                .as_ref()
                .expect("No blaming stage")
                .clients[account_id]
                .clone(),
            7 => self
                .ver_blame_responses7
                .as_ref()
                .expect("No blaming stage")
                .clients[account_id]
                .clone(),
            _ => panic!("Invalid stage {}", stage),
        }
    }
}

/// Clients received a request to sign and generated (but haven't broadcast) Comm1
pub struct SigningPhase1Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    pub comm1s: HashMap<AccountId, frost::Comm1>,
}

/// Clients generated (but haven't broadcast) VerifyComm2
pub struct SigningPhase2Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    pub ver2s: HashMap<AccountId, frost::VerifyComm2>,
}

/// Clients generated (but haven't broadcast) LocalSig3
pub struct SigningPhase3Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    pub local_sigs: HashMap<AccountId, frost::LocalSig3>,
}

/// Clients generated (but haven't broadcast) VerifyLocalSig4
pub struct SigningPhase4Data {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    pub ver4s: HashMap<AccountId, frost::VerifyLocalSig4>,
}

pub struct ValidSigningStates {
    pub sign_phase1: SigningPhase1Data,
    pub sign_phase2: SigningPhase2Data,
    pub sign_phase3: Option<SigningPhase3Data>,
    pub sign_phase4: Option<SigningPhase4Data>,
    pub outcome: SigningOutcome,
}

impl ValidSigningStates {
    /// Get a clone of the client for `account_id` from the specified stage
    pub fn get_client_at_stage(&self, account_id: &AccountId, stage: usize) -> MultisigClientNoDB {
        match stage {
            1 => self.sign_phase1.clients[account_id].clone(),
            2 => self.sign_phase2.clients[account_id].clone(),
            3 => self.sign_phase3.as_ref().expect("No stage 3").clients[account_id].clone(),
            4 => self.sign_phase4.as_ref().expect("No stage 4").clients[account_id].clone(),
            _ => panic!("Invalid stage {}", stage),
        }
    }
}

pub fn get_stage_for_keygen_ceremony(client: &MultisigClientNoDB) -> Option<String> {
    client
        .ceremony_manager
        .get_keygen_stage_for(KEYGEN_CEREMONY_ID)
}

#[derive(Default)]
struct CustomDataToSend {
    /// Maps a (sender, receiver) pair to the data that will be
    /// sent (in case it needs to be invalid/different from what
    /// is expected normally)
    comm1_signing: HashMap<(AccountId, AccountId), SigningCommitment>,
    comm1_keygen: HashMap<(AccountId, AccountId), DKGUnverifiedCommitment>,
    // Sig3 to send between (sender, receiver) in case we want
    // an invalid sig3 or inconsistent sig3 for tests
    sig3s: HashMap<(AccountId, AccountId), LocalSig3>,
    // Secret shares to send between (sender, receiver) in case it
    // needs to be different from the regular (valid) one
    secret_shares: HashMap<(AccountId, AccountId), SecretShare3>,
    // Secret shares to be broadcast during blaming stage
    secret_shares_blaming: HashMap<(AccountId, AccountId), keygen::BlameResponse6>,
    // Complaints to be broadcast
    complaints: HashMap<(AccountId, AccountId), keygen::Complaints4>,
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
    pub outcome_receivers: HashMap<AccountId, MultisigOutcomeReceiver>,
    pub p2p_receivers: HashMap<AccountId, P2PMessageReceiver>,
    /// This clients will match the ones in `key_ready`,
    /// but stored separately so we could substitute
    /// them in more advanced tests
    clients: HashMap<AccountId, MultisigClientNoDB>,
    /// Maps AccountId to the corresponding signer index
    /// (and vice versa)
    idx_mapping: PartyIdxMapping,
    /// The key that was generated
    key_id: Option<KeyId>,
    // Cache of all tags that used in log calls
    pub tag_cache: TagCache,
    pub auto_clear_tag_cache: bool,
}

fn gen_invalid_local_sig() -> LocalSig3 {
    use crate::multisig::crypto::Scalar;
    frost::LocalSig3 {
        response: Scalar::random(),
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

impl KeygenContext {
    /// Generate context without starting the keygen ceremony.
    /// `allowing_high_pubkey` is enabled so tests will not fail.
    pub fn new() -> Self {
        let account_ids = super::ACCOUNT_IDS.clone();
        KeygenContext::inner_new(account_ids, KeygenOptions::allowing_high_pubkey())
    }

    pub fn new_with_account_ids(
        account_ids: Vec<AccountId>,
        keygen_options: KeygenOptions,
    ) -> Self {
        KeygenContext::inner_new(account_ids, keygen_options)
    }

    /// Generate context with the KeygenOptions as default, (No `allowing_high_pubkey`)
    pub fn new_disallow_high_pubkey() -> Self {
        let account_ids = super::ACCOUNT_IDS.clone();
        KeygenContext::inner_new(account_ids, KeygenOptions::default())
    }

    fn inner_new(account_ids: Vec<AccountId>, keygen_options: KeygenOptions) -> Self {
        let (logger, tag_cache) = logging::test_utils::new_test_logger_with_tag_cache();
        let mut p2p_receivers = HashMap::new();
        let mut clients = HashMap::new();

        let idx_mapping = PartyIdxMapping::from_unsorted_signers(&account_ids);

        let mut outcome_receivers = HashMap::new();

        for id in &account_ids {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let (p2p_tx, p2p_rx) = tokio::sync::mpsc::unbounded_channel();
            let client = MultisigClient::new(
                id.clone(),
                KeyDBMock::new(),
                tx,
                p2p_tx,
                keygen_options,
                &logger,
            );

            clients.insert(id.clone(), client);

            p2p_receivers.insert(
                id.clone(),
                Box::pin(UnboundedReceiverStream::new(p2p_rx).peekable()),
            ); // See KeygenContext TODO
            outcome_receivers.insert(
                id.clone(),
                Box::pin(UnboundedReceiverStream::new(rx).peekable()),
            );
        }

        KeygenContext {
            account_ids,
            outcome_receivers,
            p2p_receivers,
            clients,
            custom_data: Default::default(),
            key_id: None,
            idx_mapping,
            tag_cache,
            auto_clear_tag_cache: true,
        }
    }

    pub fn get_account_ids(&self) -> &[AccountId] {
        &self.account_ids
    }

    /// Get account id based on the index in the initially
    /// provided list of signers (this index has nothing to
    /// do with the "signer index")
    pub fn get_account_id(&self, index: usize) -> AccountId {
        self.account_ids[index].clone()
    }

    pub fn key_id(&self) -> KeyId {
        self.key_id.as_ref().expect("must have key").clone()
    }

    pub fn get_client(&self, id: &AccountId) -> &MultisigClientNoDB {
        &self.clients[id]
    }

    pub fn use_invalid_local_sig(&mut self, sender_id: &AccountId) {
        let fake_sig3 = gen_invalid_local_sig();

        for receiver_id in &self.account_ids {
            if sender_id != receiver_id {
                self.custom_data
                    .sig3s
                    .insert((sender_id.clone(), receiver_id.clone()), fake_sig3.clone());
            }
        }
    }

    pub fn use_invalid_secret_share(&mut self, sender_id: &AccountId, receiver_id: &AccountId) {
        assert_ne!(sender_id, receiver_id);

        let invalid_share = SecretShare3::create_random();

        self.custom_data
            .secret_shares
            .insert((sender_id.clone(), receiver_id.clone()), invalid_share);
    }

    pub fn use_invalid_complaint(&mut self, sender_id: &AccountId) {
        // This complaint is invalid because it contains an invalid index
        let complaint = keygen::Complaints4(vec![1, usize::MAX]);
        for receiver_id in &self.account_ids {
            if sender_id != receiver_id {
                self.custom_data
                    .complaints
                    .insert((sender_id.clone(), receiver_id.clone()), complaint.clone());
            }
        }
    }

    pub fn use_invalid_blame_response(&mut self, sender_id: &AccountId, receiver_id: &AccountId) {
        assert_ne!(sender_id, receiver_id);

        // It does not matter whether this invalid share is the same
        // as the invalid share sent earlier (prior to blaming)

        let invalid_response = {
            let invalid_share = SecretShare3::create_random();
            let mut response = keygen::BlameResponse6(HashMap::default());

            let receiver_idx = self
                .idx_mapping
                .get_idx(&receiver_id)
                .expect("unexpected account id");

            response.0.insert(receiver_idx, invalid_share);
            response
        };

        // Send the same invalid response to all other parties
        for receiver_id in &self.account_ids {
            if sender_id != receiver_id {
                self.custom_data.secret_shares_blaming.insert(
                    (sender_id.clone(), receiver_id.clone()),
                    invalid_response.clone(),
                );
            }
        }
    }

    pub fn use_inconsistent_broadcast_for_signing_comm1(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
    ) {
        assert_ne!(sender_id, receiver_id);

        // It doesn't matter what kind of commitment we create here,
        // the main idea is that the commitment doesn't match what we
        // send to all other parties
        let fake_comm1 = SigningCommitment {
            index: 0,
            d: Point::random(),
            e: Point::random(),
        };

        self.custom_data
            .comm1_signing
            .insert((sender_id.clone(), receiver_id.clone()), fake_comm1);
    }

    /// Make the specified node send a new random commitment to the receiver
    pub fn use_inconsistent_broadcast_for_keygen_comm1(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
    ) {
        assert_ne!(sender_id, receiver_id);
        self.custom_data.comm1_keygen.insert(
            (sender_id.clone(), receiver_id.clone()),
            gen_invalid_keygen_comm1(),
        );
    }

    /// Make the specified node send an invalid commitment to all of the other account_ids
    pub fn use_invalid_keygen_comm1(&mut self, sender_id: AccountId) {
        let fake_comm1 = gen_invalid_keygen_comm1();

        for receiver_id in &self.account_ids {
            if &sender_id != receiver_id {
                self.custom_data
                    .comm1_keygen
                    .insert((sender_id.clone(), receiver_id.clone()), fake_comm1.clone());
            }
        }
    }

    pub fn use_inconsistent_broadcast_for_sig3(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
    ) {
        assert_ne!(sender_id, receiver_id);

        // It doesn't matter what kind of local sig we create here,
        // the main idea is that it doesn't match what we
        // send to all other parties
        let fake_sig3 = gen_invalid_local_sig();

        self.custom_data
            .sig3s
            .insert((sender_id.clone(), receiver_id.clone()), fake_sig3);
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

        for (_id, c) in clients.iter_mut() {
            c.process_multisig_instruction(MultisigInstruction::Keygen(keygen_info.clone()));
        }

        let comm1s = recv_all_data_keygen!(p2p_rxs, KeygenData::Comm1);

        println!("Received all comm1");

        let comm_stage1 = CommStage1Data {
            clients: clients.clone(),
            comm1s: comm1s.clone(),
        };

        distribute_data_keygen_custom!(
            clients,
            self.account_ids,
            &comm1s,
            &mut self.custom_data.comm1_keygen
        );

        println!("Distributed all comm1");

        clients
            .values()
            .for_each(|c| assert_ok!(c.ensure_at_keygen_stage(2)));

        let ver2s = recv_all_data_keygen!(p2p_rxs, KeygenData::Verify2);

        let ver_com_stage2 = CommVerStage2Data {
            clients: clients.clone(),
            ver2s: ver2s.clone(),
        };

        // *** Distribute VerifyComm2s, so we can advance and generate Secret3 ***

        distribute_data_keygen!(clients, self.account_ids, ver2s);

        if !clients
            .values()
            .next()
            .unwrap()
            .ensure_at_keygen_stage(3)
            .is_ok()
        {
            // The ceremony failed early, gather the result and reported_nodes, then return
            let mut results = vec![];
            for mut r in rxs.values_mut() {
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

            if self.auto_clear_tag_cache {
                self.tag_cache.clear();
            }

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
            .values()
            .for_each(|c| assert_ok!(c.ensure_at_keygen_stage(3)));

        // *** Collect all Secret3

        let mut sec3s: HashMap<AccountId, _> = HashMap::new();

        for (sender_id, rx) in p2p_rxs.iter_mut() {
            let mut sec3_map = HashMap::new();
            for i in 0..self.account_ids.len() - 1 {
                println!("recv secret3 keygen, i: {}", i);
                let (dest, sec3) = recv_secret3_keygen(rx).await;
                sec3_map.insert(dest, sec3);
            }

            sec3s.insert(sender_id.clone(), sec3_map);
        }

        println!("Received all sec3");

        let sec_stage3 = Some(SecStage3Data {
            clients: clients.clone(),
            sec3: sec3s.clone(),
        });

        // Distribute secret 3
        for sender_id in &self.account_ids {
            for receiver_id in &self.account_ids {
                if sender_id != receiver_id {
                    let valid_sec3 = sec3s[sender_id].get(receiver_id).unwrap();

                    let sec3 = self
                        .custom_data
                        .secret_shares
                        .remove(&(sender_id.clone(), receiver_id.clone()))
                        .unwrap_or(valid_sec3.clone());

                    let message = keygen_data_to_p2p(sec3.clone(), KEYGEN_CEREMONY_ID);

                    clients
                        .get_mut(receiver_id)
                        .unwrap()
                        .process_p2p_message(sender_id.clone(), message);
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
        let nodes_entered_blaming = clients.values().all(|c| {
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
            for rx in rxs.values_mut() {
                let result = match expect_next_with_timeout(rx).await {
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

                let mut sec_keys = HashMap::new();

                let key_id = KeyId(pubkeys[0].serialize().into());
                self.key_id = Some(key_id.clone());

                for (id, c) in clients.iter() {
                    let key = c.get_key(&key_id).expect("key must be present");
                    sec_keys.insert(id.clone(), key.clone());
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
            for rx in rxs.values_mut() {
                assert_channel_empty(rx).await;
            }

            if self.auto_clear_tag_cache {
                self.tag_cache.clear();
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
        id: &AccountId,
        client: MultisigClientNoDB,
        rx: MultisigOutcomeReceiver,
        p2p_rx: P2PMessageReceiver,
    ) {
        self.clients.insert(id.clone(), client);
        self.outcome_receivers.insert(id.clone(), rx);
        self.p2p_receivers.insert(id.clone(), p2p_rx);
    }

    pub async fn sign(&mut self) -> ValidSigningStates {
        self.sign_with_ids(&*SIGNER_IDS).await
    }

    // Use the generated key and the clients participating
    // in the ceremony and sign a message producing state
    // for each of the signing phases
    pub async fn sign_with_ids(&mut self, signer_ids: &[AccountId]) -> ValidSigningStates {
        let instant = std::time::Instant::now();

        let sign_info = SigningInfo::new(
            SIGN_CEREMONY_ID,
            self.key_id(),
            MESSAGE_HASH.clone(),
            signer_ids.to_owned(),
        );

        // Only select clients will participate
        let mut clients: HashMap<_, _> = self
            .clients
            .clone()
            .into_iter()
            .filter(|(id, _)| signer_ids.contains(id))
            .collect();

        let mut outcome_rxs: HashMap<_, _> = self
            .outcome_receivers
            .iter_mut()
            .filter(|(id, _)| signer_ids.contains(id))
            .map(|(id, rx)| (id.clone(), rx))
            .collect();

        let mut p2p_rxs: HashMap<_, _> = self
            .p2p_receivers
            .iter_mut()
            .filter(|(id, _)| signer_ids.contains(id))
            .map(|(id, rx)| (id.clone(), rx))
            .collect();

        for mut rx in p2p_rxs.values_mut() {
            assert_channel_empty(&mut rx).await;
        }

        // *** Send a request to sign and generate BC1 to be distributed ***

        // NOTE: only parties 0, 1 and 2 will participate in signing (SIGNER_IDXS)
        for c in clients.values_mut() {
            c.process_multisig_instruction(MultisigInstruction::Sign(sign_info.clone()));

            assert_ok!(c.ensure_at_signing_stage(1));
        }

        let comm1s = recv_all_data_signing!(p2p_rxs, SigningData::CommStage1);

        let sign_phase1 = SigningPhase1Data {
            clients: clients.clone(),
            comm1s: comm1s.clone(),
        };

        // *** Broadcast Comm1 messages to advance to Stage2 ***
        distribute_data_signing_custom!(&mut clients, &comm1s, &mut self.custom_data.comm1_signing);

        clients
            .values()
            .for_each(|c| assert_ok!(c.ensure_at_signing_stage(2)));

        // *** Collect Ver2 messages ***
        let ver2s = recv_all_data_signing!(p2p_rxs, SigningData::BroadcastVerificationStage2);

        let sign_phase2 = SigningPhase2Data {
            clients: clients.clone(),
            ver2s: ver2s.clone(),
        };

        // *** Distribute Ver2 messages ***

        distribute_data_signing!(&mut clients, &ver2s);

        // Check if the ceremony was aborted at this stage
        if let Some(outcome) = check_and_get_signing_outcome(&mut outcome_rxs).await {
            if self.auto_clear_tag_cache {
                self.tag_cache.clear();
            }

            // The ceremony was aborted early,
            return ValidSigningStates {
                sign_phase1,
                sign_phase2,
                sign_phase3: None,
                sign_phase4: None,
                outcome: outcome,
            };
        }

        clients
            .values()
            .for_each(|c| assert_ok!(c.ensure_at_signing_stage(3)));

        // *** Collect local sigs ***

        let local_sigs = recv_all_data_signing!(p2p_rxs, SigningData::LocalSigStage3);

        let sign_phase3 = SigningPhase3Data {
            clients: clients.clone(),
            local_sigs: local_sigs.clone(),
        };

        // *** Distribute local sigs ***
        distribute_data_signing_custom!(&mut clients, &local_sigs, &mut self.custom_data.sig3s);

        // *** Collect Ver4 messages ***
        let ver4s = recv_all_data_signing!(p2p_rxs, SigningData::VerifyLocalSigsStage4);

        let sign_phase4 = SigningPhase4Data {
            clients: clients.clone(),
            ver4s: ver4s.clone(),
        };

        // *** Distribute Ver4 messages ***

        distribute_data_signing!(&mut clients, &ver4s);

        if let Some(outcome) = check_and_get_signing_outcome(&mut outcome_rxs).await {
            println!("Signing ceremony took: {:?}", instant.elapsed());

            // Make sure the channel is clean for the unit tests
            for rx in p2p_rxs.values_mut() {
                assert_channel_empty(rx).await;
            }

            if self.auto_clear_tag_cache {
                self.tag_cache.clear();
            }

            // Verify the signature with the key
            if let Ok(sig) = &outcome.result {
                verify_sig_with_aggkey(sig, self.key_id.as_ref().expect("should have key"))
                    .expect("Should be valid signature");
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

    /// Run the keygen ceremony and check that the failure details match the expected details
    pub async fn run_keygen_and_check_failure(
        mut self,
        expected_reason: CeremonyAbortReason,
        expected_reported_nodes: Vec<AccountId>,
        expected_tag: &str,
    ) -> Result<ValidKeygenStates> {
        // Run the keygen ceremony
        let keygen_states = self.generate().await;

        // Check that it failed
        if keygen_states.key_ready.is_ok() {
            return Err(anyhow::Error::msg("Keygen did not fail"));
        }

        let (reason, reported) = keygen_states.key_ready.as_ref().unwrap_err().clone();

        // Check that the failure reason matches
        if reason != expected_reason {
            return Err(anyhow::Error::msg(format!(
                "Incorrect keygen failure reason: {:?}, expected: {:?}",
                reason, expected_reason
            )));
        }

        // Check that the reported nodes match
        let reported_nodes_sorted: Vec<AccountId> = reported.iter().sorted().cloned().collect();
        let expected_nodes_sorted: Vec<AccountId> =
            expected_reported_nodes.iter().sorted().cloned().collect();
        if reported_nodes_sorted != expected_nodes_sorted {
            return Err(anyhow::Error::msg(format!(
                "Incorrect reported nodes: {:?}, expected: {:?}",
                reported_nodes_sorted, expected_nodes_sorted
            )));
        }

        // Check that the expected tag was logged
        if !self.tag_cache.contains_tag(expected_tag) {
            return Err(anyhow::Error::msg(format!(
                "Didn't find the expected tag: {}",
                expected_tag,
            )));
        }

        Ok(keygen_states)
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
    rxs: &mut HashMap<AccountId, &mut MultisigOutcomeReceiver>,
) -> Option<SigningOutcome> {
    let mut outcomes: Vec<SigningOutcome> = Vec::new();

    for mut rx in rxs.values_mut() {
        if let Some(outcome) = peek_with_timeout(&mut rx).await.and_then(|outcome| {
            if let MultisigOutcome::Signing(outcome) = outcome {
                Some(outcome)
            } else {
                None
            }
        }) {
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
        assert_eq!(outcomes.len(), rxs.len(), "Not all signers got an outcome");

        for outcome in outcomes.iter() {
            assert_eq!(outcome, &outcomes[0], "Outcome different between signers");
        }

        // Consume the outcome message if its all good
        for mut rx in rxs.values_mut() {
            next_with_timeout(&mut rx).await;
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
        None => panic!(
            "Timeout waiting for message, expected {}",
            std::any::type_name::<I>()
        ),
    }
}

async fn recv_secret3_keygen(rx: &mut P2PMessageReceiver) -> (AccountId, keygen::SecretShare3) {
    if let (
        dest,
        MultisigMessage {
            data: MultisigData::Keygen(KeygenData::SecretShares3(sec3)),
            ..
        },
    ) = expect_next_with_timeout(rx).await
    {
        return (dest, sec3);
    } else {
        panic!("Received message is not Secret3 (keygen)");
    }
}

pub fn sig_data_to_p2p(data: impl Into<SigningData>) -> MultisigMessage {
    MultisigMessage {
        ceremony_id: SIGN_CEREMONY_ID,
        data: MultisigData::Signing(data.into()),
    }
}

pub fn keygen_data_to_p2p(data: impl Into<KeygenData>, ceremony_id: CeremonyId) -> MultisigMessage {
    MultisigMessage {
        ceremony_id,
        data: MultisigData::Keygen(data.into()),
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
    pub fn ensure_at_signing_stage(&self, stage_number: usize) -> Result<()> {
        let stage = get_stage_for_signing_ceremony(self);
        let is_at_stage = match stage_number {
            0 => stage == None,
            1 => stage.as_deref() == Some("BroadcastStage<AwaitCommitments1>"),
            2 => stage.as_deref() == Some("BroadcastStage<VerifyCommitmentsBroadcast2>"),
            3 => stage.as_deref() == Some("BroadcastStage<LocalSigStage3>"),
            4 => stage.as_deref() == Some("BroadcastStage<VerifyLocalSigsBroadcastStage4>"),
            _ => false,
        };
        if is_at_stage {
            Ok(())
        } else {
            Err(anyhow::Error::msg(format!(
                "Expected to be at stage {}, but actually at stage {:?}",
                stage_number, stage
            )))
        }
    }

    /// Check is the client is at the specified keygen BroadcastStage (0-5).
    /// 0 = No Stage
    /// 1 = AwaitCommitments1 ... and so on
    pub fn ensure_at_keygen_stage(&self, stage_number: usize) -> Result<()> {
        let stage = get_stage_for_keygen_ceremony(self);
        let is_at_stage = match stage_number {
            0 => stage == None,
            1 => stage.as_deref() == Some("BroadcastStage<AwaitCommitments1>"),
            2 => stage.as_deref() == Some("BroadcastStage<VerifyCommitmentsBroadcast2>"),
            3 => stage.as_deref() == Some("BroadcastStage<SecretSharesStage3>"),
            4 => stage.as_deref() == Some("BroadcastStage<ComplaintsStage4>"),
            5 => stage.as_deref() == Some("BroadcastStage<VerifyComplaintsBroadcastStage5>"),
            6 => stage.as_deref() == Some("BroadcastStage<BlameResponsesStage6>"),
            7 => stage.as_deref() == Some("BroadcastStage<VerifyBlameResponsesBroadcastStage7>"),
            _ => false,
        };
        if is_at_stage {
            Ok(())
        } else {
            Err(anyhow::Error::msg(format!(
                "Expected to be at stage {}, but actually at stage {:?}",
                stage_number, stage
            )))
        }
    }

    /// Sends the correct keygen data from the `ACCOUNT_IDS[sender_idx]` to the client via `process_p2p_message`
    pub fn receive_keygen_stage_data(
        &mut self,
        stage: usize,
        keygen_states: &ValidKeygenStates,
        sender_id: &AccountId,
    ) {
        let message = self.get_keygen_p2p_message_for_stage(stage, keygen_states, sender_id);
        self.process_p2p_message(sender_id.clone(), message);
    }

    /// Makes a P2PMessage using the keygen data for the specified stage
    pub fn get_keygen_p2p_message_for_stage(
        &mut self,
        stage: usize,
        keygen_states: &ValidKeygenStates,
        sender_id: &AccountId,
    ) -> MultisigMessage {
        match stage {
            1 => keygen_data_to_p2p(
                keygen_states.comm_stage1.comm1s[sender_id].clone(),
                KEYGEN_CEREMONY_ID,
            ),
            2 => keygen_data_to_p2p(
                keygen_states.ver_com_stage2.ver2s[sender_id].clone(),
                KEYGEN_CEREMONY_ID,
            ),
            3 => {
                let sec3 = keygen_states.sec_stage3.as_ref().expect("No stage 3").sec3[sender_id]
                    .get(&self.get_my_account_id())
                    .unwrap();
                keygen_data_to_p2p(sec3.clone(), KEYGEN_CEREMONY_ID)
            }
            4 => keygen_data_to_p2p(
                keygen_states
                    .comp_stage4
                    .as_ref()
                    .expect("No stage 4")
                    .comp4s[sender_id]
                    .clone(),
                KEYGEN_CEREMONY_ID,
            ),
            5 => keygen_data_to_p2p(
                keygen_states
                    .ver_comp_stage5
                    .as_ref()
                    .expect("No stage 5")
                    .ver5[sender_id]
                    .clone(),
                KEYGEN_CEREMONY_ID,
            ),
            6 => keygen_data_to_p2p(
                keygen_states
                    .blame_responses6
                    .as_ref()
                    .expect("No blaming stage 6")
                    .resp6[sender_id]
                    .clone(),
                KEYGEN_CEREMONY_ID,
            ),
            7 => keygen_data_to_p2p(
                keygen_states
                    .ver_blame_responses7
                    .as_ref()
                    .expect("No blaming stage 7")
                    .ver7[sender_id]
                    .clone(),
                KEYGEN_CEREMONY_ID,
            ),
            _ => panic!("Invalid stage to receive message, stage: {}", stage),
        }
    }

    /// Sends the correct singing data from the `ACCOUNT_IDS[sender_idx]` to the client via `process_p2p_message`
    pub fn receive_signing_stage_data(
        &mut self,
        stage: usize,
        sign_states: &ValidSigningStates,
        sender_id: &AccountId,
    ) {
        let message = self.get_signing_p2p_message_for_stage(stage, sign_states, sender_id);
        self.process_p2p_message(sender_id.clone(), message);
    }

    /// Makes a P2PMessage using the signing data for the specified stage
    pub fn get_signing_p2p_message_for_stage(
        &mut self,
        stage: usize,
        sign_states: &ValidSigningStates,
        sender_id: &AccountId,
    ) -> MultisigMessage {
        match stage {
            1 => sig_data_to_p2p(sign_states.sign_phase1.comm1s[sender_id].clone()),
            2 => sig_data_to_p2p(sign_states.sign_phase2.ver2s[sender_id].clone()),
            3 => sig_data_to_p2p(
                sign_states
                    .sign_phase3
                    .as_ref()
                    .expect("No signing stage 3")
                    .local_sigs[sender_id]
                    .clone(),
            ),
            4 => sig_data_to_p2p(
                sign_states
                    .sign_phase4
                    .as_ref()
                    .expect("No signing stage 4")
                    .ver4s[sender_id]
                    .clone(),
            ),
            _ => panic!("Invalid stage to receive message, stage: {}", stage),
        }
    }
}

pub async fn check_blamed_paries(rx: &mut MultisigOutcomeReceiver, expected: &[AccountId]) {
    let blamed_parties = match peek_with_timeout(rx)
        .await
        .as_ref()
        .expect("expected multisig_outcome")
    {
        MultisigOutcome::Signing(outcome) => &outcome.result.as_ref().unwrap_err().1,
        MultisigOutcome::Keygen(outcome) => &outcome.result.as_ref().unwrap_err().1,
        MultisigOutcome::Ignore => {
            panic!("Cannot check blamed parties on an ignored request");
        }
    };

    assert_eq!(&blamed_parties[..], expected);
}

/// Using the given key_id, verify the signature is correct
pub fn verify_sig_with_aggkey(sig: &SchnorrSignature, key_id: &KeyId) -> Result<()> {
    // Get the aggkey
    let pk_ser: &[u8; 33] = key_id.0[..].try_into().unwrap();
    let agg_key = AggKey::from_pubkey_compressed(pk_ser.clone());

    // Verify the signature with the aggkey
    agg_key
        .verify(
            &MESSAGE_HASH.0,
            &SchnorrVerificationComponents::from(sig.clone()),
        )
        .map_err(|e| anyhow::Error::msg(format!("Failed to verify signature: {:?}", e)))?;

    Ok(())
}
