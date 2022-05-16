use std::{
    any::Any,
    collections::{BTreeSet, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Display,
    iter::FromIterator,
    pin::Pin,
    time::Duration,
};

use anyhow::Result;
use cf_chains::eth::{AggKey, SchnorrVerificationComponents};
use cf_traits::AuthorityCount;
use futures::{stream, Future, StreamExt};
use itertools::{Either, Itertools};

use rand_legacy::{FromEntropy, RngCore, SeedableRng};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::{mpsc::UnboundedReceiver, oneshot};
use utilities::success_threshold_from_share_count;

use crate::{
    common::{all_same, split_at},
    logging::{KEYGEN_CEREMONY_FAILED, KEYGEN_REJECTED_INCOMPATIBLE, SIGNING_CEREMONY_FAILED},
    multisig::{
        client::{
            ceremony_manager::{CeremonyManager, CeremonyResultReceiver},
            keygen::{HashComm1, HashContext, SecretShare3},
            signing, KeygenResultInfo, MultisigData, ThresholdParameters,
        },
        crypto::{ECPoint, Rng},
        KeyId, MessageHash,
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
            keygen::{self, KeygenData},
            MultisigMessage,
        },
        // This determines which crypto scheme will be used in tests
        // (we make arbitrary choice to use eth)
        crypto::eth::{EthSchnorrSignature, EthSigning, Point},
    },
};

use state_chain_runtime::AccountId;

use super::{
    ACCOUNT_IDS, DEFAULT_KEYGEN_CEREMONY_ID, DEFAULT_KEYGEN_SEED, DEFAULT_SIGNING_CEREMONY_ID,
    DEFAULT_SIGNING_SEED, STAGE_FINISHED_OR_NOT_STARTED,
};

use crate::multisig::tests::fixtures::MESSAGE_HASH;

pub type StageMessages<T> = HashMap<AccountId, HashMap<AccountId, T>>;

pub async fn recv_with_timeout<I>(receiver: &mut UnboundedReceiver<I>) -> Option<I> {
    tokio::time::timeout(CHANNEL_TIMEOUT, receiver.recv())
        .await
        .ok()?
}

pub async fn expect_recv_with_timeout<Item: std::fmt::Debug>(
    receiver: &mut UnboundedReceiver<Item>,
) -> Item {
    match recv_with_timeout(receiver).await {
        Some(i) => i,
        None => panic!(
            "Timeout waiting for message, expected {}",
            std::any::type_name::<Item>()
        ),
    }
}

pub struct Node {
    pub ceremony_manager: CeremonyManager<EthSigning>,
    pub outgoing_p2p_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
    pub tag_cache: TagCache,
}

pub fn new_node(account_id: AccountId, allow_high_pubkey: bool) -> Node {
    let (logger, tag_cache) = logging::test_utils::new_test_logger_with_tag_cache();
    let logger = logger.new(slog::o!("account_id" => format!("{}",account_id)));
    let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let mut ceremony_manager =
        CeremonyManager::new(account_id, outgoing_p2p_message_sender, &logger);
    if allow_high_pubkey {
        ceremony_manager.allow_high_pubkey();
    }

    Node {
        ceremony_manager,
        outgoing_p2p_message_receiver,
        tag_cache,
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

impl Node {
    pub fn request_signing(
        &mut self,
        signing_ceremony_details: SigningCeremonyDetails,
    ) -> CeremonyResultReceiver<EthSchnorrSignature> {
        let (result_sender, result_receiver) = oneshot::channel();
        self.ceremony_manager.on_request_to_sign(
            signing_ceremony_details.ceremony_id,
            signing_ceremony_details.signers,
            signing_ceremony_details.message_hash,
            signing_ceremony_details.keygen_result_info,
            signing_ceremony_details.rng,
            result_sender,
        );
        result_receiver
    }

    pub fn request_keygen(
        &mut self,
        keygen_ceremony_details: KeygenCeremonyDetails,
    ) -> CeremonyResultReceiver<KeygenResultInfo<Point>> {
        let (result_sender, result_receiver) = oneshot::channel();
        self.ceremony_manager.on_keygen_request(
            keygen_ceremony_details.ceremony_id,
            keygen_ceremony_details.signers,
            keygen_ceremony_details.rng,
            result_sender,
        );
        result_receiver
    }
}

pub fn new_nodes<AccountIds: IntoIterator<Item = AccountId>>(
    account_ids: AccountIds,
) -> HashMap<AccountId, Node> {
    account_ids
        .into_iter()
        .map(|account_id| (account_id.clone(), new_node(account_id, true)))
        .collect()
}

pub fn new_nodes_allow_high_pubkey<AccountIds: IntoIterator<Item = AccountId>>(
    account_ids: AccountIds,
) -> HashMap<AccountId, Node> {
    account_ids
        .into_iter()
        .map(|account_id| (account_id.clone(), new_node(account_id, false)))
        .collect()
}

pub trait CeremonyRunnerStrategy {
    type CeremonyData: Into<MultisigData<Point>>
        + TryFrom<MultisigData<Point>, Error = MultisigData<Point>>
        + Clone
        + Display;
    type Output: std::fmt::Debug;
    type CheckedOutput: std::fmt::Debug;
    type InitialStageData: TryFrom<
            <Self as CeremonyRunnerStrategy>::CeremonyData,
            Error = <Self as CeremonyRunnerStrategy>::CeremonyData,
        > + Clone;
    const CEREMONY_FAILED_TAG: &'static str;

    fn post_successful_complete_check(
        &self,
        outputs: HashMap<AccountId, Self::Output>,
    ) -> Self::CheckedOutput;

    fn request_ceremony(
        &mut self,
        node_id: &AccountId,
    ) -> CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output>;

    fn inner_distribute_message(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        stage_data: Self::CeremonyData,
    );
}

pub struct CeremonyRunner<CeremonyRunnerData> {
    pub nodes: HashMap<AccountId, Node>,
    pub ceremony_id: CeremonyId,
    pub ceremony_runner_data: CeremonyRunnerData,
    pub rng: Rng,
}

impl<CeremonyRunnerData> CeremonyRunner<CeremonyRunnerData>
where
    Self: CeremonyRunnerStrategy,
{
    pub fn inner_new(
        nodes: HashMap<AccountId, Node>,
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

    pub fn get_mut_node(&mut self, account_id: &AccountId) -> &mut Node {
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

    pub fn distribute_messages<StageData: Into<<Self as CeremonyRunnerStrategy>::CeremonyData>>(
        &mut self,
        stage_data: StageMessages<StageData>,
    ) {
        for (sender_id, messages) in stage_data {
            for (receiver_id, message) in messages {
                self.distribute_message(&sender_id, &receiver_id, message);
            }
        }
    }

    pub fn distribute_message<StageData: Into<<Self as CeremonyRunnerStrategy>::CeremonyData>>(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        stage_data: StageData,
    ) {
        assert_ne!(receiver_id, sender_id);
        self.inner_distribute_message(sender_id, receiver_id, stage_data.into());
    }

    pub fn distribute_messages_with_non_sender<
        StageData: Into<<Self as CeremonyRunnerStrategy>::CeremonyData>,
    >(
        &mut self,
        mut stage_data: StageMessages<StageData>,
        non_sender: &AccountId,
    ) {
        stage_data.remove(non_sender).unwrap();
        self.distribute_messages(stage_data);
        for (_, node) in self
            .nodes
            .iter_mut()
            .filter(|(account_id, _)| *account_id != non_sender)
        {
            node.force_stage_timeout();
        }
    }

    pub async fn gather_outgoing_messages<
        NextStageData: TryFrom<<Self as CeremonyRunnerStrategy>::CeremonyData, Error = Error> + Clone,
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

            let ceremony_data = <Self as CeremonyRunnerStrategy>::CeremonyData::try_from(data)
                .map_err(|err| {
                    format!(
                        "Expected outgoing ceremony data {}, got {:?}.",
                        std::any::type_name::<<Self as CeremonyRunnerStrategy>::CeremonyData>(),
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
        NextStageData: TryFrom<<Self as CeremonyRunnerStrategy>::CeremonyData, Error = Error> + Clone,
        StageData: Into<<Self as CeremonyRunnerStrategy>::CeremonyData>,
        Error: Display,
    >(
        &mut self,
        stage_data: StageMessages<StageData>,
    ) -> StageMessages<NextStageData> {
        self.distribute_messages(stage_data);
        self.gather_outgoing_messages().await
    }

    pub async fn run_stage_with_non_sender<
        NextStageData: TryFrom<<Self as CeremonyRunnerStrategy>::CeremonyData, Error = Error> + Clone,
        StageData: Into<<Self as CeremonyRunnerStrategy>::CeremonyData>,
        Error: Display,
    >(
        &mut self,
        stage_data: StageMessages<StageData>,
        non_sender: &AccountId,
    ) -> StageMessages<NextStageData> {
        self.distribute_messages_with_non_sender(stage_data, non_sender);
        self.gather_outgoing_messages().await
    }

    pub async fn try_gather_outcomes(
        &mut self,
        result_receivers: &mut HashMap<
            AccountId,
            CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output>,
        >,
    ) -> Option<Result<<Self as CeremonyRunnerStrategy>::CheckedOutput, BTreeSet<AccountId>>> {
        let results = self
            .nodes
            .iter_mut()
            .map(|(account_id, node)| {
                let result = result_receivers
                    .get_mut(account_id)
                    .unwrap()
                    .try_recv()
                    .ok()?;

                if result.is_err() {
                    assert!(node
                        .tag_cache
                        .contains_tag(<Self as CeremonyRunnerStrategy>::CEREMONY_FAILED_TAG));
                }

                Some((account_id.clone(), result))
            })
            .collect::<Vec<
                Option<(
                    AccountId,
                    Result<
                        <Self as CeremonyRunnerStrategy>::Output,
                        (BTreeSet<AccountId>, anyhow::Error),
                    >,
                )>,
            >>()
            .into_iter()
            .collect::<Option<
                HashMap<
                    AccountId,
                    Result<
                        <Self as CeremonyRunnerStrategy>::Output,
                        (BTreeSet<AccountId>, anyhow::Error),
                    >,
                >,
            >>()?;

        let (ok_results, all_reported_parties): (HashMap<_, _>, BTreeSet<_>) = results
            .into_iter()
            .partition_map(|(account_id, result)| match result {
                Ok(output) => Either::Left((account_id, output)),
                Err((reported_parties, _error)) => Either::Right(reported_parties),
            });

        if !ok_results.is_empty() && all_reported_parties.is_empty() {
            Some(Ok(self.post_successful_complete_check(ok_results)))
        } else if ok_results.is_empty() && !all_reported_parties.is_empty() {
            assert_eq!(
                all_reported_parties.len(),
                1,
                "Reported parties weren't the same for all nodes"
            );
            Some(Err(all_reported_parties.into_iter().next().unwrap()))
        } else {
            panic!("Ceremony results weren't consistently Ok() or Err() for all nodes");
        }
    }

    pub async fn complete(
        &mut self,
        mut result_receivers: HashMap<
            AccountId,
            CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output>,
        >,
    ) -> <Self as CeremonyRunnerStrategy>::CheckedOutput {
        assert_ok!(self
            .try_gather_outcomes(&mut result_receivers)
            .await
            .expect("Failed to get all ceremony outcomes"))
    }

    pub async fn try_complete_with_error(
        &mut self,
        bad_account_ids: &[AccountId],
        result_receivers: &mut HashMap<
            AccountId,
            CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output>,
        >,
    ) -> Option<()> {
        let reported = self
            .try_gather_outcomes(result_receivers)
            .await?
            .unwrap_err();
        assert_eq!(
            BTreeSet::from_iter(bad_account_ids.iter()),
            reported.iter().collect()
        );
        Some(())
    }

    pub async fn complete_with_error(
        &mut self,
        bad_account_ids: &[AccountId],
        mut result_receivers: HashMap<
            AccountId,
            CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output>,
        >,
    ) {
        self.try_complete_with_error(bad_account_ids, &mut result_receivers)
            .await
            .expect("Failed to get all ceremony outcomes");
    }

    pub fn request_without_gather(
        &mut self,
    ) -> HashMap<AccountId, CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output>> {
        self.nodes
            .keys()
            .sorted()
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .map(|id| {
                let result_receiver = self.request_ceremony(&id);
                (id, result_receiver)
            })
            .collect()
    }

    pub async fn request(
        &mut self,
    ) -> (
        HashMap<
            AccountId,
            HashMap<
                AccountId,
                <CeremonyRunner<CeremonyRunnerData> as CeremonyRunnerStrategy>::InitialStageData,
            >,
        >,
        HashMap<AccountId, CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output>>,
    ) {
        let result_receivers = self.request_without_gather();

        (self.gather_outgoing_messages().await, result_receivers)
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

pub type KeygenCeremonyRunner = CeremonyRunner<()>;
impl CeremonyRunnerStrategy for KeygenCeremonyRunner {
    type CeremonyData = KeygenData<Point>;
    type Output = KeygenResultInfo<Point>;
    type CheckedOutput = (KeyId, HashMap<AccountId, Self::Output>);
    type InitialStageData = keygen::HashComm1;
    const CEREMONY_FAILED_TAG: &'static str = KEYGEN_CEREMONY_FAILED;

    fn post_successful_complete_check(
        &self,
        outputs: HashMap<AccountId, Self::Output>,
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

    fn request_ceremony(
        &mut self,
        node_id: &AccountId,
    ) -> CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output> {
        let keygen_ceremony_details = self.keygen_ceremony_details();

        self.nodes
            .get_mut(node_id)
            .unwrap()
            .request_keygen(keygen_ceremony_details)
    }

    fn inner_distribute_message(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        stage_data: Self::CeremonyData,
    ) {
        self.nodes
            .get_mut(receiver_id)
            .unwrap()
            .ceremony_manager
            .process_keygen_data(sender_id.clone(), self.ceremony_id, stage_data);
    }
}
impl KeygenCeremonyRunner {
    pub fn new(nodes: HashMap<AccountId, Node>, ceremony_id: CeremonyId, rng: Rng) -> Self {
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
pub type SigningCeremonyRunner = CeremonyRunner<SigningCeremonyRunnerData>;
impl CeremonyRunnerStrategy for SigningCeremonyRunner {
    type CeremonyData = SigningData<Point>;
    type Output = EthSchnorrSignature;
    type CheckedOutput = EthSchnorrSignature;
    type InitialStageData = frost::Comm1<Point>;
    const CEREMONY_FAILED_TAG: &'static str = SIGNING_CEREMONY_FAILED;

    fn post_successful_complete_check(
        &self,
        outputs: HashMap<AccountId, Self::Output>,
    ) -> Self::CheckedOutput {
        let signature = all_same(outputs.into_iter().map(|(_, signature)| signature))
            .expect("Signatures don't match");

        verify_sig_with_aggkey(&signature, &self.ceremony_runner_data.key_id)
            .expect("Should be valid signature");

        signature
    }

    fn request_ceremony(
        &mut self,
        node_id: &AccountId,
    ) -> CeremonyResultReceiver<<Self as CeremonyRunnerStrategy>::Output> {
        let signing_ceremony_details = self.signing_ceremony_details(node_id);

        self.nodes
            .get_mut(node_id)
            .unwrap()
            .request_signing(signing_ceremony_details)
    }

    fn inner_distribute_message(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        stage_data: Self::CeremonyData,
    ) {
        self.nodes
            .get_mut(receiver_id)
            .unwrap()
            .ceremony_manager
            .process_signing_data(sender_id.clone(), self.ceremony_id, stage_data);
    }
}
impl SigningCeremonyRunner {
    pub fn new_with_all_signers(
        nodes: HashMap<AccountId, Node>,
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
        nodes: HashMap<AccountId, Node>,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        key_data: HashMap<AccountId, KeygenResultInfo<Point>>,
        message_hash: MessageHash,
        rng: Rng,
    ) -> (Self, HashMap<AccountId, Node>) {
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

pub async fn new_signing_ceremony_with_keygen() -> (SigningCeremonyRunner, HashMap<AccountId, Node>)
{
    let (key_id, key_data, _messages, nodes) =
        run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;

    SigningCeremonyRunner::new_with_threshold_subset_of_signers(
        nodes,
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
        let (stage_1_messages, result_receivers) = ceremony.request().await;
        visitor.yield_ceremony(&stage_1_messages)?;
        let stage_2_messages = ceremony.run_stage(stage_1_messages.clone()).await;
        visitor.yield_ceremony(&stage_2_messages)?;
        let stage_3_messages = ceremony.run_stage(stage_2_messages.clone()).await;
        visitor.yield_ceremony(&stage_3_messages)?;
        let stage_4_messages = ceremony.run_stage(stage_3_messages.clone()).await;
        visitor.yield_ceremony(&stage_4_messages)?;
        ceremony.distribute_messages(stage_4_messages.clone());

        Some((
            ceremony.complete(result_receivers).await,
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
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, keygen::Comm1<Point>>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComm2<Point>>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, keygen::SecretShare3<Point>>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, keygen::Complaints4>>,
    pub stage_5_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComplaints5>>,
}

pub async fn standard_keygen(
    mut keygen_ceremony: KeygenCeremonyRunner,
) -> (
    KeyId,
    HashMap<AccountId, KeygenResultInfo<Point>>,
    StandardKeygenMessages,
    HashMap<AccountId, Node>,
) {
    let (stage_1a_messages, result_receivers) = keygen_ceremony.request().await;
    let stage_2a_messages = keygen_ceremony.run_stage(stage_1a_messages.clone()).await;
    let stage_1_messages = keygen_ceremony.run_stage(stage_2a_messages.clone()).await;
    let stage_2_messages = keygen_ceremony.run_stage(stage_1_messages.clone()).await;
    let stage_3_messages = keygen_ceremony.run_stage(stage_2_messages.clone()).await;
    let stage_4_messages = keygen_ceremony.run_stage(stage_3_messages.clone()).await;
    let stage_5_messages = keygen_ceremony.run_stage(stage_4_messages.clone()).await;
    keygen_ceremony.distribute_messages(stage_5_messages.clone());
    let (key_id, key_data) = keygen_ceremony.complete(result_receivers).await;

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
    nodes: HashMap<AccountId, Node>,
    ceremony_id: CeremonyId,
) -> (
    KeyId,
    HashMap<AccountId, KeygenResultInfo<Point>>,
    StandardKeygenMessages,
    HashMap<AccountId, Node>,
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
        HashMap<AccountId, Node>,
    ),
    (),
> {
    let mut keygen_ceremony = KeygenCeremonyRunner::new(
        new_nodes_allow_high_pubkey(account_ids),
        DEFAULT_KEYGEN_CEREMONY_ID,
        Rng::from_entropy(),
    );
    let (stage_1a_messages, mut result_receivers) = keygen_ceremony.request().await;
    let stage_2_messages = run_stages!(
        keygen_ceremony,
        stage_1a_messages,
        keygen::VerifyHashComm2,
        keygen::Comm1<Point>,
        keygen::VerifyComm2<Point>
    );
    keygen_ceremony.distribute_messages(stage_2_messages);
    match keygen_ceremony
        .try_complete_with_error(&[], &mut result_receivers)
        .await
    {
        Some(_) => {
            for node in keygen_ceremony.nodes.values() {
                assert!(node.tag_cache.contains_tag(KEYGEN_REJECTED_INCOMPATIBLE));
            }
            Err(())
        }
        None => {
            let stage_3_messages = keygen_ceremony
                .gather_outgoing_messages::<keygen::SecretShare3<Point>, _>()
                .await;
            let stage_5_messages = run_stages!(
                keygen_ceremony,
                stage_3_messages,
                keygen::Complaints4,
                keygen::VerifyComplaints5
            );
            keygen_ceremony.distribute_messages(stage_5_messages);

            let (key_id, key_data) = keygen_ceremony.complete(result_receivers).await;

            Ok((key_id, key_data, keygen_ceremony.nodes))
        }
    }
}

#[derive(Clone)]
pub struct AllKeygenMessages {
    pub stage_1a_messages: HashMap<AccountId, HashMap<AccountId, keygen::HashComm1>>,
    pub stage_2a_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyHashComm2>>,
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, keygen::Comm1<Point>>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComm2<Point>>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, keygen::SecretShare3<Point>>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, keygen::Complaints4>>,
    pub stage_5_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComplaints5>>,
    pub stage_6_messages: HashMap<AccountId, HashMap<AccountId, keygen::BlameResponse6<Point>>>,
    pub stage_7_messages:
        HashMap<AccountId, HashMap<AccountId, keygen::VerifyBlameResponses7<Point>>>,
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
        let (stage_1a_messages, result_receivers) = ceremony.request().await;
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
            .unwrap() = SecretShare3::create_random(&mut ceremony.rng);

        let stage_4_messages = ceremony.run_stage(stage_3_messages.clone()).await;
        visitor.yield_ceremony(&stage_4_messages)?;
        let stage_5_messages = ceremony.run_stage(stage_4_messages.clone()).await;
        visitor.yield_ceremony(&stage_5_messages)?;
        let stage_6_messages = ceremony.run_stage(stage_5_messages.clone()).await;
        visitor.yield_ceremony(&stage_6_messages)?;
        let stage_7_messages = ceremony.run_stage(stage_6_messages.clone()).await;
        visitor.yield_ceremony(&stage_7_messages)?;
        ceremony.distribute_messages(stage_7_messages.clone());
        let (_key_id, key_data) = ceremony.complete(result_receivers).await;

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
pub fn gen_invalid_keygen_comm1(rng: &mut Rng) -> DKGUnverifiedCommitment<Point> {
    let (_, fake_comm1) = generate_shares_and_commitment(
        rng,
        // The commitment is only invalid because of the invalid context
        &HashContext([0; 32]),
        0,
        ThresholdParameters {
            share_count: ACCOUNT_IDS.len() as AuthorityCount,
            threshold: ACCOUNT_IDS.len() as AuthorityCount,
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

const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

impl Node {
    pub fn force_stage_timeout(&mut self) {
        self.ceremony_manager.expire_all();
        self.ceremony_manager.check_timeouts();
    }

    pub fn ensure_ceremony_at_signing_stage(
        &self,
        stage_number: usize,
        ceremony_id: CeremonyId,
    ) -> Result<()> {
        let stage = self.ceremony_manager.get_signing_stage_for(ceremony_id);
        let is_at_stage = match stage_number {
            STAGE_FINISHED_OR_NOT_STARTED => stage == None,
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

    /// Check is the ceremony is at the specified keygen BroadcastStage (0-5).
    pub fn ensure_ceremony_at_keygen_stage(
        &self,
        stage_number: usize,
        ceremony_id: CeremonyId,
    ) -> Result<()> {
        let stage = self.ceremony_manager.get_keygen_stage_for(ceremony_id);
        let is_at_stage = match stage_number {
            STAGE_FINISHED_OR_NOT_STARTED => stage == None,
            1 => stage.as_deref() == Some("BroadcastStage<HashCommitments1>"),
            2 => stage.as_deref() == Some("BroadcastStage<VerifyHashCommitmentsBroadcast2>"),
            3 => stage.as_deref() == Some("BroadcastStage<AwaitCommitments1>"),
            4 => stage.as_deref() == Some("BroadcastStage<VerifyCommitmentsBroadcast2>"),
            5 => stage.as_deref() == Some("BroadcastStage<SecretSharesStage3>"),
            6 => stage.as_deref() == Some("BroadcastStage<ComplaintsStage4>"),
            7 => stage.as_deref() == Some("BroadcastStage<VerifyComplaintsBroadcastStage5>"),
            8 => stage.as_deref() == Some("BroadcastStage<BlameResponsesStage6>"),
            9 => stage.as_deref() == Some("BroadcastStage<VerifyBlameResponsesBroadcastStage7>"),
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

pub fn switch_out_participant(participants: &mut Vec<AccountId>, keep: AccountId, add: AccountId) {
    participants.retain(|account_id| keep != *account_id);
    let account_id = participants
        .iter_mut()
        .find(|account_id| **account_id != add)
        .unwrap();
    *account_id = add;
    participants.push(keep);
}
