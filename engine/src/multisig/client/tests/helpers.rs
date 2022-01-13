use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
    fmt::Debug,
    pin::Pin,
    time::Duration,
};

use anyhow::Result;
use cf_chains::eth::{AggKey, SchnorrVerificationComponents};
use futures::{
    stream::{self, Peekable},
    StreamExt,
};
use itertools::Itertools;

use pallet_cf_vaults::CeremonyId;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{
    common::format_iterator,
    multisig::{
        client::{
            keygen::{HashContext, KeygenOptions, SecretShare3},
            signing,
            utils::PartyIdxMapping,
            CeremonyAbortReason, MultisigData, ThresholdParameters,
        },
        KeyId, MultisigInstruction, SchnorrSignature,
    },
    multisig_p2p::OutgoingMultisigStageMessages,
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
};

use state_chain_runtime::AccountId;

pub type MultisigClientNoDB = MultisigClient<KeyDBMock>;

use super::{ACCOUNT_IDS, KEYGEN_CEREMONY_ID, MESSAGE_HASH, SIGNER_IDS, SIGN_CEREMONY_ID};

pub const STAGE_FINISHED: usize = 0;

macro_rules! recv_broadcast {
    ($rxs:expr, $ceremony_variant: path, $variant: path) => {{
        futures::stream::iter($rxs.iter_mut())
            .then(|(id, rx)| async move {
                (
                    id.clone(),
                    match expect_next_with_timeout(rx).await {
                        OutgoingMultisigStageMessages::Broadcast(
                            _,
                            MultisigMessage {
                                data: $ceremony_variant($variant(inner)),
                                ..
                            },
                        ) => {
                            assert_channel_empty(rx).await;
                            inner
                        }
                        _ => {
                            panic!("Received message is not {}", stringify!($variant));
                        }
                    },
                )
            })
            .collect::<HashMap<_, _>>()
            .await
    }};
}

macro_rules! recv_keygen_broadcast {
    ($rxs:expr, $variant: path) => {
        recv_broadcast!($rxs, MultisigData::Keygen, $variant)
    };
}

macro_rules! recv_siging_broadcast {
    ($rxs:expr, $variant: path) => {
        recv_broadcast!($rxs, MultisigData::Signing, $variant)
    };
}

macro_rules! distribute_data_keygen_custom {
    ($clients:expr, $account_ids: expr, $messages: expr, $custom_messages: expr, $should_timeout: expr, $ceremony_id: expr, $stage_idx: expr) => {{
        for sender_id in &$account_ids {
            for receiver_id in &$account_ids {
                if receiver_id == sender_id {
                    continue;
                }

                if $should_timeout.contains(&(sender_id.clone(), receiver_id.clone(), $stage_idx)) {
                    continue;
                }

                let valid_message = $messages[&sender_id].clone();

                let message = $custom_messages
                    .remove(&(sender_id.clone(), receiver_id.clone()))
                    .unwrap_or(valid_message);

                let message = keygen_data_to_p2p_with_ceremony_id(message, $ceremony_id);

                $clients
                    .get_mut(receiver_id)
                    .unwrap()
                    .process_p2p_message(sender_id.clone(), message.clone());
            }
        }
    }};
}

macro_rules! distribute_data_signing_custom {
    ($clients:expr, $messages: expr, $custom_messages: expr, $should_timeout: expr, $stage_idx: expr) => {{
        let ids = $clients.keys().cloned().collect_vec();

        for sender_id in &ids {
            for receiver_id in &ids {
                if receiver_id == sender_id {
                    continue;
                }

                if $should_timeout.contains(&(sender_id.clone(), receiver_id.clone(), $stage_idx)) {
                    continue;
                }

                let valid_message = $messages[sender_id].clone();
                let message = $custom_messages
                    .remove(&(sender_id.clone(), receiver_id.clone()))
                    .unwrap_or(valid_message);

                let message = sig_data_to_p2p(message);

                $clients
                    .get_mut(receiver_id)
                    .unwrap()
                    .process_p2p_message(sender_id.clone(), message.clone());
            }
        }
    }};
}

pub(super) type MultisigOutcomeReceiver =
    Pin<Box<Peekable<UnboundedReceiverStream<MultisigOutcome>>>>;

pub(super) type P2PMessageReceiver =
    Pin<Box<Peekable<UnboundedReceiverStream<OutgoingMultisigStageMessages>>>>;

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

/// The outcome of the ceremony and the Clients at the state
/// when the outcome was generated,
pub struct SigningFinishedData {
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
    pub outcome: SigningOutcome,
}

pub struct ValidSigningStates {
    pub sign_phase1: SigningPhase1Data,
    pub sign_phase2: SigningPhase2Data,
    pub sign_phase3: Option<SigningPhase3Data>,
    pub sign_phase4: Option<SigningPhase4Data>,
    pub sign_finished: SigningFinishedData,
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

pub fn get_stage_for_keygen_ceremony(
    client: &MultisigClientNoDB,
    ceremony_id: &CeremonyId,
) -> Option<String> {
    client.ceremony_manager.get_keygen_stage_for(*ceremony_id)
}

// TODO: all of these should probably be collapsed into a single
// hashmap with stage as one of the keys
#[derive(Default)]
struct CustomDataToSendKeygen {
    comm1: HashMap<(AccountId, AccountId), keygen::Comm1>,
    ver2: HashMap<(AccountId, AccountId), keygen::VerifyComm2>,
    secret_shares: HashMap<(AccountId, AccountId), keygen::SecretShare3>,
    complaints: HashMap<(AccountId, AccountId), keygen::Complaints4>,
    ver5: HashMap<(AccountId, AccountId), keygen::VerifyComplaints5>,
    secret_shares_blaming: HashMap<(AccountId, AccountId), keygen::BlameResponse6>,
    ver7: HashMap<(AccountId, AccountId), keygen::VerifyBlameResponses7>,
}

// derive_impls_for_signing_data!(Comm1, SigningData::CommStage1);
// derive_impls_for_signing_data!(VerifyComm2, SigningData::BroadcastVerificationStage2);
// derive_impls_for_signing_data!(LocalSig3, SigningData::LocalSigStage3);
// derive_impls_for_signing_data!(VerifyLocalSig4, SigningData::VerifyLocalSigsStage4);

#[derive(Default)]
struct CustomDataToSendSigning {
    /// Maps a (sender, receiver) pair to the data that will be
    /// sent (in case it needs to be invalid/different from what
    /// is expected normally)
    comm1: HashMap<(AccountId, AccountId), frost::Comm1>,

    ver2: HashMap<(AccountId, AccountId), frost::VerifyComm2>,
    // Sig3 to send between (sender, receiver) in case we want
    // an invalid sig3 or inconsistent sig3 for tests
    sig3s: HashMap<(AccountId, AccountId), LocalSig3>,

    ver4: HashMap<(AccountId, AccountId), frost::VerifyLocalSig4>,
}

#[derive(Default)]
struct CustomDataToSend {
    keygen: CustomDataToSendKeygen,
    signing: CustomDataToSendSigning,
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
    /// Controls a which singing stage (if any) a party should
    /// timeout (fail to send messages to peers)
    should_timeout_signing: HashSet<(AccountId, AccountId, usize)>,
    /// Controls a which keygen stage (if any) a party should
    /// timeout (fail to send messages to peers)
    should_timeout_keygen: HashSet<(AccountId, AccountId, usize)>,
    pub outcome_receivers: HashMap<AccountId, MultisigOutcomeReceiver>,
    pub p2p_receivers: HashMap<AccountId, P2PMessageReceiver>,
    /// This clients will match the ones in `key_ready`,
    /// but stored separately so we could substitute
    /// them in more advanced tests
    pub clients: HashMap<AccountId, MultisigClientNoDB>,
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

pub fn gen_invalid_keygen_comm1() -> DKGUnverifiedCommitment {
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

pub fn gen_invalid_signing_comm1() -> SigningCommitment {
    SigningCommitment {
        index: 0,
        d: Point::random(),
        e: Point::random(),
    }
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
            );
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
            should_timeout_signing: Default::default(),
            should_timeout_keygen: Default::default(),
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
                    .signing
                    .sig3s
                    .insert((sender_id.clone(), receiver_id.clone()), fake_sig3.clone());
            }
        }
    }

    pub fn use_invalid_secret_share(&mut self, sender_id: &AccountId, receiver_id: &AccountId) {
        assert_ne!(sender_id, receiver_id);

        let invalid_share = SecretShare3::create_random();

        self.custom_data
            .keygen
            .secret_shares
            .insert((sender_id.clone(), receiver_id.clone()), invalid_share);
    }

    pub fn use_invalid_complaint(&mut self, sender_id: &AccountId) {
        // This complaint is invalid because it contains an invalid index
        let complaint = keygen::Complaints4(vec![1, usize::MAX]);
        for receiver_id in &self.account_ids {
            if sender_id != receiver_id {
                self.custom_data
                    .keygen
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
                self.custom_data.keygen.secret_shares_blaming.insert(
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
        self.custom_data.signing.comm1.insert(
            (sender_id.clone(), receiver_id.clone()),
            gen_invalid_signing_comm1(),
        );
    }

    /// Make the specified node send a new random commitment to the receiver
    pub fn use_inconsistent_broadcast_for_keygen_comm1(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
    ) {
        assert_ne!(sender_id, receiver_id);
        self.custom_data.keygen.comm1.insert(
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
                    .keygen
                    .comm1
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
            .signing
            .sig3s
            .insert((sender_id.clone(), receiver_id.clone()), fake_sig3);
    }

    // TODO: use enum for stage idx
    /// Force `sender_id` to appear as offline to `receiver_id` during `stage_idx`;
    /// appear offline to all nodes if `receiver_id` is None
    pub fn force_party_timeout_signing(
        &mut self,
        sender_id: &AccountId,
        receiver_id: Option<&AccountId>,
        stage_idx: usize,
    ) {
        if let Some(receiver_id) = receiver_id {
            self.should_timeout_signing
                .insert((sender_id.clone(), receiver_id.clone(), stage_idx));
        } else {
            for receiver_id in &self.account_ids {
                if sender_id != receiver_id {
                    self.should_timeout_signing.insert((
                        sender_id.clone(),
                        receiver_id.clone(),
                        stage_idx,
                    ));
                }
            }
        }
    }

    // TODO: combine this with the above (maybe when stage_idx is enum?)
    pub fn force_party_timeout_keygen(
        &mut self,
        sender_id: &AccountId,
        receiver_id: Option<&AccountId>,
        stage_idx: usize,
    ) {
        if let Some(receiver_id) = receiver_id {
            self.should_timeout_keygen
                .insert((sender_id.clone(), receiver_id.clone(), stage_idx));
        } else {
            for receiver_id in &self.account_ids {
                if sender_id != receiver_id {
                    self.should_timeout_keygen.insert((
                        sender_id.clone(),
                        receiver_id.clone(),
                        stage_idx,
                    ));
                }
            }
        }
    }

    // Generate keygen states for each of the phases,
    // resulting in `KeygenContext` which can be used
    // to sign messages
    pub async fn generate(&mut self) -> ValidKeygenStates {
        self.generate_with_ceremony_id(KEYGEN_CEREMONY_ID).await
    }

    // Generate keygen states using the specified ceremony id
    pub async fn generate_with_ceremony_id(
        &mut self,
        ceremony_id: CeremonyId,
    ) -> ValidKeygenStates {
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
            ceremony_id,
            signers: account_ids.clone(),
        };

        for (_id, c) in clients.iter_mut() {
            c.process_multisig_instruction(MultisigInstruction::Keygen(keygen_info.clone()));
        }

        let comm1s = recv_keygen_broadcast!(p2p_rxs, KeygenData::Comm1);

        println!("Received all comm1");

        let comm_stage1 = CommStage1Data {
            clients: clients.clone(),
            comm1s: comm1s.clone(),
        };

        distribute_data_keygen_custom!(
            clients,
            self.account_ids,
            &comm1s,
            &mut self.custom_data.keygen.comm1,
            &mut self.should_timeout_keygen,
            ceremony_id,
            1
        );

        clients.values_mut().for_each(|c| {
            c.ensure_stage_finalized_keygen(ceremony_id, "BroadcastStage<AwaitCommitments1>")
        });

        println!("Distributed all comm1");

        clients
            .values()
            .for_each(|c| assert_ok!(c.ensure_ceremony_at_keygen_stage(2, &ceremony_id)));

        let ver2s = recv_keygen_broadcast!(p2p_rxs, KeygenData::Verify2);

        let ver_com_stage2 = CommVerStage2Data {
            clients: clients.clone(),
            ver2s: ver2s.clone(),
        };

        // *** Distribute VerifyComm2s, so we can advance and generate Secret3 ***

        distribute_data_keygen_custom!(
            clients,
            self.account_ids,
            ver2s,
            self.custom_data.keygen.ver2,
            self.should_timeout_keygen,
            ceremony_id,
            2
        );

        clients.values_mut().for_each(|c| {
            c.ensure_stage_finalized_keygen(
                ceremony_id,
                "BroadcastStage<VerifyCommitmentsBroadcast2>",
            )
        });

        if !clients
            .values()
            .next()
            .unwrap()
            .ensure_ceremony_at_keygen_stage(3, &ceremony_id)
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
            .for_each(|c| assert_ok!(c.ensure_ceremony_at_keygen_stage(3, &ceremony_id)));

        // *** Collect all Secret3

        let sec3s = recv_keygen_secret3(p2p_rxs).await;

        println!("Received all sec3");

        let sec_stage3 = Some(SecStage3Data {
            clients: clients.clone(),
            sec3: sec3s.clone(),
        });

        // TODO: this should use distribute_data_keygen_custom or something similar
        // Distribute secret 3
        for sender_id in &self.account_ids {
            for receiver_id in &self.account_ids {
                if sender_id != receiver_id {
                    let valid_sec3 = sec3s[sender_id].get(receiver_id).unwrap();

                    if self.should_timeout_keygen.contains(&(
                        sender_id.clone(),
                        receiver_id.clone(),
                        3, /* stage_idx */
                    )) {
                        continue;
                    }

                    let sec3 = self
                        .custom_data
                        .keygen
                        .secret_shares
                        .remove(&(sender_id.clone(), receiver_id.clone()))
                        .unwrap_or(valid_sec3.clone());

                    let message = keygen_data_to_p2p_with_ceremony_id(sec3.clone(), ceremony_id);

                    clients
                        .get_mut(receiver_id)
                        .unwrap()
                        .process_p2p_message(sender_id.clone(), message);
                }
            }
        }

        println!("Distributed all sec3");

        let complaints = recv_keygen_broadcast!(p2p_rxs, KeygenData::Complaints4);

        let comp_stage4 = Some(CompStage4Data {
            clients: clients.clone(),
            comp4s: complaints.clone(),
        });

        println!("Collected all complaints");

        distribute_data_keygen_custom!(
            clients,
            self.account_ids,
            complaints,
            self.custom_data.keygen.complaints,
            self.should_timeout_keygen,
            ceremony_id,
            4
        );

        println!("Distributed all complaints");

        let ver_complaints = recv_keygen_broadcast!(p2p_rxs, KeygenData::VerifyComplaints5);

        let ver_comp_stage5 = Some(VerCompStage5Data {
            clients: clients.clone(),
            ver5: ver_complaints.clone(),
        });

        println!("Collected all verify complaints");

        distribute_data_keygen_custom!(
            clients,
            self.account_ids,
            ver_complaints,
            self.custom_data.keygen.ver5,
            self.should_timeout_keygen,
            ceremony_id,
            5
        );

        println!("Distributed all verify complaints");

        // Now we are either done or have to enter the blaming stage
        let nodes_entered_blaming = clients.values().all(|c| {
            get_stage_for_keygen_ceremony(&c, &ceremony_id).as_deref()
                == Some("BroadcastStage<BlameResponsesStage6>")
        });

        let (mut blame_responses6, mut ver_blame_responses7) = (None, None);

        if nodes_entered_blaming {
            println!("All clients entered blaming phase!");

            let responses6 = recv_keygen_broadcast!(p2p_rxs, KeygenData::BlameResponse6);
            blame_responses6 = Some(BlameResponses6Data {
                clients: clients.clone(),
                resp6: responses6.clone(),
            });

            println!("Collected all blame responses");

            distribute_data_keygen_custom!(
                clients,
                self.account_ids,
                responses6,
                &mut self.custom_data.keygen.secret_shares_blaming,
                &mut self.should_timeout_keygen,
                ceremony_id,
                6
            );

            println!("Distributed all blame responses");

            let ver7 = recv_keygen_broadcast!(p2p_rxs, KeygenData::VerifyBlameResponses7);
            ver_blame_responses7 = Some(VerBlameResponses7Data {
                clients: clients.clone(),
                ver7: ver7.clone(),
            });

            println!("Collected all blame responses verification");

            distribute_data_keygen_custom!(
                clients,
                self.account_ids,
                ver7,
                &mut self.custom_data.keygen.ver7,
                self.should_timeout_keygen,
                ceremony_id,
                7
            );

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
                let first_pubkey = pubkeys
                    .first()
                    .expect("Node 0 should have a public key")
                    .clone();
                for pubkey in pubkeys {
                    assert_eq!(first_pubkey.serialize(), pubkey.serialize());
                }

                let mut sec_keys = HashMap::new();

                let key_id = KeyId(first_pubkey.serialize().into());
                self.key_id = Some(key_id.clone());

                for (id, c) in clients.iter() {
                    let key = c.get_key(&key_id).expect("key must be present");
                    sec_keys.insert(id.clone(), key.clone());
                }

                Ok(KeyReadyData {
                    clients: clients.clone(),
                    pubkey: first_pubkey,
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
        self.sign_custom(&*SIGNER_IDS, None).await
    }

    // Use the generated key and the clients participating
    // in the ceremony and sign a message producing state
    // for each of the signing phases
    pub async fn sign_custom(
        &mut self,
        signer_ids: &[AccountId],
        key_id: Option<KeyId>,
    ) -> ValidSigningStates {
        let instant = std::time::Instant::now();

        let key_id = key_id.unwrap_or(self.key_id());

        let sign_info = SigningInfo::new(
            SIGN_CEREMONY_ID,
            key_id,
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

            // TODO: If the key does not exist, we won't progress to the next stage, so
            // we should only test this if we know that the key is present

            assert_ok!(c.ensure_at_signing_stage(1));
        }

        let comm1s = recv_siging_broadcast!(p2p_rxs, SigningData::CommStage1);

        let sign_phase1 = SigningPhase1Data {
            clients: clients.clone(),
            comm1s: comm1s.clone(),
        };

        // *** Broadcast Comm1 messages to advance to Stage2 ***
        distribute_data_signing_custom!(
            &mut clients,
            &comm1s,
            &mut self.custom_data.signing.comm1,
            &mut self.should_timeout_signing,
            1
        );

        // Force timeout in case we are still in stage 1
        // (e.g. not all messages arrived)
        clients.values_mut().for_each(|c| {
            c.ensure_stage_finalized_signing(SIGN_CEREMONY_ID, "BroadcastStage<AwaitCommitments1>")
        });

        clients
            .values()
            .for_each(|c| assert_ok!(c.ensure_at_signing_stage(2)));

        // *** Collect Ver2 messages ***
        let ver2s = recv_siging_broadcast!(p2p_rxs, SigningData::BroadcastVerificationStage2);

        let sign_phase2 = SigningPhase2Data {
            clients: clients.clone(),
            ver2s: ver2s.clone(),
        };

        // *** Distribute Ver2 messages ***

        distribute_data_signing_custom!(
            &mut clients,
            &ver2s,
            &mut self.custom_data.signing.ver2,
            &mut self.should_timeout_signing,
            2
        );

        // Force timeout in case we are still in stage 2
        // (e.g. not all messages arrived)
        clients.values_mut().for_each(|c| {
            c.ensure_stage_finalized_signing(
                SIGN_CEREMONY_ID,
                "BroadcastStage<VerifyCommitmentsBroadcast2>",
            )
        });

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
                sign_finished: SigningFinishedData {
                    outcome,
                    clients: clients.clone(),
                },
            };
        }

        clients
            .values()
            .for_each(|c| assert_ok!(c.ensure_at_signing_stage(3)));

        // *** Collect local sigs ***

        let local_sigs = recv_siging_broadcast!(p2p_rxs, SigningData::LocalSigStage3);

        let sign_phase3 = SigningPhase3Data {
            clients: clients.clone(),
            local_sigs: local_sigs.clone(),
        };

        // *** Distribute local sigs ***
        distribute_data_signing_custom!(
            &mut clients,
            &local_sigs,
            &mut self.custom_data.signing.sig3s,
            &mut self.should_timeout_signing,
            3
        );

        // Force timeout in case we are still in stage 3
        // (e.g. not all messages arrived)
        clients.values_mut().for_each(|c| {
            c.ensure_stage_finalized_signing(SIGN_CEREMONY_ID, "BroadcastStage<LocalSigStage3>")
        });

        // *** Collect Ver4 messages ***
        let ver4s = recv_siging_broadcast!(p2p_rxs, SigningData::VerifyLocalSigsStage4);

        let sign_phase4 = SigningPhase4Data {
            clients: clients.clone(),
            ver4s: ver4s.clone(),
        };

        // *** Distribute Ver4 messages ***

        distribute_data_signing_custom!(
            &mut clients,
            &ver4s,
            &mut self.custom_data.signing.ver4,
            &mut self.should_timeout_signing,
            4
        );

        // Force timeout in case we are still in stage 4
        // (e.g. not all messages arrived)
        clients.values_mut().for_each(|c| {
            c.ensure_stage_finalized_signing(
                SIGN_CEREMONY_ID,
                "BroadcastStage<VerifyLocalSigsBroadcastStage4>",
            )
        });

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
                sign_finished: SigningFinishedData {
                    outcome,
                    clients: clients.clone(),
                },
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
                "Incorrect reported nodes: {}, expected: {}",
                format_iterator(&reported_nodes_sorted),
                format_iterator(&expected_nodes_sorted)
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
        if let Some(MultisigOutcome::Signing(outcome)) = peek_with_timeout(&mut rx).await {
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
            rxs.len(),
            "Not all signers got an outcome: {:?}",
            outcomes[0].result
        );

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

async fn recv_keygen_secret3(
    p2p_rxs: &mut HashMap<AccountId, P2PMessageReceiver>,
) -> HashMap<AccountId, HashMap<AccountId, keygen::SecretShare3>> {
    stream::iter(p2p_rxs.iter_mut())
        .then(|(id, rx)| async move {
            if let OutgoingMultisigStageMessages::Private(messages) =
                expect_next_with_timeout(rx).await
            {
                (
                    id.clone(),
                    messages
                        .into_iter()
                        .map(move |(dest, message)| {
                            (
                                dest,
                                match message {
                                    MultisigMessage {
                                        data: MultisigData::Keygen(KeygenData::SecretShares3(sec3)),
                                        ..
                                    } => sec3,
                                    _ => panic!("Received message is not Secret3 (keygen)"),
                                },
                            )
                        })
                        .collect(),
                )
            } else {
                panic!("Expected p2p private messages");
            }
        })
        .collect::<HashMap<_, _>>()
        .await
}

pub fn sig_data_to_p2p(data: impl Into<SigningData>) -> MultisigMessage {
    MultisigMessage {
        ceremony_id: SIGN_CEREMONY_ID,
        data: MultisigData::Signing(data.into()),
    }
}

// Create a p2p MultisigMessage using the default ceremony id
pub fn keygen_data_to_p2p(data: impl Into<KeygenData>) -> MultisigMessage {
    keygen_data_to_p2p_with_ceremony_id(data, KEYGEN_CEREMONY_ID)
}

// Create a p2p MultisigMessage using the specified ceremony id
pub fn keygen_data_to_p2p_with_ceremony_id(
    data: impl Into<KeygenData>,
    ceremony_id: CeremonyId,
) -> MultisigMessage {
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
            STAGE_FINISHED => stage == None,
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

    /// Check is the default ceremony is at the specified keygen BroadcastStage (0-5).
    /// 0 = No Stage
    /// 1 = AwaitCommitments1 ... and so on
    pub fn ensure_at_keygen_stage(&self, stage_number: usize) -> Result<()> {
        self.ensure_ceremony_at_keygen_stage(stage_number, &KEYGEN_CEREMONY_ID)
    }

    /// Check is the ceremony is at the specified keygen BroadcastStage (0-5).
    pub fn ensure_ceremony_at_keygen_stage(
        &self,
        stage_number: usize,
        ceremony_id: &CeremonyId,
    ) -> Result<()> {
        let stage = get_stage_for_keygen_ceremony(self, ceremony_id);
        let is_at_stage = match stage_number {
            STAGE_FINISHED => stage == None,
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
            1 => keygen_data_to_p2p(keygen_states.comm_stage1.comm1s[sender_id].clone()),
            2 => keygen_data_to_p2p(keygen_states.ver_com_stage2.ver2s[sender_id].clone()),
            3 => {
                let sec3 = keygen_states.sec_stage3.as_ref().expect("No stage 3").sec3[sender_id]
                    .get(&self.get_my_account_id())
                    .unwrap();
                keygen_data_to_p2p(sec3.clone())
            }
            4 => keygen_data_to_p2p(
                keygen_states
                    .comp_stage4
                    .as_ref()
                    .expect("No stage 4")
                    .comp4s[sender_id]
                    .clone(),
            ),
            5 => keygen_data_to_p2p(
                keygen_states
                    .ver_comp_stage5
                    .as_ref()
                    .expect("No stage 5")
                    .ver5[sender_id]
                    .clone(),
            ),
            6 => keygen_data_to_p2p(
                keygen_states
                    .blame_responses6
                    .as_ref()
                    .expect("No blaming stage 6")
                    .resp6[sender_id]
                    .clone(),
            ),
            7 => keygen_data_to_p2p(
                keygen_states
                    .ver_blame_responses7
                    .as_ref()
                    .expect("No blaming stage 7")
                    .ver7[sender_id]
                    .clone(),
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
