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
use itertools::Itertools;

use rand_legacy::{FromEntropy, SeedableRng};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedReceiver;
use utilities::success_threshold_from_share_count;

use crate::{
    common::{all_same, split_at},
    logging::{KEYGEN_CEREMONY_FAILED, KEYGEN_REJECTED_INCOMPATIBLE, SIGNING_CEREMONY_FAILED},
    multisig::{
        client::{
            keygen::{HashComm1, HashContext, KeygenOptions, SecretShare3},
            signing, CeremonyAbortReason, CeremonyError, CeremonyOutcome, MultisigData,
            ThresholdParameters,
        },
        crypto::Rng,
        KeyId, MessageHash, MultisigInstruction, SchnorrSignature,
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
        KeyDBMock, KeygenInfo, SigningInfo,
    },
};

use state_chain_runtime::AccountId;

pub type MultisigClientNoDB = MultisigClient<KeyDBMock>;

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
    pub client: MultisigClientNoDB,
    pub multisig_outcome_receiver: UnboundedReceiver<MultisigOutcome>,
    pub outgoing_p2p_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
    pub tag_cache: TagCache,
}

pub fn new_node(account_id: AccountId, keygen_options: KeygenOptions) -> Node {
    let (logger, tag_cache) = logging::test_utils::new_test_logger_with_tag_cache();
    let (multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let client = MultisigClient::new(
        account_id,
        KeyDBMock::default(),
        multisig_outcome_sender,
        outgoing_p2p_message_sender,
        keygen_options,
        &logger,
    );

    Node {
        client,
        multisig_outcome_receiver,
        outgoing_p2p_message_receiver,
        tag_cache,
    }
}

impl Node {
    pub async fn try_recv_outcome<Output: PartialEq + std::fmt::Debug>(
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
}

pub fn new_nodes<AccountIds: IntoIterator<Item = AccountId>>(
    account_ids: AccountIds,
    keygen_options: KeygenOptions,
) -> HashMap<AccountId, Node> {
    account_ids
        .into_iter()
        .map(|account_id| (account_id.clone(), new_node(account_id, keygen_options)))
        .collect()
}

pub trait CeremonyRunnerStrategy<Output> {
    type MappedOutcome: std::fmt::Debug;
    type InitialStageData;
    const CEREMONY_FAILED_TAG: &'static str;

    fn post_successful_complete_check(&self, outcome: Output) -> Self::MappedOutcome;

    fn multisig_instruction(&self) -> MultisigInstruction;
}

pub struct CeremonyRunner<CeremonyData, Output, CeremonyRunnerData> {
    pub nodes: HashMap<AccountId, Node>,
    pub ceremony_id: CeremonyId,
    pub ceremony_runner_data: CeremonyRunnerData,
    pub rng: Rng,
    _phantom: std::marker::PhantomData<(CeremonyData, Output)>,
}

impl<CeremonyData, Output, CeremonyRunnerData>
    CeremonyRunner<CeremonyData, Output, CeremonyRunnerData>
where
    CeremonyData:
        Into<MultisigData> + TryFrom<MultisigData, Error = MultisigData> + Clone + Display,
    Output: PartialEq + std::fmt::Debug,
    CeremonyOutcome<CeremonyId, Output>: TryFrom<MultisigOutcome, Error = MultisigOutcome>,
    Self: CeremonyRunnerStrategy<Output>,
    <Self as CeremonyRunnerStrategy<Output>>::InitialStageData:
        TryFrom<CeremonyData, Error = CeremonyData> + Clone,
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
            _phantom: Default::default(),
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

    pub fn distribute_messages<StageData: Into<CeremonyData>>(
        &mut self,
        stage_data: StageMessages<StageData>,
    ) {
        for (sender_id, messages) in stage_data {
            for (receiver_id, message) in messages {
                self.distribute_message(&sender_id, &receiver_id, message);
            }
        }
    }

    pub fn distribute_message<StageData: Into<CeremonyData>>(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        stage_data: StageData,
    ) {
        assert_ne!(receiver_id, sender_id);
        self.nodes
            .get_mut(receiver_id)
            .unwrap()
            .client
            .process_p2p_message(
                sender_id.clone(),
                MultisigMessage {
                    ceremony_id: self.ceremony_id,
                    data: stage_data.into().into(),
                },
            );
    }

    pub fn distribute_messages_with_non_sender<StageData: Into<CeremonyData>>(
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
            node.client.force_stage_timeout();
        }
    }

    async fn gather_outgoing_messages<
        NextStageData: TryFrom<CeremonyData, Error = Error> + Clone,
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

            let ceremony_data = CeremonyData::try_from(data)
                .map_err(|err| {
                    format!(
                        "Expected outgoing ceremony data {}, got {:?}.",
                        std::any::type_name::<CeremonyData>(),
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
        NextStageData: TryFrom<CeremonyData, Error = Error> + Clone,
        StageData: Into<CeremonyData>,
        Error: Display,
    >(
        &mut self,
        stage_data: StageMessages<StageData>,
    ) -> StageMessages<NextStageData> {
        self.distribute_messages(stage_data);
        self.gather_outgoing_messages().await
    }

    pub async fn run_stage_with_non_sender<
        NextStageData: TryFrom<CeremonyData, Error = Error> + Clone,
        StageData: Into<CeremonyData>,
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
    ) -> Option<Result<<Self as CeremonyRunnerStrategy<Output>>::MappedOutcome, CeremonyError>>
    {
        let outcomes = stream::iter(self.nodes.iter_mut())
            .then(|(account_id, node)| async move {
                let outcome = node.try_recv_outcome().await?;

                if outcome.result.is_err() {
                    assert!(node.tag_cache.contains_tag(
                        <Self as CeremonyRunnerStrategy<Output>>::CEREMONY_FAILED_TAG
                    ));
                }

                Some((account_id.clone(), outcome))
            })
            .collect::<Vec<Option<(AccountId, CeremonyOutcome<CeremonyId, Output>)>>>()
            .await
            .into_iter()
            .collect::<Option<HashMap<AccountId, CeremonyOutcome<CeremonyId, Output>>>>()?;

        let _ceremony_id = all_same(outcomes.iter().map(|(_account_id, outcome)| outcome.id))
            .expect("Inconsistent ceremony ids in the ceremony outcomes");

        Some(
            all_same(outcomes.into_iter().map(|(_, outcome)| {
                outcome.result.map_err(|(reason, reported)| {
                    (reason, reported.into_iter().sorted().collect::<Vec<_>>())
                })
            }))
            .expect("Ceremony results weren't consistent for all nodes")
            .map(|ok| self.post_successful_complete_check(ok)),
        )
    }

    pub async fn complete(&mut self) -> <Self as CeremonyRunnerStrategy<Output>>::MappedOutcome {
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
        let instruction = self.multisig_instruction();
        for (_, node) in self.nodes.iter_mut().sorted_by_key(|(id, _)| (*id).clone()) {
            node.client
                .process_multisig_instruction(instruction.clone(), &mut self.rng);
        }
    }

    pub async fn request(
        &mut self,
    ) -> HashMap<
        AccountId,
        HashMap<
            AccountId,
            <CeremonyRunner<CeremonyData, Output, CeremonyRunnerData> as CeremonyRunnerStrategy<
                Output,
            >>::InitialStageData,
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

pub type KeygenCeremonyRunner = CeremonyRunner<KeygenData, secp256k1::PublicKey, ()>;
impl CeremonyRunnerStrategy<secp256k1::PublicKey> for KeygenCeremonyRunner {
    type MappedOutcome = KeyId;
    type InitialStageData = keygen::HashComm1;
    const CEREMONY_FAILED_TAG: &'static str = KEYGEN_CEREMONY_FAILED;

    fn post_successful_complete_check(
        &self,
        public_key: secp256k1::PublicKey,
    ) -> Self::MappedOutcome {
        KeyId(public_key.serialize().into())
    }

    fn multisig_instruction(&self) -> MultisigInstruction {
        MultisigInstruction::Keygen(KeygenInfo {
            ceremony_id: self.ceremony_id,
            signers: self.nodes.keys().cloned().collect(),
        })
    }
}
impl KeygenCeremonyRunner {
    pub fn new(nodes: HashMap<AccountId, Node>, ceremony_id: CeremonyId, rng: Rng) -> Self {
        Self::inner_new(nodes, ceremony_id, (), rng)
    }
}

pub struct SigningCeremonyRunnerData {
    pub key_id: KeyId,
    pub message_hash: MessageHash,
}
pub type SigningCeremonyRunner =
    CeremonyRunner<SigningData, SchnorrSignature, SigningCeremonyRunnerData>;
impl CeremonyRunnerStrategy<SchnorrSignature> for SigningCeremonyRunner {
    type MappedOutcome = SchnorrSignature;
    type InitialStageData = frost::Comm1;
    const CEREMONY_FAILED_TAG: &'static str = SIGNING_CEREMONY_FAILED;

    fn post_successful_complete_check(&self, signature: SchnorrSignature) -> Self::MappedOutcome {
        verify_sig_with_aggkey(&signature, &self.ceremony_runner_data.key_id)
            .expect("Should be valid signature");
        signature
    }

    fn multisig_instruction(&self) -> MultisigInstruction {
        MultisigInstruction::Sign(SigningInfo::new(
            self.ceremony_id,
            self.ceremony_runner_data.key_id.clone(),
            self.ceremony_runner_data.message_hash.clone(),
            self.nodes.keys().cloned().collect(),
        ))
    }
}
impl SigningCeremonyRunner {
    pub fn new_with_all_signers(
        nodes: HashMap<AccountId, Node>,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        message_hash: MessageHash,
        rng: Rng,
    ) -> Self {
        Self::inner_new(
            nodes,
            ceremony_id,
            SigningCeremonyRunnerData {
                key_id,
                message_hash,
            },
            rng,
        )
    }

    pub fn new_with_threshold_subset_of_signers(
        nodes: HashMap<AccountId, Node>,
        ceremony_id: CeremonyId,
        key_id: KeyId,
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
            Self::new_with_all_signers(signers, ceremony_id, key_id, message_hash, rng),
            non_signers,
        )
    }

    pub fn signing_info(&self) -> SigningInfo {
        SigningInfo::new(
            self.ceremony_id,
            self.ceremony_runner_data.key_id.clone(),
            self.ceremony_runner_data.message_hash.clone(),
            self.nodes.keys().cloned().collect(),
        )
    }
}

pub async fn new_signing_ceremony_with_keygen() -> (SigningCeremonyRunner, HashMap<AccountId, Node>)
{
    let (key_id, _messages, nodes) = run_keygen(
        new_nodes(ACCOUNT_IDS.clone(), KeygenOptions::allowing_high_pubkey()),
        1,
    )
    .await;

    SigningCeremonyRunner::new_with_threshold_subset_of_signers(
        nodes,
        1,
        key_id,
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
    pub stage_1a_messages: HashMap<AccountId, HashMap<AccountId, keygen::HashComm1>>,
    pub stage_2a_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyHashComm2>>,
    pub stage_1_messages: HashMap<AccountId, HashMap<AccountId, keygen::Comm1>>,
    pub stage_2_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComm2>>,
    pub stage_3_messages: HashMap<AccountId, HashMap<AccountId, keygen::SecretShare3>>,
    pub stage_4_messages: HashMap<AccountId, HashMap<AccountId, keygen::Complaints4>>,
    pub stage_5_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyComplaints5>>,
}

pub async fn standard_keygen(
    mut keygen_ceremony: KeygenCeremonyRunner,
) -> (KeyId, StandardKeygenMessages, HashMap<AccountId, Node>) {
    let stage_1a_messages: HashMap<
        sp_runtime::AccountId32,
        HashMap<sp_runtime::AccountId32, HashComm1>,
    > = keygen_ceremony.request().await;

    let stage_2a_messages = keygen_ceremony.run_stage(stage_1a_messages.clone()).await;
    let stage_1_messages = keygen_ceremony.run_stage(stage_2a_messages.clone()).await;
    let stage_2_messages = keygen_ceremony.run_stage(stage_1_messages.clone()).await;
    let stage_3_messages = keygen_ceremony.run_stage(stage_2_messages.clone()).await;
    let stage_4_messages = keygen_ceremony.run_stage(stage_3_messages.clone()).await;
    let stage_5_messages = keygen_ceremony.run_stage(stage_4_messages.clone()).await;
    keygen_ceremony.distribute_messages(stage_5_messages.clone());
    let key_id = keygen_ceremony.complete().await;

    (
        key_id,
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
) -> (KeyId, StandardKeygenMessages, HashMap<AccountId, Node>) {
    let keygen_ceremony = KeygenCeremonyRunner::new(nodes, ceremony_id, Rng::from_seed([8; 32]));
    standard_keygen(keygen_ceremony).await
}

pub async fn run_keygen_with_err_on_high_pubkey<AccountIds: IntoIterator<Item = AccountId>>(
    account_ids: AccountIds,
) -> Result<(KeyId, HashMap<AccountId, Node>), ()> {
    let mut keygen_ceremony = KeygenCeremonyRunner::new(
        new_nodes(account_ids, KeygenOptions::default()),
        1,
        Rng::from_entropy(),
    );
    let stage_1a_messages = keygen_ceremony.request().await;
    let stage_2_messages = run_stages!(
        keygen_ceremony,
        stage_1a_messages,
        keygen::VerifyHashComm2,
        keygen::Comm1,
        keygen::VerifyComm2
    );
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

            let key_id = keygen_ceremony.complete().await;

            Ok((key_id, keygen_ceremony.nodes))
        }
    }
}

#[derive(Clone)]
pub struct AllKeygenMessages {
    pub stage_1a_messages: HashMap<AccountId, HashMap<AccountId, keygen::HashComm1>>,
    pub stage_2a_messages: HashMap<AccountId, HashMap<AccountId, keygen::VerifyHashComm2>>,
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
) -> BoxFuture<'a, Option<(KeyId, Vec<StageMessages<KeygenData>>, AllKeygenMessages)>> {
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
        let key_id = ceremony.complete().await;

        Some((
            key_id,
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

impl MultisigClientNoDB {
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
