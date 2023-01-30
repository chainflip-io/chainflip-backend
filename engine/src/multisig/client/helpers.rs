use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	fmt::Display,
	sync::Arc,
	time::Duration,
};

use anyhow::Result;
use cf_primitives::{AuthorityCount, CeremonyId};
use futures::{stream, StreamExt};
use itertools::{Either, Itertools};

use async_trait::async_trait;

use rand_legacy::{RngCore, SeedableRng};

use slog::Logger;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use utilities::{assert_ok, success_threshold_from_share_count, threshold_from_share_count};

use crate::{
	common::{all_same, split_at},
	logging::test_utils::new_test_logger,
	multisig::{
		client::{
			ceremony_manager::{
				prepare_keygen_request, prepare_signing_request, CeremonyOutcome, CeremonyTrait,
				KeygenCeremony, SigningCeremony,
			},
			ceremony_runner::CeremonyRunner,
			common::{broadcast::BroadcastStage, CeremonyCommon, CeremonyFailureReason},
			keygen::{generate_key_data, HashComm1, HashContext, VerifyHashCommitmentsBroadcast2},
			signing, KeygenResultInfo, PartyIdxMapping, ThresholdParameters,
		},
		crypto::{ECPoint, Rng},
		CryptoScheme, KeyId,
	},
	p2p::OutgoingMultisigStageMessages,
};

use signing::{LocalSig3, SigningCommitment};

use keygen::{generate_shares_and_commitment, DKGUnverifiedCommitment};

use crate::{
	multisig::{
		client::{keygen, MultisigMessage},
		// This determines which crypto scheme will be used in tests
		// (we make arbitrary choice to use eth)
		crypto::eth::{EthSigning, Point},
	},
	testing::expect_recv_with_timeout,
};

use state_chain_runtime::{constants::common::MAX_STAGE_DURATION_SECONDS, AccountId};

use lazy_static::lazy_static;

/// Default seeds
pub const DEFAULT_KEYGEN_SEED: [u8; 32] = [8; 32];
pub const DEFAULT_SIGNING_SEED: [u8; 32] = [4; 32];

// Default ceremony ids used in many unit tests.
/// The initial latest ceremony id starts at 0,
/// so the first ceremony request must have a ceremony id of 1.
/// Also the SC will never send a ceremony request at id 0.
pub const INITIAL_LATEST_CEREMONY_ID: CeremonyId = 0;
// Ceremony ids must be consecutive.
pub const DEFAULT_KEYGEN_CEREMONY_ID: CeremonyId = INITIAL_LATEST_CEREMONY_ID + 1;
pub const DEFAULT_SIGNING_CEREMONY_ID: CeremonyId = DEFAULT_KEYGEN_CEREMONY_ID + 1;

/// Time it takes to cause a ceremony timeout (2 stages) with a small delay to allow for processing
pub const CEREMONY_TIMEOUT_DURATION: Duration =
	Duration::from_millis((((MAX_STAGE_DURATION_SECONDS * 2) as u64) * 1000) + 50);

lazy_static! {
	pub static ref ACCOUNT_IDS: Vec<AccountId> =
		[1, 2, 3, 4].iter().map(|i| AccountId::new([*i; 32])).collect();
}

pub type StageMessages<T> = HashMap<AccountId, HashMap<AccountId, T>>;
type KeygenCeremonyEth = KeygenCeremony<EthSigning>;

pub struct Node<C: CeremonyTrait> {
	own_account_id: AccountId,
	outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
	pub ceremony_runner: CeremonyRunner<C>,
	outgoing_p2p_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
	/// If any of the methods we called on the ceremony runner returned the outcome,
	/// it will be stored here
	outcome: Option<CeremonyOutcome<C>>,
	logger: slog::Logger,
}

fn new_node<C: CeremonyTrait>(account_id: AccountId) -> Node<C> {
	let logger = new_test_logger().new(slog::o!("account_id" => account_id.to_string()));

	let (outgoing_p2p_message_sender, outgoing_p2p_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let ceremony_runner = CeremonyRunner::new_unauthorised_for_test(logger.clone());

	Node {
		outgoing_p2p_message_sender,
		own_account_id: account_id,
		ceremony_runner,
		outgoing_p2p_message_receiver,
		outcome: None,
		logger,
	}
}

// Exists so some of the tests can easily modify signing requests
pub struct SigningCeremonyDetails<C: CryptoScheme> {
	pub rng: Rng,
	pub ceremony_id: CeremonyId,
	pub signers: BTreeSet<AccountId>,
	pub payload: C::SigningPayload,
	pub keygen_result_info: KeygenResultInfo<C>,
}

pub struct KeygenCeremonyDetails {
	pub rng: Rng,
	pub ceremony_id: CeremonyId,
	pub signers: BTreeSet<AccountId>,
}

impl<C: CeremonyTrait> Node<C> {
	fn on_ceremony_outcome(&mut self, outcome: CeremonyOutcome<C>) {
		match &outcome {
			Ok(_) => {
				slog::debug!(self.logger, "Node got successful outcome");
			},
			Err((reported_parties, failure_reason)) => {
				failure_reason.log(reported_parties, &self.logger);
			},
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

impl<C: CryptoScheme> Node<SigningCeremony<C>> {
	pub async fn request_signing(&mut self, signing_ceremony_details: SigningCeremonyDetails<C>) {
		let SigningCeremonyDetails {
			rng,
			ceremony_id,
			signers,
			payload: message_hash,
			keygen_result_info,
		} = signing_ceremony_details;

		let request = prepare_signing_request::<C>(
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

		if let Some(outcome) = self.ceremony_runner.on_ceremony_request(request.initial_stage).await
		{
			self.on_ceremony_outcome(outcome);
		}
	}
}

impl Node<KeygenCeremonyEth> {
	pub async fn request_keygen(&mut self, keygen_ceremony_details: KeygenCeremonyDetails) {
		let KeygenCeremonyDetails { ceremony_id, rng, signers } = keygen_ceremony_details;

		let request = prepare_keygen_request::<EthSigning>(
			ceremony_id,
			&self.own_account_id,
			signers,
			&self.outgoing_p2p_message_sender,
			rng,
			&self.logger,
		)
		.expect("invalid request");

		if let Some(outcome) = self.ceremony_runner.on_ceremony_request(request.initial_stage).await
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
		.map(|account_id| (account_id.clone(), new_node(account_id)))
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
		outputs: HashMap<AccountId, <Self::CeremonyType as CeremonyTrait>::Output>,
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
		Self { nodes, ceremony_id, ceremony_runner_data, rng }
	}

	pub fn get_mut_node(&mut self, account_id: &AccountId) -> &mut Node<C> {
		self.nodes.get_mut(account_id).unwrap()
	}

	pub fn select_account_ids<const COUNT: usize>(&self) -> [AccountId; COUNT] {
		self.nodes
			.keys()
			.cloned()
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
				self.distribute_message(&sender_id, &receiver_id, message).await;
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
		for (_, node) in self.nodes.iter_mut().filter(|(account_id, _)| *account_id != non_sender) {
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
				"Client output p2p message for ceremony_id {ceremony_id}, expected {self_ceremony_id}"
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
					// TODO Consider member functions on OutgoingMultisigStageMessages for
					// transforms
					match expect_recv_with_timeout(&mut node.outgoing_p2p_message_receiver).await {
						OutgoingMultisigStageMessages::Broadcast(receiver_ids, message) => {
							let next_data =
								message_to_next_stage_data(bincode::deserialize(&message).unwrap());
							receiver_ids
								.into_iter()
								.map(move |receiver_id| (receiver_id, next_data.clone()))
								.collect()
						},
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
		self.distribute_messages_with_non_sender(stage_data, non_sender).await;
		self.gather_outgoing_messages().await
	}

	// Checks if all nodes have an outcome and the outcomes are consistent, returning the outcome.
	async fn collect_and_check_outcomes(
		&mut self,
	) -> Option<
		Result<
			<Self as CeremonyRunnerStrategy>::CheckedOutput,
			(
				BTreeSet<AccountId>,
				<<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::FailureReason,
			),
		>,
	> {
		// Gather the outcomes from all the nodes
		let results: HashMap<_, _> = self
			.nodes
			.iter_mut()
			.filter_map(|(account_id, node)| {
				node.outcome.take().map(|outcome| (account_id.clone(), outcome))
			})
			.collect();

		if results.is_empty() {
			// No nodes have gotten an outcome yet
			return None
		}

		if results.len() != self.nodes.len() {
			panic!("Not all nodes had an outcome");
		}

		// Split up the outcomes into success and fails
		let (ok_results, (all_reported_parties, failure_reasons)): (
			HashMap<_, _>,
			(BTreeSet<_>, BTreeSet<_>),
		) = results.into_iter().partition_map(|(account_id, result)| match result {
			Ok(output) => Either::Left((account_id, output)),
			Err((reported_parties, reason)) => Either::Right((reported_parties, reason)),
		});

		if !ok_results.is_empty() && failure_reasons.is_empty() {
			// All nodes completed successfully
			Some(Ok(self.post_successful_complete_check(ok_results)))
		} else if ok_results.is_empty() && !failure_reasons.is_empty() {
			// All nodes reported failure, check that the reasons and reported nodes are the same
			assert_eq!(
				all_reported_parties.len(),
				1,
				"Reported parties weren't the same for all nodes"
			);
			assert_eq!(
				failure_reasons.len(),
				1,
				"The ceremony failure reason was not the same for all nodes: {failure_reasons:?}",
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
			.collect_and_check_outcomes()
			.await
			.expect("Failed to get all ceremony outcomes"))
	}

	async fn try_complete_with_error(
		&mut self,
		bad_account_ids: &[AccountId],
		expected_failure_reason: <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::FailureReason,
	) -> Option<()> {
		let (reported, reason) = self.collect_and_check_outcomes().await?.unwrap_err();
		assert_eq!(BTreeSet::from_iter(bad_account_ids.iter()), reported.iter().collect());
		assert_eq!(expected_failure_reason, reason);
		Some(())
	}

	/// Gathers the ceremony outcomes from all nodes,
	/// making sure they are identical and match the expected failure reason.
	pub async fn complete_with_error(
		&mut self,
		bad_account_ids: &[AccountId],
		expected_failure_reason: <<Self as CeremonyRunnerStrategy>::CeremonyType as CeremonyTrait>::FailureReason,
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
	type CheckedOutput = (KeyId, HashMap<AccountId, <Self::CeremonyType as CeremonyTrait>::Output>);
	type InitialStageData = keygen::HashComm1;

	fn post_successful_complete_check(
		&self,
		outputs: HashMap<AccountId, <Self::CeremonyType as CeremonyTrait>::Output>,
	) -> Self::CheckedOutput {
		let (_, public_key) = all_same(outputs.values().map(|keygen_result_info| {
			(keygen_result_info.params, keygen_result_info.key.get_public_key().get_element())
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

pub struct SigningCeremonyRunnerData<C: CryptoScheme> {
	pub key_id: KeyId,
	pub key_data: HashMap<AccountId, KeygenResultInfo<C>>,
	pub payload: C::SigningPayload,
}
pub type SigningCeremonyRunner<C> =
	CeremonyTestRunner<SigningCeremonyRunnerData<C>, SigningCeremony<C>>;

#[async_trait]
impl<C: CryptoScheme> CeremonyRunnerStrategy for SigningCeremonyRunner<C> {
	type CeremonyType = SigningCeremony<C>;
	type CheckedOutput = C::Signature;
	type InitialStageData = signing::Comm1<C::Point>;

	fn post_successful_complete_check(
		&self,
		outputs: HashMap<AccountId, <Self::CeremonyType as CeremonyTrait>::Output>,
	) -> Self::CheckedOutput {
		let signature = all_same(outputs.into_values()).expect("Signatures don't match");

		C::verify_signature(
			&signature,
			&self.ceremony_runner_data.key_id,
			&C::signing_payload_for_test(),
		)
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

impl<C: CryptoScheme> SigningCeremonyRunner<C> {
	pub fn new_with_all_signers(
		nodes: HashMap<AccountId, Node<SigningCeremony<C>>>,
		ceremony_id: CeremonyId,
		key_id: KeyId,
		key_data: HashMap<AccountId, KeygenResultInfo<C>>,
		payload: C::SigningPayload,
		rng: Rng,
	) -> Self {
		Self::inner_new(
			nodes,
			ceremony_id,
			SigningCeremonyRunnerData { key_id, key_data, payload },
			rng,
		)
	}

	pub fn new_with_threshold_subset_of_signers(
		nodes: HashMap<AccountId, Node<SigningCeremony<C>>>,
		ceremony_id: CeremonyId,
		key_id: KeyId,
		key_data: HashMap<AccountId, KeygenResultInfo<C>>,
		payload: C::SigningPayload,
		rng: Rng,
	) -> (Self, HashMap<AccountId, Node<SigningCeremony<C>>>) {
		let nodes_len = nodes.len();
		let (signers, non_signers) = split_at(
			nodes.into_iter().sorted_by_key(|(account_id, _)| account_id.clone()),
			success_threshold_from_share_count(nodes_len as AuthorityCount) as usize,
		);

		(
			Self::new_with_all_signers(signers, ceremony_id, key_id, key_data, payload, rng),
			non_signers,
		)
	}

	pub fn signing_ceremony_details(
		&mut self,
		account_id: &AccountId,
	) -> SigningCeremonyDetails<C> {
		use rand_legacy::Rng as _;

		SigningCeremonyDetails {
			ceremony_id: self.ceremony_id,
			rng: Rng::from_seed(self.rng.gen()),
			signers: self.nodes.keys().cloned().collect(),
			payload: self.ceremony_runner_data.payload.clone(),
			keygen_result_info: self.ceremony_runner_data.key_data[account_id].clone(),
		}
	}
}

pub async fn new_signing_ceremony<C: CryptoScheme>(
) -> (SigningCeremonyRunner<C>, HashMap<AccountId, Node<SigningCeremony<C>>>) {
	let (key_id, key_data) = generate_key_data::<C>(
		BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		&mut Rng::from_seed(DEFAULT_KEYGEN_SEED),
	);

	SigningCeremonyRunner::new_with_threshold_subset_of_signers(
		new_nodes(ACCOUNT_IDS.clone()),
		DEFAULT_SIGNING_CEREMONY_ID,
		key_id,
		key_data,
		C::signing_payload_for_test(),
		Rng::from_seed(DEFAULT_SIGNING_SEED),
	)
}

pub async fn standard_signing<C: CryptoScheme>(
	signing_ceremony: &mut SigningCeremonyRunner<C>,
) -> C::Signature {
	let stage_1_messages = signing_ceremony.request().await;
	let messages = run_stages!(
		signing_ceremony,
		stage_1_messages,
		signing::VerifyComm2<C::Point>,
		signing::LocalSig3<C::Point>,
		signing::VerifyLocalSig4<C::Point>
	);
	signing_ceremony.distribute_messages(messages).await;
	signing_ceremony.complete().await
}

pub async fn standard_keygen(
	mut keygen_ceremony: KeygenCeremonyRunner,
) -> (KeyId, HashMap<AccountId, KeygenResultInfo<EthSigning>>) {
	let stage_1_messages = keygen_ceremony.request().await;
	let messages = run_stages!(
		keygen_ceremony,
		stage_1_messages,
		keygen::VerifyHashComm2,
		keygen::CoeffComm3<Point>,
		keygen::VerifyCoeffComm4<Point>,
		keygen::SecretShare5<Point>,
		keygen::Complaints6,
		keygen::VerifyComplaints7
	);
	keygen_ceremony.distribute_messages(messages).await;
	keygen_ceremony.complete().await
}

pub async fn run_keygen(
	nodes: HashMap<AccountId, Node<KeygenCeremonyEth>>,
	ceremony_id: CeremonyId,
) -> (KeyId, HashMap<AccountId, KeygenResultInfo<EthSigning>>) {
	let keygen_ceremony =
		KeygenCeremonyRunner::new(nodes, ceremony_id, Rng::from_seed(DEFAULT_KEYGEN_SEED));
	standard_keygen(keygen_ceremony).await
}

/// Generate an invalid local sig for stage3
pub fn gen_invalid_local_sig<P: ECPoint>(rng: &mut Rng) -> LocalSig3<P> {
	use crate::multisig::crypto::ECScalar;

	signing::LocalSig3 { response: P::Scalar::random(rng) }
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
			threshold: threshold_from_share_count(share_count) as AuthorityCount,
		},
	);
	fake_comm1
}

pub fn gen_invalid_signing_comm1(rng: &mut Rng) -> SigningCommitment<Point> {
	SigningCommitment { d: Point::random(rng), e: Point::random(rng) }
}

pub fn gen_invalid_keygen_stage_2_state<P: ECPoint>(
	ceremony_id: CeremonyId,
	account_ids: BTreeSet<AccountId>,
	mut rng: Rng,
	logger: Logger,
) -> CeremonyRunner<KeygenCeremony<EthSigning>> {
	let validator_mapping = Arc::new(PartyIdxMapping::from_participants(account_ids.clone()));
	let common = CeremonyCommon {
		ceremony_id,
		own_idx: 0,
		all_idxs: BTreeSet::from_iter((0..account_ids.len()).into_iter().map(|idx| idx as u32)),
		outgoing_p2p_message_sender: tokio::sync::mpsc::unbounded_channel().0,
		validator_mapping,
		rng: rng.clone(),
		logger: logger.clone(),
	};

	let commitment = gen_invalid_keygen_comm1(&mut rng, account_ids.len() as u32);
	let processor = VerifyHashCommitmentsBroadcast2::new(
		common.clone(),
		commitment,
		account_ids.iter().map(|_| (0, None)).collect(),
		keygen::OutgoingShares(BTreeMap::new()),
		HashContext([0; 32]),
	);

	let stage = Box::new(BroadcastStage::new(processor, common));

	CeremonyRunner::new_authorised(stage, logger)
}

/// Generates key data using the DEFAULT_KEYGEN_SEED and returns the KeygenResultInfo for the first
/// signer.
pub fn get_key_data_for_test<C: CryptoScheme>(signers: BTreeSet<AccountId>) -> KeygenResultInfo<C> {
	generate_key_data::<C>(signers.clone(), &mut Rng::from_seed(DEFAULT_KEYGEN_SEED))
		.1
		.get(signers.iter().next().unwrap())
		.expect("should get keygen for an account")
		.to_owned()
}
