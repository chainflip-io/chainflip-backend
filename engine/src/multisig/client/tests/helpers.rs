use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Display,
    pin::Pin,
    sync::Arc,
};

use anyhow::Result;
use cf_chains::eth::{AggKey, SchnorrVerificationComponents};
use cf_traits::AuthorityCount;
use futures::{stream, Future, StreamExt};
use itertools::{Either, Itertools};

use async_trait::async_trait;

use rand_legacy::{FromEntropy, RngCore, SeedableRng};

use pallet_cf_vaults::CeremonyId;
use slog::Logger;
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot,
};
use utilities::{assert_ok, success_threshold_from_share_count, threshold_from_share_count};

use crate::{
    common::{all_same, split_at},
    multisig::{
        client::{
            ceremony_manager::{
                prepare_keygen_request, prepare_signing_request, CeremonyOutcome, CeremonyTrait,
                KeygenCeremony, SigningCeremony,
            },
            common::{
                broadcast::BroadcastStage, CeremonyCommon, CeremonyFailureReason,
                CeremonyStageName, KeygenFailureReason,
            },
            keygen::{
                generate_key_data, get_key_data_for_test, HashComm1, HashContext, SecretShare5,
                VerifyHashCommitmentsBroadcast2,
            },
            signing,
            state_runner::CeremonyRunner,
            KeygenResultInfo, PartyIdxMapping, ThresholdParameters,
        },
        crypto::{ECPoint, Rng},
        KeyId, MessageHash,
    },
    multisig_p2p::OutgoingMultisigStageMessages,
};

use signing::frost::{self, LocalSig3, SigningCommitment, SigningData};

use keygen::{generate_shares_and_commitment, DKGUnverifiedCommitment};

use crate::{
    logging::{self, test_utils::TagCache},
    multisig::{
        client::{
            keygen::{self, KeygenData},
            MultisigMessage,
        },
        // This determines which crypto scheme will be used in tests
        // (we make arbitrary choice to use eth)
        crypto::eth::{EthSchnorrSignature, EthSigning, Point},
        tests::fixtures::MESSAGE_HASH,
    },
    testing::expect_recv_with_timeout,
};

use state_chain_runtime::AccountId;

use super::{
    ACCOUNT_IDS, DEFAULT_KEYGEN_CEREMONY_ID, DEFAULT_KEYGEN_SEED, DEFAULT_SIGNING_CEREMONY_ID,
    DEFAULT_SIGNING_SEED, INITIAL_LATEST_CEREMONY_ID,
};

pub type StageMessages<T> = HashMap<AccountId, HashMap<AccountId, T>>;
type SigningCeremonyEth = SigningCeremony<EthSigning>;
type KeygenCeremonyEth = KeygenCeremony<EthSigning>;

pub struct Node<C: CeremonyTrait> {
    own_account_id: AccountId,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    pub ceremony_runner: CeremonyRunner<C>,
    outgoing_p2p_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
    /// If any of the methods we called on the ceremony runner returned the outcome,
    /// it will be stored here
    outcome: Option<CeremonyOutcome<C>>,
    allowing_high_pubkey: bool,
    logger: slog::Logger,
}

fn new_node<C: CeremonyTrait>(account_id: AccountId, allowing_high_pubkey: bool) -> Node<C> {
    let logger = logging::test_utils::new_test_logger();
    let logger = logger.new(slog::o!("account_id" => account_id.to_string()));
    let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let ceremony_runner =
        CeremonyRunner::new_unauthorised_for_test(INITIAL_LATEST_CEREMONY_ID, &logger);

    Node {
        outgoing_p2p_message_sender,
        own_account_id: account_id,
        ceremony_runner,
        outgoing_p2p_message_receiver,
        allowing_high_pubkey,
        outcome: None,
        logger,
    }
}

// Exists so some of the tests can easily modify signing requests
pub struct SigningCeremonyDetails {
    pub rng: Rng,
    pub ceremony_id: CeremonyId,
    pub signers: Vec<AccountId>,
    pub message_hash: MessageHash,
    pub keygen_result_info: KeygenResultInfo<Point>,
}

pub struct KeygenCeremonyDetails {
    pub rng: Rng,
    pub ceremony_id: CeremonyId,
    pub signers: Vec<AccountId>,
}

impl<C: CeremonyTrait> Node<C> {
    fn on_ceremony_outcome(&mut self, outcome: CeremonyOutcome<C>) {
        match &outcome {
            Ok(_) => {
                slog::debug!(self.logger, "Node got successful outcome");
            }
            Err((_, failure_reason)) => {
                slog::debug!(self.logger, "Node got failure outcome: {}", failure_reason);
            }
        }

        assert!(
            self.outcome.replace(outcome).is_none(),
            "Should not receive more than one outcome"
        );
    }

    pub async fn force_stage_timeout(&mut self) {
        if let Some(outcome) = self.ceremony_runner.force_timeout().await {
            self.on_ceremony_outcome(outcome);
        }
    }
}

impl Node<SigningCeremonyEth> {
    pub async fn request_signing(&mut self, signing_ceremony_details: SigningCeremonyDetails) {
        let SigningCeremonyDetails {
            rng,
            ceremony_id,
            signers,
            message_hash,
            keygen_result_info,
        } = signing_ceremony_details;

        let request = prepare_signing_request::<EthSigning>(
            ceremony_id,
            &self.own_account_id,
            signers,
            keygen_result_info,
            message_hash,
            &self.outgoing_p2p_message_sender,
            rng,
            &self.logger,
        )
        .expect("invalid request");

        if let Some(outcome) = self
            .ceremony_runner
            .on_ceremony_request(
                request.init_stage,
                request.idx_mapping,
                result_sender,
                request.participants_count,
            )
            .await
        {
            self.on_ceremony_outcome(outcome);
        }
    }
}

impl Node<KeygenCeremonyEth> {
    pub async fn request_keygen(&mut self, keygen_ceremony_details: KeygenCeremonyDetails) {
        let KeygenCeremonyDetails {
            ceremony_id,
            rng,
            signers,
        } = keygen_ceremony_details;

        let request = prepare_keygen_request::<EthSigning>(
            ceremony_id,
            &self.own_account_id,
            signers,
            &self.outgoing_p2p_message_sender,
            rng,
            self.allowing_high_pubkey,
            &self.logger,
        )
        .expect("invalid request");

        if let Some(outcome) = self
            .ceremony_runner
            .on_ceremony_request(
                request.init_stage,
                request.idx_mapping,
                result_sender,
                request.participants_count,
            )
            .await
        {
            self.on_ceremony_outcome(outcome)
        }
    }
}

pub fn new_nodes<AccountIds: IntoIterator<Item = AccountId>, C: CeremonyTrait>(
    account_ids: AccountIds,
) -> HashMap<AccountId, Node<C>> {
    account_ids
        .into_iter()
        .map(|account_id| (account_id.clone(), new_node(account_id, true)))
        .collect()
}

pub fn new_nodes_without_allow_high_pubkey<
    AccountIds: IntoIterator<Item = AccountId>,
    C: CeremonyTrait,
>(
    account_ids: AccountIds,
) -> HashMap<AccountId, Node<C>> {
    account_ids
        .into_iter()
        .map(|account_id| (account_id.clone(), new_node(account_id, false)))
        .collect()
}

#[async_trait]
pub trait CeremonyRunnerStrategy {
    type CeremonyType: CeremonyTrait;

    type CheckedOutput: std::fmt::Debug;
    type InitialStageData: TryFrom<
            <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data,
            Error = <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data,
        > + Clone;

    fn post_successful_complete_check(
        &self,
        outputs: HashMap<AccountId, <Self::CeremonyType as CeremonyTrait>::Artefact>,
    ) -> Self::CheckedOutput;

    async fn request_ceremony(&mut self, node_id: &AccountId);
}

pub struct CeremonyTestRunner<CeremonyRunnerData, C: CeremonyTrait> {
    pub nodes: HashMap<AccountId, Node<C>>,
    pub ceremony_id: CeremonyId,
    pub ceremony_runner_data: CeremonyRunnerData,
    pub rng: Rng,
}

impl<CeremonyRunnerData, C: CeremonyTrait> CeremonyTestRunner<CeremonyRunnerData, C>
where
    Self: CeremonyRunnerStrategy<CeremonyType = C>,
{
    fn inner_new(
        nodes: HashMap<AccountId, Node<C>>,
        ceremony_id: CeremonyId,
        ceremony_runner_data: CeremonyRunnerData,
        rng: Rng,
    ) -> Self {
        Self {
            nodes,
            ceremony_id,
            ceremony_runner_data,
            rng,
        }
    }

    pub fn get_mut_node(&mut self, account_id: &AccountId) -> &mut Node<C> {
        self.nodes.get_mut(account_id).unwrap()
    }

    pub fn select_account_ids<const COUNT: usize>(&self) -> [AccountId; COUNT] {
        self.nodes
            .iter()
            .map(|(account_id, _)| account_id.clone())
            .sorted()
            .take(COUNT)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    pub async fn distribute_messages<
        StageData: Into<<<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data>,
    >(
        &mut self,
        stage_data: StageMessages<StageData>,
    ) {
        for (sender_id, messages) in stage_data {
            for (receiver_id, message) in messages {
                self.distribute_message(&sender_id, &receiver_id, message)
                    .await;
            }
        }
    }

    pub async fn distribute_message<
        StageData: Into<<<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data>,
    >(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        stage_data: StageData,
    ) {
        assert_ne!(receiver_id, sender_id);

        let node = self.nodes.get_mut(receiver_id).unwrap();

        if let Some(outcome) = node
            .ceremony_runner
            .process_or_delay_message(sender_id.clone(), stage_data.into())
            .await
        {
            node.on_ceremony_outcome(outcome);
        }
    }

    pub async fn distribute_messages_with_non_sender<
        StageData: Into<<<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data>,
    >(
        &mut self,
        mut stage_data: StageMessages<StageData>,
        non_sender: &AccountId,
    ) {
        stage_data.remove(non_sender).unwrap();
        self.distribute_messages(stage_data).await;
        for (_, node) in self
            .nodes
            .iter_mut()
            .filter(|(account_id, _)| *account_id != non_sender)
        {
            node.force_stage_timeout().await;
        }
    }

    pub async fn gather_outgoing_messages<
        NextStageData: TryFrom<
                <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data,
                Error = Error,
            > + Clone,
        Error: Display,
    >(
        &mut self,
    ) -> StageMessages<NextStageData> {
        let self_ceremony_id = self.ceremony_id;
        let message_to_next_stage_data = |message| {
            let MultisigMessage { ceremony_id, data } = message;

            assert_eq!(
                ceremony_id, self_ceremony_id,
                "Client output p2p message for ceremony_id {}, expected {}",
                ceremony_id, self_ceremony_id
            );

            let ceremony_data =
                <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data::try_from(
                    data,
                )
                .map_err(|err| {
                    format!(
                        "Expected outgoing ceremony data {}, got {:?}.",
                        std::any::type_name::<
                            <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data,
                        >(),
                        err
                    )
                })
                .unwrap();
            NextStageData::try_from(ceremony_data)
                .map_err(|err| {
                    format!(
                        "Expected outgoing ceremony data {}, got {}.",
                        std::any::type_name::<NextStageData>(),
                        err
                    )
                })
                .unwrap()
        };

        stream::iter(self.nodes.iter_mut())
            .then(|(account_id, node)| async move {
                (account_id.clone(), {
                    // TODO Consider member functions on OutgoingMultisigStageMessages for transforms
                    match expect_recv_with_timeout(&mut node.outgoing_p2p_message_receiver).await {
                        OutgoingMultisigStageMessages::Broadcast(receiver_ids, message) => {
                            let next_data =
                                message_to_next_stage_data(bincode::deserialize(&message).unwrap());
                            receiver_ids
                                .into_iter()
                                .map(move |receiver_id| (receiver_id, next_data.clone()))
                                .collect()
                        }
                        OutgoingMultisigStageMessages::Private(messages) => messages
                            .into_iter()
                            .map(|(receiver_id, message)| {
                                (
                                    receiver_id,
                                    message_to_next_stage_data(
                                        bincode::deserialize(&message).unwrap(),
                                    ),
                                )
                            })
                            .collect(),
                    }
                })
            })
            .collect()
            .await
    }

    pub async fn run_stage<
        NextStageData: TryFrom<
                <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data,
                Error = Error,
            > + Clone,
        StageData: Into<<<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data>,
        Error: Display,
    >(
        &mut self,
        stage_data: StageMessages<StageData>,
    ) -> StageMessages<NextStageData> {
        self.distribute_messages(stage_data).await;
        self.gather_outgoing_messages().await
    }

    pub async fn run_stage_with_non_sender<
        NextStageData: TryFrom<
                <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data,
                Error = Error,
            > + Clone,
        StageData: Into<<<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::Data>,
        Error: Display,
    >(
        &mut self,
        stage_data: StageMessages<StageData>,
        non_sender: &AccountId,
    ) -> StageMessages<NextStageData> {
        self.distribute_messages_with_non_sender(stage_data, non_sender)
            .await;
        self.gather_outgoing_messages().await
    }

    async fn check_node_outcomes(
        &mut self,
    ) -> Option<
        Result<
            <Self as CeremonyRunnerStrategy>::CheckedOutput,
            (
                BTreeSet<AccountId>,
                CeremonyFailureReason<<<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::FailureReason>,
            ),
        >,
    >{
        let results: HashMap<_, _> = self
            .nodes
            .iter_mut()
            .filter_map(|(account_id, node)| {
                if let Some(outcome) = node.outcome.take() {
                    Some((account_id.clone(), outcome))
                } else {
                    None
                }
            })
            .collect();

        let (ok_results, (all_reported_parties, failure_reasons)): (
            HashMap<_, _>,
            (BTreeSet<_>, BTreeSet<_>),
        ) = results
            .into_iter()
            .partition_map(|(account_id, result)| match result {
                Ok(output) => Either::Left((account_id, output)),
                Err((reported_parties, reason)) => Either::Right((reported_parties, reason)),
            });

        if !ok_results.is_empty() && all_reported_parties.is_empty() {
            Some(Ok(self.post_successful_complete_check(ok_results)))
        } else if ok_results.is_empty() && !all_reported_parties.is_empty() {
            assert_eq!(
                all_reported_parties.len(),
                1,
                "Reported parties weren't the same for all nodes"
            );
            assert_eq!(
                failure_reasons.len(),
                1,
                "The ceremony failure reason was not the same for all nodes: {:?}",
                failure_reasons
            );
            Some(Err((
                all_reported_parties.into_iter().next().unwrap(),
                failure_reasons.into_iter().next().unwrap(),
            )))
        } else {
            panic!("Ceremony results weren't consistently Ok() or Err() for all nodes");
        }
    }

    pub async fn complete(&mut self) -> <Self as CeremonyRunnerStrategy>::CheckedOutput {
        assert_ok!(self
            .check_node_outcomes()
            .await
            .expect("Failed to get all ceremony outcomes"))
    }

    async fn try_complete_with_error(
        &mut self,
        bad_account_ids: &[AccountId],
        expected_failure_reason: CeremonyFailureReason<
            <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::FailureReason,
        >,
    ) -> Option<()> {
        let (reported, reason) = self.check_node_outcomes().await?.unwrap_err();
        assert_eq!(
            BTreeSet::from_iter(bad_account_ids.iter()),
            reported.iter().collect()
        );
        assert_eq!(expected_failure_reason, reason);
        Some(())
    }

    /// Gathers the ceremony outcomes from all nodes,
    /// making sure they are identical and match the expected failure reason.
    pub async fn complete_with_error(
        &mut self,
        bad_account_ids: &[AccountId],
        expected_failure_reason: CeremonyFailureReason<
            <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::FailureReason,
        >,
    ) {
        self.try_complete_with_error(bad_account_ids, expected_failure_reason)
            .await
            .expect("Failed to get all ceremony outcomes");
    }

    async fn request_without_gather(&mut self) {
        for node_id in self.nodes.keys().sorted().cloned().collect::<Vec<_>>() {
            self.request_ceremony(&node_id).await;
        }
    }

    pub async fn request(
        &mut self,
    ) -> HashMap<
        AccountId,
        HashMap<
            AccountId,
            <CeremonyTestRunner<CeremonyRunnerData, C> as CeremonyRunnerStrategy>::InitialStageData,
        >,
    > {
        self.request_without_gather().await;

        self.gather_outgoing_messages().await
    }
}

macro_rules! run_stages {
    ($ceremony:ident, $messages:expr, $first_stage:ty, $($stage:ty),*) => {{
        let messages = $ceremony
            .run_stage::<$first_stage, _, _>($messages)
            .await;
        $(
            let messages = $ceremony
                .run_stage::<$stage, _, _>(messages)
                .await;
        )*
        messages
    }}
}
pub(crate) use run_stages;

pub type KeygenCeremonyRunner = CeremonyTestRunner<(), KeygenCeremony<EthSigning>>;

#[async_trait]
impl CeremonyRunnerStrategy for KeygenCeremonyRunner {
    type CeremonyType = KeygenCeremony<EthSigning>;
    type CheckedOutput = (
        KeyId,
        HashMap<AccountId, <Self::CeremonyType as CeremonyTrait>::Artefact>,
    );
    type InitialStageData = keygen::HashComm1;

    fn post_successful_complete_check(
        &self,
        outputs: HashMap<AccountId, <Self::CeremonyType as CeremonyTrait>::Artefact>,
    ) -> Self::CheckedOutput {
        let (_, public_key) = all_same(outputs.iter().map(|(_, keygen_result_info)| {
            (
                keygen_result_info.params,
                keygen_result_info.key.get_public_key().get_element(),
            )
        }))
        .expect("Generated keys don't match");

        (KeyId(public_key.serialize().into()), outputs)
    }

    async fn request_ceremony(&mut self, node_id: &AccountId) {
        let keygen_ceremony_details = self.keygen_ceremony_details();

        self.nodes
            .get_mut(node_id)
            .unwrap()
            .request_keygen(keygen_ceremony_details)
            .await;
    }
}
impl KeygenCeremonyRunner {
    pub fn new(
        nodes: HashMap<AccountId, Node<KeygenCeremonyEth>>,
        ceremony_id: CeremonyId,
        rng: Rng,
    ) -> Self {
        Self::inner_new(nodes, ceremony_id, (), rng)
    }

    pub fn keygen_ceremony_details(&mut self) -> KeygenCeremonyDetails {
        use rand_legacy::Rng as _;

        KeygenCeremonyDetails {
            ceremony_id: self.ceremony_id,
            rng: Rng::from_seed(self.rng.gen()),
            signers: self.nodes.keys().cloned().collect(),
        }
    }

    /// Create a keygen ceremony with all ACCOUNT_IDS and default parameters
    pub fn new_with_default() -> Self {
        KeygenCeremonyRunner::new(
            new_nodes(ACCOUNT_IDS.clone()),
            DEFAULT_KEYGEN_CEREMONY_ID,
            Rng::from_seed(DEFAULT_KEYGEN_SEED),
        )
    }
}

pub struct SigningCeremonyRunnerData {
    pub key_id: KeyId,
    pub key_data: HashMap<AccountId, KeygenResultInfo<Point>>,
    pub message_hash: MessageHash,
}
pub type SigningCeremonyRunner =
    CeremonyTestRunner<SigningCeremonyRunnerData, SigningCeremony<EthSigning>>;

#[async_trait]
impl CeremonyRunnerStrategy for SigningCeremonyRunner {
    type CeremonyType = SigningCeremonyEth;
    type CheckedOutput = EthSchnorrSignature;
    type InitialStageData = frost::Comm1<Point>;

    fn post_successful_complete_check(
        &self,
        outputs: HashMap<AccountId, <Self::CeremonyType as CeremonyTrait>::Artefact>,
    ) -> Self::CheckedOutput {
        let signature = all_same(outputs.into_iter().map(|(_, signature)| signature))
            .expect("Signatures don't match");

        verify_sig_with_aggkey(&signature, &self.ceremony_runner_data.key_id)
            .expect("Should be valid signature");

        signature
    }

    async fn request_ceremony(&mut self, node_id: &AccountId) {
        let signing_ceremony_details = self.signing_ceremony_details(node_id);

        self.nodes
            .get_mut(node_id)
            .unwrap()
            .request_signing(signing_ceremony_details)
            .await;
    }
}
impl SigningCeremonyRunner {
    pub fn new_with_all_signers(
        nodes: HashMap<AccountId, Node<SigningCeremonyEth>>,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        key_data: HashMap<AccountId, KeygenResultInfo<Point>>,
        message_hash: MessageHash,
        rng: Rng,
    ) -> Self {
        Self::inner_new(
            nodes,
            ceremony_id,
            SigningCeremonyRunnerData {
                key_id,
                key_data,
                message_hash,
            },
            rng,
        )
    }

    pub fn new_with_threshold_subset_of_signers(
        nodes: HashMap<AccountId, Node<SigningCeremonyEth>>,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        key_data: HashMap<AccountId, KeygenResultInfo<Point>>,
        message_hash: MessageHash,
        rng: Rng,
    ) -> (Self, HashMap<AccountId, Node<SigningCeremonyEth>>) {
        let nodes_len = nodes.len();
        let (signers, non_signers) = split_at(
            nodes
                .into_iter()
                .sorted_by_key(|(account_id, _)| account_id.clone()),
            success_threshold_from_share_count(nodes_len as AuthorityCount) as usize,
        );

        (
            Self::new_with_all_signers(signers, ceremony_id, key_id, key_data, message_hash, rng),
            non_signers,
        )
    }

    pub fn signing_ceremony_details(&mut self, account_id: &AccountId) -> SigningCeremonyDetails {
        use rand_legacy::Rng as _;

        SigningCeremonyDetails {
            ceremony_id: self.ceremony_id,
            rng: Rng::from_seed(self.rng.gen()),
            signers: self.nodes.keys().cloned().collect(),
            message_hash: self.ceremony_runner_data.message_hash.clone(),
            keygen_result_info: self.ceremony_runner_data.key_data[account_id].clone(),
        }
    }
}

pub async fn new_signing_ceremony_with_keygen() -> (
    SigningCeremonyRunner,
    HashMap<AccountId, Node<SigningCeremonyEth>>,
) {
    let (key_id, key_data) =
        generate_key_data(&ACCOUNT_IDS, &mut Rng::from_seed(DEFAULT_KEYGEN_SEED), true)
            .expect("Should generate key for test");

    SigningCeremonyRunner::new_with_threshold_subset_of_signers(
        new_nodes(ACCOUNT_IDS.clone()),
        DEFAULT_SIGNING_CEREMONY_ID,
        key_id,
        key_data,
        MESSAGE_HASH.clone(),
        Rng::from_seed(DEFAULT_SIGNING_SEED),
    )
}

/// Filters out messages that aren't for receiver_id and splits them into those from sender_id and the others
pub fn split_messages_for<StageData>(
    messages: StageMessages<StageData>,
    receiver_id: &AccountId,
    sender_id: &AccountId,
) -> (StageMessages<StageData>, StageMessages<StageData>) {
    messages
        .into_iter()
        .map(|(sender_id, messages)| {
            (
                sender_id,
                messages
                    .into_iter()
                    .filter(|(other_receiver_id, _)| other_receiver_id == receiver_id)
                    .collect(),
            )
        })
        .partition(|(other_sender_id, _)| other_sender_id == sender_id)
}

// TODO Collect Messages Here into a Vector
#[derive(PartialEq, Eq)]
pub enum CeremonyVisitor {
    Complete,
    StageNumber(usize),
}
impl CeremonyVisitor {
    pub fn yield_ceremony<OtherStageData: Any>(
        &mut self,
        _messages: &StageMessages<OtherStageData>,
    ) -> Option<()> {
        match self {
            CeremonyVisitor::Complete => Some(()),
            CeremonyVisitor::StageNumber(stage) => {
                if 0 == *stage {
                    None
                } else {
                    *stage = stage.checked_sub(1).unwrap();
                    Some(())
                }
            }
        }
    }
}

type BoxFuture<'a, T> = Pin<Box<dyn 'a + Future<Output = T>>>;

pub async fn for_each_stage<
    'a,
    Stages: IntoIterator<Item = usize>,
    CeremonyType: 'a,
    CeremonyOutput: Clone,
    F: Future<Output = ()>,
>(
    stages: Stages,
    ceremony_runner_fn: impl Fn() -> BoxFuture<'a, CeremonyType>,
    ceremony_runner_coroutine: impl for<'s> Fn(
        &'s mut CeremonyVisitor,
        &'s mut CeremonyType,
    ) -> BoxFuture<'s, Option<CeremonyOutput>>,
    stage_logic: impl Fn(usize, CeremonyType, CeremonyOutput) -> F,
) {
    let mut visitor = CeremonyVisitor::Complete;
    let mut ceremony = ceremony_runner_fn().await;
    let output = ceremony_runner_coroutine(&mut visitor, &mut ceremony)
        .await
        .unwrap();

    for stage in stages {
        let mut visitor = CeremonyVisitor::StageNumber(
            stage
                .checked_sub(1)
                .expect("Stages should be indexed from 1"),
        );
        let mut ceremony = ceremony_runner_fn().await;
        ceremony_runner_coroutine(&mut visitor, &mut ceremony).await;

        stage_logic(stage, ceremony, output.clone()).await;
    }
}

fn into_generic_stage_data<CeremonyData, StageData: Into<CeremonyData>>(
    messages: StageMessages<StageData>,
) -> StageMessages<CeremonyData> {
    messages
        .into_iter()
        .map(|(account_id, messages)| {
            (
                account_id,
                messages
                    .into_iter()
                    .map(|(account_id, message)| (account_id, message.into()))
                    .collect(),
            )
        })
        .collect()
}

#[derive(Clone)]
pub struct StandardSigningMessages {
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, frost::Comm1<Point>>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, frost::VerifyComm2<Point>>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, frost::LocalSig3<Point>>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, frost::VerifyLocalSig4<Point>>>,
}

#[allow(clippy::type_complexity)]
pub fn standard_signing_coroutine<'a>(
    visitor: &'a mut CeremonyVisitor,
    ceremony: &'a mut SigningCeremonyRunner,
) -> BoxFuture<
    'a,
    Option<(
        EthSchnorrSignature,
        Vec<StageMessages<SigningData<Point>>>,
        StandardSigningMessages,
    )>,
> {
    Box::pin(async move {
        let stage_1_messages = ceremony.request().await;
        visitor.yield_ceremony(&stage_1_messages)?;
        let stage_2_messages = ceremony.run_stage(stage_1_messages.clone()).await;
        visitor.yield_ceremony(&stage_2_messages)?;
        let stage_3_messages = ceremony.run_stage(stage_2_messages.clone()).await;
        visitor.yield_ceremony(&stage_3_messages)?;
        let stage_4_messages = ceremony.run_stage(stage_3_messages.clone()).await;
        visitor.yield_ceremony(&stage_4_messages)?;
        ceremony.distribute_messages(stage_4_messages.clone()).await;

        Some((
            ceremony.complete().await,
            vec![
                into_generic_stage_data(stage_1_messages.clone()),
                into_generic_stage_data(stage_2_messages.clone()),
                into_generic_stage_data(stage_3_messages.clone()),
                into_generic_stage_data(stage_4_messages.clone()),
            ],
            StandardSigningMessages {
                stage_1_messages,
                stage_2_messages,
                stage_3_messages,
                stage_4_messages,
            },
        ))
    })
}

pub async fn standard_signing(
    signing_ceremony: &mut SigningCeremonyRunner,
) -> (EthSchnorrSignature, StandardSigningMessages) {
    let mut visitor = CeremonyVisitor::Complete;
    let (signature, _, messages) = standard_signing_coroutine(&mut visitor, signing_ceremony)
        .await
        .unwrap();
    (signature, messages)
}

pub struct StandardKeygenMessages {
    pub stage_1a_messages: HashMap<AccountId, HashMap<AccountId, keygen::HashComm1>>,
    pub stage_2a_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyHashComm2>>,
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, keygen::CoeffComm3<Point>>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyCoeffComm4<Point>>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, keygen::SecretShare5<Point>>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, keygen::Complaints6>>,
    pub stage_5_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComplaints7>>,
}

pub async fn standard_keygen(
    mut keygen_ceremony: KeygenCeremonyRunner,
) -> (
    KeyId,
    HashMap<AccountId, KeygenResultInfo<Point>>,
    StandardKeygenMessages,
    HashMap<AccountId, Node<KeygenCeremonyEth>>,
) {
    let stage_1a_messages = keygen_ceremony.request().await;
    let stage_2a_messages = keygen_ceremony.run_stage(stage_1a_messages.clone()).await;
    let stage_1_messages = keygen_ceremony.run_stage(stage_2a_messages.clone()).await;
    let stage_2_messages = keygen_ceremony.run_stage(stage_1_messages.clone()).await;
    let stage_3_messages = keygen_ceremony.run_stage(stage_2_messages.clone()).await;
    let stage_4_messages = keygen_ceremony.run_stage(stage_3_messages.clone()).await;
    let stage_5_messages = keygen_ceremony.run_stage(stage_4_messages.clone()).await;
    keygen_ceremony
        .distribute_messages(stage_5_messages.clone())
        .await;
    let (key_id, key_data) = keygen_ceremony.complete().await;

    (
        key_id,
        key_data,
        StandardKeygenMessages {
            stage_1a_messages,
            stage_2a_messages,
            stage_1_messages,
            stage_2_messages,
            stage_3_messages,
            stage_4_messages,
            stage_5_messages,
        },
        keygen_ceremony.nodes,
    )
}

pub async fn run_keygen(
    nodes: HashMap<AccountId, Node<KeygenCeremonyEth>>,
    ceremony_id: CeremonyId,
) -> (
    KeyId,
    HashMap<AccountId, KeygenResultInfo<Point>>,
    StandardKeygenMessages,
    HashMap<AccountId, Node<KeygenCeremonyEth>>,
) {
    let keygen_ceremony =
        KeygenCeremonyRunner::new(nodes, ceremony_id, Rng::from_seed(DEFAULT_KEYGEN_SEED));
    standard_keygen(keygen_ceremony).await
}

pub async fn run_keygen_with_err_on_high_pubkey<AccountIds: IntoIterator<Item = AccountId>>(
    account_ids: AccountIds,
) -> Result<
    (
        KeyId,
        HashMap<AccountId, KeygenResultInfo<Point>>,
        HashMap<AccountId, Node<KeygenCeremonyEth>>,
    ),
    (),
> {
    let mut keygen_ceremony = KeygenCeremonyRunner::new(
        new_nodes_without_allow_high_pubkey(account_ids),
        DEFAULT_KEYGEN_CEREMONY_ID,
        Rng::from_entropy(),
    );

    let stage_1_messages = keygen_ceremony.request().await;
    let stage_4_messages = run_stages!(
        keygen_ceremony,
        stage_1_messages,
        keygen::VerifyHashComm2,
        keygen::CoeffComm3<Point>,
        keygen::VerifyCoeffComm4<Point>
    );
    keygen_ceremony.distribute_messages(stage_4_messages).await;
    match keygen_ceremony
        .try_complete_with_error(
            &[],
            CeremonyFailureReason::Other(KeygenFailureReason::KeyNotCompatible),
        )
        .await
    {
        Some(_) => Err(()),
        None => {
            let stage_5_messages = keygen_ceremony
                .gather_outgoing_messages::<keygen::SecretShare5<Point>, _>()
                .await;
            let stage_7_messages = run_stages!(
                keygen_ceremony,
                stage_5_messages,
                keygen::Complaints6,
                keygen::VerifyComplaints7
            );
            keygen_ceremony.distribute_messages(stage_7_messages).await;

            let (key_id, key_data) = keygen_ceremony.complete().await;

            Ok((key_id, key_data, keygen_ceremony.nodes))
        }
    }
}

#[derive(Clone)]
pub struct AllKeygenMessages {
    pub stage_1a_messages: HashMap<AccountId, HashMap<AccountId, keygen::HashComm1>>,
    pub stage_2a_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyHashComm2>>,
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, keygen::CoeffComm3<Point>>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyCoeffComm4<Point>>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, keygen::SecretShare5<Point>>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, keygen::Complaints6>>,
    pub stage_5_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComplaints7>>,
    pub stage_6_messages: HashMap<AccountId, HashMap<AccountId, keygen::BlameResponse8<Point>>>,
    pub stage_7_messages:
        HashMap<AccountId, HashMap<AccountId, keygen::VerifyBlameResponses9<Point>>>,
}

#[allow(clippy::type_complexity)]
pub fn all_stages_with_single_invalid_share_keygen_coroutine<'a>(
    visitor: &'a mut CeremonyVisitor,
    ceremony: &'a mut KeygenCeremonyRunner,
) -> BoxFuture<
    'a,
    Option<(
        HashMap<AccountId, KeygenResultInfo<Point>>,
        Vec<StageMessages<KeygenData<Point>>>,
        AllKeygenMessages,
    )>,
> {
    Box::pin(async move {
        let stage_1a_messages = ceremony.request().await;
        visitor.yield_ceremony(&stage_1a_messages)?;
        let stage_2a_messages = ceremony.run_stage(stage_1a_messages.clone()).await;
        visitor.yield_ceremony(&stage_2a_messages)?;
        let stage_1_messages = ceremony.run_stage(stage_2a_messages.clone()).await;
        visitor.yield_ceremony(&stage_1_messages)?;
        let stage_2_messages = ceremony.run_stage(stage_1_messages.clone()).await;
        visitor.yield_ceremony(&stage_2_messages)?;
        let mut stage_3_messages = ceremony.run_stage(stage_2_messages.clone()).await;
        visitor.yield_ceremony(&stage_3_messages)?;
        let [node_id_0, node_id_1] = ceremony.select_account_ids();
        *stage_3_messages
            .get_mut(&node_id_0)
            .unwrap()
            .get_mut(&node_id_1)
            .unwrap() = SecretShare5::create_random(&mut ceremony.rng);

        let stage_4_messages = ceremony.run_stage(stage_3_messages.clone()).await;
        visitor.yield_ceremony(&stage_4_messages)?;
        let stage_5_messages = ceremony.run_stage(stage_4_messages.clone()).await;
        visitor.yield_ceremony(&stage_5_messages)?;
        let stage_6_messages = ceremony.run_stage(stage_5_messages.clone()).await;
        visitor.yield_ceremony(&stage_6_messages)?;
        let stage_7_messages = ceremony.run_stage(stage_6_messages.clone()).await;
        visitor.yield_ceremony(&stage_7_messages)?;
        ceremony.distribute_messages(stage_7_messages.clone()).await;
        let (_key_id, key_data) = ceremony.complete().await;

        Some((
            key_data,
            vec![
                into_generic_stage_data(stage_1a_messages.clone()),
                into_generic_stage_data(stage_2a_messages.clone()),
                into_generic_stage_data(stage_1_messages.clone()),
                into_generic_stage_data(stage_2_messages.clone()),
                into_generic_stage_data(stage_3_messages.clone()),
                into_generic_stage_data(stage_4_messages.clone()),
                into_generic_stage_data(stage_5_messages.clone()),
                into_generic_stage_data(stage_6_messages.clone()),
                into_generic_stage_data(stage_7_messages.clone()),
            ],
            AllKeygenMessages {
                stage_1a_messages,
                stage_2a_messages,
                stage_1_messages,
                stage_2_messages,
                stage_3_messages,
                stage_4_messages,
                stage_5_messages,
                stage_6_messages,
                stage_7_messages,
            },
        ))
    })
}

/// Generate an invalid local sig for stage3
pub fn gen_invalid_local_sig<P: ECPoint>(rng: &mut Rng) -> LocalSig3<P> {
    use crate::multisig::crypto::ECScalar;

    frost::LocalSig3 {
        response: P::Scalar::random(rng),
    }
}

pub fn get_invalid_hash_comm(rng: &mut Rng) -> keygen::HashComm1 {
    use sp_core::H256;

    let mut buffer: [u8; 32] = [0; 32];
    rng.fill_bytes(&mut buffer);

    HashComm1(H256::from(buffer))
}

// Make these member functions of the CeremonyRunner
pub fn gen_invalid_keygen_comm1<P: ECPoint>(
    rng: &mut Rng,
    share_count: AuthorityCount,
) -> DKGUnverifiedCommitment<P> {
    let (_, fake_comm1) = generate_shares_and_commitment(
        rng,
        // The commitment is only invalid because of the invalid context
        &HashContext([0; 32]),
        0,
        ThresholdParameters {
            share_count,
            threshold: threshold_from_share_count(share_count as u32) as AuthorityCount,
        },
    );
    fake_comm1
}

pub fn gen_invalid_signing_comm1(rng: &mut Rng) -> SigningCommitment<Point> {
    SigningCommitment {
        d: Point::random(rng),
        e: Point::random(rng),
    }
}

/// Using the given key_id, verify the signature is correct
pub fn verify_sig_with_aggkey(sig: &EthSchnorrSignature, key_id: &KeyId) -> Result<()> {
    // Get the aggkey
    let pk_ser: &[u8; 33] = key_id.0[..].try_into().unwrap();
    let agg_key = AggKey::from_pubkey_compressed(*pk_ser);

    // Verify the signature with the aggkey
    agg_key
        .verify(
            &MESSAGE_HASH.0,
            &SchnorrVerificationComponents::from(sig.clone()),
        )
        .map_err(|e| anyhow::Error::msg(format!("Failed to verify signature: {:?}", e)))?;

    Ok(())
}

pub fn gen_invalid_keygen_stage_2_state<P: ECPoint>(
    ceremony_id: CeremonyId,
    account_ids: &[AccountId],
    mut rng: Rng,
    logger: Logger,
) -> CeremonyRunner<KeygenCeremony<EthSigning>> {
    let validator_mapping = Arc::new(PartyIdxMapping::from_unsorted_signers(account_ids));
    let common = CeremonyCommon {
        ceremony_id,
        own_idx: 0,
        all_idxs: BTreeSet::from_iter((0..account_ids.len()).into_iter().map(|id| id as u32)),
        outgoing_p2p_message_sender: tokio::sync::mpsc::unbounded_channel().0,
        validator_mapping: validator_mapping.clone(),
        rng: rng.clone(),
        logger: logger.clone(),
    };

    let commitment = gen_invalid_keygen_comm1(&mut rng, account_ids.len() as u32);
    let processor = VerifyHashCommitmentsBroadcast2::new(
        common.clone(),
        true,
        commitment,
        account_ids.iter().map(|_| (0, None)).collect(),
        keygen::OutgoingShares(BTreeMap::new()),
        HashContext([0; 32]),
    );

    let stage = Box::new(BroadcastStage::new(processor, common));

    CeremonyRunner::new_authorised(
        ceremony_id,
        stage,
        validator_mapping,
        oneshot::channel().0,
        account_ids.len() as u32,
        logger,
    )
}

pub fn get_keygen_stage_name_from_number(stage_number: usize) -> Option<CeremonyStageName> {
    match stage_number {
        1 => Some(CeremonyStageName::HashCommitments1),
        2 => Some(CeremonyStageName::VerifyHashCommitmentsBroadcast2),
        3 => Some(CeremonyStageName::CoefficientCommitments3),
        4 => Some(CeremonyStageName::VerifyCommitmentsBroadcast4),
        5 => Some(CeremonyStageName::SecretSharesStage5),
        6 => Some(CeremonyStageName::ComplaintsStage6),
        7 => Some(CeremonyStageName::VerifyComplaintsBroadcastStage7),
        8 => Some(CeremonyStageName::BlameResponsesStage8),
        9 => Some(CeremonyStageName::VerifyBlameResponsesBroadcastStage9),
        _ => None,
    }
}

pub fn get_signing_stage_name_from_number(stage_number: usize) -> Option<CeremonyStageName> {
    match stage_number {
        1 => Some(CeremonyStageName::AwaitCommitments1),
        2 => Some(CeremonyStageName::VerifyCommitmentsBroadcast2),
        3 => Some(CeremonyStageName::LocalSigStage3),
        4 => Some(CeremonyStageName::VerifyLocalSigsBroadcastStage4),
        _ => None,
    }
}
