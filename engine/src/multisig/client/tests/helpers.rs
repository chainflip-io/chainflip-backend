use std::{
    any::Any,
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Display,
    pin::Pin,
    time::Duration,
};

use anyhow::Result;
use cf_chains::eth::{AggKey, SchnorrVerificationComponents};
use futures::{stream, Future, StreamExt};
use itertools::{Either, Itertools};

use rand_legacy::{FromEntropy, SeedableRng};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedReceiver;
use utilities::success_threshold_from_share_count;

use crate::{
    common::{all_same, split_at},
    logging::{KEYGEN_CEREMONY_FAILED, KEYGEN_REJECTED_INCOMPATIBLE, SIGNING_CEREMONY_FAILED},
    multisig::{
        client::{
            ceremony_manager::CeremonyManager,
            keygen::{HashContext, KeygenOptions, SecretShare3},
            signing, CeremonyAbortReason, CeremonyError, CeremonyOutcome, KeygenResultInfo,
            MultisigData, ThresholdParameters,
        },
        crypto::Rng,
        KeyId, MessageHash, SchnorrSignature,
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
            MultisigClient, MultisigMessage, MultisigOutcome,
        },
        crypto::Point,
        KeyDBMock, KeygenRequest,
    },
};

use state_chain_runtime::AccountId;

use super::ACCOUNT_IDS;

use crate::multisig::tests::fixtures::MESSAGE_HASH;

pub const STAGE_FINISHED_OR_NOT_STARTED: usize = 0;

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
    pub ceremony_manager: CeremonyManager,
    pub multisig_outcome_receiver: UnboundedReceiver<MultisigOutcome>,
    pub outgoing_p2p_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
    pub tag_cache: TagCache,
}

pub fn new_node(account_id: AccountId) -> Node {
    let (logger, tag_cache) = logging::test_utils::new_test_logger_with_tag_cache();
    let (multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let ceremony_manager = CeremonyManager::new(
        account_id,
        multisig_outcome_sender,
        outgoing_p2p_message_sender,
        &logger,
    );

    Node {
        ceremony_manager,
        multisig_outcome_receiver,
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
    pub keygen_result_info: KeygenResultInfo,
}

pub struct KeygenCeremonyDetails {
    pub rng: Rng,
    pub ceremony_id: CeremonyId,
    pub signers: Vec<AccountId>,
    pub keygen_options: KeygenOptions,
}

impl Node {
    pub async fn try_recv_outcome<Output: std::fmt::Debug>(
        &mut self,
    ) -> Option<CeremonyOutcome<CeremonyId, Output>>
    where
        CeremonyOutcome<CeremonyId, Output>: TryFrom<MultisigOutcome, Error = MultisigOutcome>,
    {
        Some(
            CeremonyOutcome::<CeremonyId, Output>::try_from(
                recv_with_timeout(&mut self.multisig_outcome_receiver).await?,
            )
            .unwrap(),
        )
    }

    pub fn request_signing(&mut self, signing_ceremony_details: SigningCeremonyDetails) {
        self.ceremony_manager.on_request_to_sign(
            signing_ceremony_details.rng,
            signing_ceremony_details.message_hash,
            signing_ceremony_details.keygen_result_info,
            signing_ceremony_details.signers,
            signing_ceremony_details.ceremony_id,
        );
    }

    pub fn request_keygen(&mut self, keygen_ceremony_details: KeygenCeremonyDetails) {
        self.ceremony_manager.on_keygen_request(
            keygen_ceremony_details.rng,
            KeygenRequest {
                ceremony_id: keygen_ceremony_details.ceremony_id,
                signers: keygen_ceremony_details.signers,
            },
            keygen_ceremony_details.keygen_options,
        );
    }
}

pub fn new_nodes<AccountIds: IntoIterator<Item = AccountId>>(
    account_ids: AccountIds,
) -> HashMap<AccountId, Node> {
    account_ids
        .into_iter()
        .map(|account_id| (account_id.clone(), new_node(account_id)))
        .collect()
}

pub trait CeremonyRunnerStrategy {
    type CeremonyData: Into<MultisigData>
        + TryFrom<MultisigData, Error = MultisigData>
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

    fn request_ceremony(&mut self, node_id: &AccountId);

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
    CeremonyOutcome<CeremonyId, <Self as CeremonyRunnerStrategy>::Output>:
        TryFrom<MultisigOutcome, Error = MultisigOutcome>,
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

    async fn gather_outgoing_messages<
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
                            let next_data = message_to_next_stage_data(message);
                            receiver_ids
                                .into_iter()
                                .map(move |receiver_id| (receiver_id, next_data.clone()))
                                .collect()
                        }
                        OutgoingMultisigStageMessages::Private(messages) => messages
                            .into_iter()
                            .map(|(receiver_id, message)| {
                                (receiver_id, message_to_next_stage_data(message))
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
    ) -> Option<Result<<Self as CeremonyRunnerStrategy>::CheckedOutput, CeremonyError>> {
        let outcomes = stream::iter(self.nodes.iter_mut())
            .then(|(account_id, node)| async move {
                let outcome = node.try_recv_outcome().await?;

                if outcome.result.is_err() {
                    assert!(node
                        .tag_cache
                        .contains_tag(<Self as CeremonyRunnerStrategy>::CEREMONY_FAILED_TAG));
                }

                Some((account_id.clone(), outcome))
            })
            .collect::<Vec<
                Option<(
                    AccountId,
                    CeremonyOutcome<CeremonyId, <Self as CeremonyRunnerStrategy>::Output>,
                )>,
            >>()
            .await
            .into_iter()
            .collect::<Option<
                HashMap<
                    AccountId,
                    CeremonyOutcome<CeremonyId, <Self as CeremonyRunnerStrategy>::Output>,
                >,
            >>()?;

        let _ceremony_id = all_same(outcomes.iter().map(|(_account_id, outcome)| outcome.id))
            .expect("Inconsistent ceremony ids in the ceremony outcomes");

        let (ok_outcomes, error_outcomes): (HashMap<_, _>, Vec<_>) = outcomes
            .into_iter()
            .partition_map(|(account_id, outcome)| match outcome.result {
                Ok(output) => Either::Left((account_id, output)),
                Err(error) => Either::Right(error),
            });

        if !ok_outcomes.is_empty() && error_outcomes.is_empty() {
            Some(Ok(self.post_successful_complete_check(ok_outcomes)))
        } else if ok_outcomes.is_empty() && !error_outcomes.is_empty() {
            Some(Err(all_same(error_outcomes.into_iter().map(
                |(reason, reported)| (reason, reported.into_iter().sorted().collect::<Vec<_>>()),
            ))
            .expect("Ceremony Errors weren't consistent for all nodes")))
        } else {
            panic!("Ceremony results weren't consistently Ok() or Err() for all nodes");
        }
    }

    pub async fn complete(&mut self) -> <Self as CeremonyRunnerStrategy>::CheckedOutput {
        assert_ok!(self.try_gather_outcomes().await.unwrap())
    }

    pub async fn try_complete_with_error(&mut self, bad_account_ids: &[AccountId]) -> Option<()> {
        let (reason, reported) = self.try_gather_outcomes().await?.unwrap_err();
        assert_eq!(CeremonyAbortReason::Invalid, reason);
        assert_eq!(bad_account_ids, &reported[..]);
        Some(())
    }

    pub async fn complete_with_error(&mut self, bad_account_ids: &[AccountId]) {
        self.try_complete_with_error(bad_account_ids).await.unwrap();
    }

    pub fn request_without_gather(&mut self) {
        for id in self.nodes.keys().sorted().cloned().collect::<Vec<_>>() {
            self.request_ceremony(&id);
        }
    }

    pub async fn request(
        &mut self,
    ) -> HashMap<
        AccountId,
        HashMap<
            AccountId,
            <CeremonyRunner<CeremonyRunnerData> as CeremonyRunnerStrategy>::InitialStageData,
        >,
    > {
        self.request_without_gather();

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

pub struct KeygenCeremonyRunnerData {
    keygen_options: KeygenOptions,
}
pub type KeygenCeremonyRunner = CeremonyRunner<KeygenCeremonyRunnerData>;
impl CeremonyRunnerStrategy for KeygenCeremonyRunner {
    type CeremonyData = KeygenData;
    type Output = KeygenResultInfo;
    type CheckedOutput = (KeyId, HashMap<AccountId, Self::Output>);
    type InitialStageData = keygen::Comm1;
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

    fn request_ceremony(&mut self, node_id: &AccountId) {
        let keygen_ceremony_details = self.keygen_ceremony_details();

        self.nodes
            .get_mut(node_id)
            .unwrap()
            .request_keygen(keygen_ceremony_details);
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
    pub fn new(
        nodes: HashMap<AccountId, Node>,
        ceremony_id: CeremonyId,
        keygen_options: KeygenOptions,
        rng: Rng,
    ) -> Self {
        Self::inner_new(
            nodes,
            ceremony_id,
            KeygenCeremonyRunnerData { keygen_options },
            rng,
        )
    }

    pub fn keygen_ceremony_details(&mut self) -> KeygenCeremonyDetails {
        use rand_legacy::Rng as _;

        KeygenCeremonyDetails {
            ceremony_id: self.ceremony_id,
            rng: Rng::from_seed(self.rng.gen()),
            signers: self.nodes.keys().cloned().collect(),
            keygen_options: self.ceremony_runner_data.keygen_options,
        }
    }
}

pub struct SigningCeremonyRunnerData {
    pub key_id: KeyId,
    pub key_data: HashMap<AccountId, KeygenResultInfo>,
    pub message_hash: MessageHash,
}
pub type SigningCeremonyRunner = CeremonyRunner<SigningCeremonyRunnerData>;
impl CeremonyRunnerStrategy for SigningCeremonyRunner {
    type CeremonyData = SigningData;
    type Output = SchnorrSignature;
    type CheckedOutput = SchnorrSignature;
    type InitialStageData = frost::Comm1;
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

    fn request_ceremony(&mut self, node_id: &AccountId) {
        let signing_ceremony_details = self.signing_ceremony_details(node_id);

        self.nodes
            .get_mut(node_id)
            .unwrap()
            .request_signing(signing_ceremony_details);
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
        key_data: HashMap<AccountId, KeygenResultInfo>,
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
        key_data: HashMap<AccountId, KeygenResultInfo>,
        message_hash: MessageHash,
        rng: Rng,
    ) -> (Self, HashMap<AccountId, Node>) {
        let nodes_len = nodes.len();
        let (signers, non_signers) = split_at(
            nodes
                .into_iter()
                .sorted_by_key(|(account_id, _)| account_id.clone()),
            success_threshold_from_share_count(nodes_len as u32) as usize,
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
    let (key_id, key_data, _messages, nodes) = run_keygen(
        new_nodes(ACCOUNT_IDS.clone()),
        1,
        KeygenOptions::allowing_high_pubkey(),
    )
    .await;

    SigningCeremonyRunner::new_with_threshold_subset_of_signers(
        nodes,
        1,
        key_id,
        key_data,
        MESSAGE_HASH.clone(),
        Rng::from_seed([4; 32]),
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
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, frost::Comm1>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, frost::VerifyComm2>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, frost::LocalSig3>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, frost::VerifyLocalSig4>>,
}

pub fn standard_signing_coroutine<'a>(
    visitor: &'a mut CeremonyVisitor,
    ceremony: &'a mut SigningCeremonyRunner,
) -> BoxFuture<
    'a,
    Option<(
        SchnorrSignature,
        Vec<StageMessages<SigningData>>,
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
        ceremony.distribute_messages(stage_4_messages.clone());

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
) -> (SchnorrSignature, StandardSigningMessages) {
    let mut visitor = CeremonyVisitor::Complete;
    let (signature, _, messages) = standard_signing_coroutine(&mut visitor, signing_ceremony)
        .await
        .unwrap();
    (signature, messages)
}

pub struct StandardKeygenMessages {
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, keygen::Comm1>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComm2>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, keygen::SecretShare3>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, keygen::Complaints4>>,
    pub stage_5_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComplaints5>>,
}

pub async fn standard_keygen(
    mut keygen_ceremony: KeygenCeremonyRunner,
) -> (
    KeyId,
    HashMap<AccountId, KeygenResultInfo>,
    StandardKeygenMessages,
    HashMap<AccountId, Node>,
) {
    let stage_1_messages = keygen_ceremony.request().await;
    let stage_2_messages = keygen_ceremony.run_stage(stage_1_messages.clone()).await;
    let stage_3_messages = keygen_ceremony.run_stage(stage_2_messages.clone()).await;
    let stage_4_messages = keygen_ceremony.run_stage(stage_3_messages.clone()).await;
    let stage_5_messages = keygen_ceremony.run_stage(stage_4_messages.clone()).await;
    keygen_ceremony.distribute_messages(stage_5_messages.clone());
    let (key_id, key_data) = keygen_ceremony.complete().await;

    (
        key_id,
        key_data,
        StandardKeygenMessages {
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
    keygen_options: KeygenOptions,
) -> (
    KeyId,
    HashMap<AccountId, KeygenResultInfo>,
    StandardKeygenMessages,
    HashMap<AccountId, Node>,
) {
    let keygen_ceremony =
        KeygenCeremonyRunner::new(nodes, ceremony_id, keygen_options, Rng::from_seed([8; 32]));
    standard_keygen(keygen_ceremony).await
}

pub async fn run_keygen_with_err_on_high_pubkey<AccountIds: IntoIterator<Item = AccountId>>(
    account_ids: AccountIds,
) -> Result<
    (
        KeyId,
        HashMap<AccountId, KeygenResultInfo>,
        HashMap<AccountId, Node>,
    ),
    (),
> {
    let mut keygen_ceremony = KeygenCeremonyRunner::new(
        new_nodes(account_ids),
        1,
        KeygenOptions::default(),
        Rng::from_entropy(),
    );
    let stage_1_messages = keygen_ceremony.request().await;
    let stage_2_messages = keygen_ceremony
        .run_stage::<keygen::VerifyComm2, _, _>(stage_1_messages)
        .await;
    keygen_ceremony.distribute_messages(stage_2_messages);
    match keygen_ceremony.try_complete_with_error(&[]).await {
        Some(_) => {
            for node in keygen_ceremony.nodes.values() {
                assert!(node.tag_cache.contains_tag(KEYGEN_REJECTED_INCOMPATIBLE));
            }
            Err(())
        }
        None => {
            let stage_3_messages = keygen_ceremony
                .gather_outgoing_messages::<keygen::SecretShare3, _>()
                .await;
            let stage_5_messages = run_stages!(
                keygen_ceremony,
                stage_3_messages,
                keygen::Complaints4,
                keygen::VerifyComplaints5
            );
            keygen_ceremony.distribute_messages(stage_5_messages);

            let (key_id, key_data) = keygen_ceremony.complete().await;

            Ok((key_id, key_data, keygen_ceremony.nodes))
        }
    }
}

#[derive(Clone)]
pub struct AllKeygenMessages {
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, keygen::Comm1>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComm2>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, keygen::SecretShare3>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, keygen::Complaints4>>,
    pub stage_5_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComplaints5>>,
    pub stage_6_messages: HashMap<AccountId, HashMap<AccountId, keygen::BlameResponse6>>,
    pub stage_7_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyBlameResponses7>>,
}

pub fn all_stages_with_single_invalid_share_keygen_coroutine<'a>(
    visitor: &'a mut CeremonyVisitor,
    ceremony: &'a mut KeygenCeremonyRunner,
) -> BoxFuture<
    'a,
    Option<(
        HashMap<AccountId, KeygenResultInfo>,
        Vec<StageMessages<KeygenData>>,
        AllKeygenMessages,
    )>,
> {
    Box::pin(async move {
        let stage_1_messages = ceremony.request().await;
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
        let (_key_id, key_data) = ceremony.complete().await;

        Some((
            key_data,
            vec![
                into_generic_stage_data(stage_1_messages.clone()),
                into_generic_stage_data(stage_2_messages.clone()),
                into_generic_stage_data(stage_3_messages.clone()),
                into_generic_stage_data(stage_4_messages.clone()),
                into_generic_stage_data(stage_5_messages.clone()),
                into_generic_stage_data(stage_6_messages.clone()),
                into_generic_stage_data(stage_7_messages.clone()),
            ],
            AllKeygenMessages {
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
pub fn gen_invalid_local_sig(mut rng: &mut Rng) -> LocalSig3 {
    use crate::multisig::crypto::Scalar;
    frost::LocalSig3 {
        response: Scalar::random(&mut rng),
    }
}

// Make these member functions of the CeremonyRunner
pub fn gen_invalid_keygen_comm1(mut rng: &mut Rng) -> DKGUnverifiedCommitment {
    let (_, fake_comm1) = generate_shares_and_commitment(
        &mut rng,
        &HashContext([0; 32]),
        0,
        ThresholdParameters {
            share_count: ACCOUNT_IDS.len(),
            threshold: ACCOUNT_IDS.len(),
        },
    );
    fake_comm1
}

pub fn gen_invalid_signing_comm1(mut rng: &mut Rng) -> SigningCommitment {
    SigningCommitment {
        d: Point::random(&mut rng),
        e: Point::random(&mut rng),
    }
}

const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

impl Node {
    pub fn force_stage_timeout(&mut self) {
        self.ceremony_manager.expire_all();
        self.ceremony_manager.cleanup();
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
}

/// Using the given key_id, verify the signature is correct
pub fn verify_sig_with_aggkey(sig: &SchnorrSignature, key_id: &KeyId) -> Result<()> {
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
