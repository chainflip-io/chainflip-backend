use std::sync::Arc;

use crate::{
	logging::test_utils::new_test_logger,
	multisig::{
		client::{
			ceremony_manager::{prepare_signing_request, KeygenCeremony, SigningCeremony},
			common::{broadcast::BroadcastStage, CeremonyCommon},
			gen_keygen_data_verify_hash_comm2, gen_signing_data_stage1, gen_signing_data_stage4,
			get_key_data_for_test,
			helpers::{
				cause_ceremony_timeout, gen_invalid_keygen_stage_2_state, ACCOUNT_IDS,
				DEFAULT_KEYGEN_SEED, DEFAULT_SIGNING_SEED,
			},
			signing::{
				frost::SigningData, frost_stages::AwaitCommitments1, SigningStateCommonInfo,
			},
			KeygenResult, PartyIdxMapping,
		},
		crypto::CryptoScheme,
		eth::{EthSigning, Point},
		tests::fixtures::MESSAGE_HASH,
		Rng,
	},
};

use rand_legacy::SeedableRng;
use sp_runtime::AccountId32;
use tokio::sync::mpsc;

use super::*;

type CeremonyRunnerChannels = (
	UnboundedSender<(AccountId32, SigningData<Point>)>,
	tokio::sync::oneshot::Sender<PreparedRequest<SigningCeremony<EthSigning>>>,
	UnboundedReceiver<(CeremonyId, CeremonyOutcome<SigningCeremony<EthSigning>>)>,
);

// For these tests the ceremony id does not matter
const DEFAULT_CEREMONY_ID: CeremonyId = 1;

/// Spawn a signing ceremony runner task in the an unauthorised state with some default parameters
fn spawn_signing_ceremony_runner(
) -> (tokio::task::JoinHandle<Result<(), anyhow::Error>>, CeremonyRunnerChannels) {
	let (message_sender, message_receiver) = mpsc::unbounded_channel();
	let (request_sender, request_receiver) = oneshot::channel();
	let (outcome_sender, outcome_receiver) = mpsc::unbounded_channel();

	let task_handle = tokio::spawn(CeremonyRunner::<SigningCeremony<EthSigning>>::run(
		DEFAULT_CEREMONY_ID,
		message_receiver,
		request_receiver,
		outcome_sender,
		new_test_logger(),
	));

	(task_handle, (message_sender, request_sender, outcome_receiver))
}

#[tokio::test]
async fn should_ignore_stage_data_with_incorrect_size() {
	let logger = new_test_logger();
	let rng = Rng::from_seed(DEFAULT_KEYGEN_SEED);
	let num_of_participants = ACCOUNT_IDS.len() as u32;

	// This test only works on message stage data that can have incorrect size (ie. not first
	// stage), so we must create a stage 2 state and add it to the ceremony managers keygen states,
	// allowing us to process a stage 2 message.
	let mut stage_2_state = gen_invalid_keygen_stage_2_state::<<EthSigning as CryptoScheme>::Point>(
		DEFAULT_CEREMONY_ID,
		BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		rng,
		logger.clone(),
	);

	// Built a stage 2 message that has the incorrect number of elements
	let stage_2_data = gen_keygen_data_verify_hash_comm2(num_of_participants + 1);

	// Process the bad message and it should get rejected
	assert_eq!(
		stage_2_state
			.process_or_delay_message(ACCOUNT_IDS[0].clone(), stage_2_data)
			.await,
		None
	);

	// Check that the bad message was ignored, so the stage is still awaiting all
	// num_of_participants messages.
	assert_eq!(stage_2_state.get_awaited_parties_count(), Some(num_of_participants));
}

#[tokio::test]
async fn should_ignore_non_stage_1_messages_while_unauthorised() {
	let num_of_participants = ACCOUNT_IDS.len() as u32;

	// Create an unauthorised ceremony
	let mut unauthorised_ceremony_runner: CeremonyRunner<KeygenCeremony<EthSigning>> =
		CeremonyRunner::new_unauthorised(
			DEFAULT_CEREMONY_ID,
			mpsc::unbounded_channel().0,
			&new_test_logger(),
		);

	// Process a stage 2 message
	assert_eq!(
		unauthorised_ceremony_runner
			.process_or_delay_message(
				ACCOUNT_IDS[0].clone(),
				gen_keygen_data_verify_hash_comm2(num_of_participants)
			)
			.await,
		None
	);

	// Check that the message was ignored and not delayed
	assert_eq!(unauthorised_ceremony_runner.delayed_messages.len(), 0);
}

#[tokio::test]
async fn should_delay_stage_1_message_while_unauthorised() {
	// Create an unauthorised ceremony
	let mut unauthorised_ceremony_runner: CeremonyRunner<SigningCeremony<EthSigning>> =
		CeremonyRunner::new_unauthorised(
			DEFAULT_CEREMONY_ID,
			mpsc::unbounded_channel().0,
			&new_test_logger(),
		);

	// Process a stage 1 message
	assert_eq!(
		unauthorised_ceremony_runner
			.process_or_delay_message(ACCOUNT_IDS[0].clone(), gen_signing_data_stage1())
			.await,
		None
	);

	// Check that the message was delayed
	assert_eq!(unauthorised_ceremony_runner.delayed_messages.len(), 1);
}

/// Sends a message to the state and makes sure it was ignored (not delayed or accepted)
async fn ensure_message_is_ignored(
	state: &mut CeremonyRunner<SigningCeremony<EthSigning>>,
	sender_id: AccountId,
	message: SigningData<Point>,
) {
	let awaited_parties_before_message = state.get_awaited_parties_count();

	assert_eq!(state.process_or_delay_message(sender_id, message).await, None);

	assert!(state.delayed_messages.is_empty());
	assert_eq!(state.get_awaited_parties_count(), awaited_parties_before_message);
}

/// Setup a ceremony runner for a signing ceremony at stage 1 (in an authorised state)
fn gen_stage_1_signing_state(
	own_idx: AuthorityCount,
	signing_idxs: BTreeSet<AuthorityCount>,
) -> CeremonyRunner<SigningCeremony<EthSigning>> {
	let rng = Rng::from_seed(DEFAULT_SIGNING_SEED);
	let logger = new_test_logger();
	let key: Arc<KeygenResult<Point>> =
		get_key_data_for_test(BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned())).key;

	let validator_mapping = Arc::new(PartyIdxMapping::from_participants(BTreeSet::from_iter(
		ACCOUNT_IDS.iter().cloned(),
	)));
	let common = CeremonyCommon {
		ceremony_id: DEFAULT_CEREMONY_ID,
		own_idx,
		all_idxs: signing_idxs,
		outgoing_p2p_message_sender: tokio::sync::mpsc::unbounded_channel().0,
		validator_mapping,
		rng,
		logger: logger.clone(),
	};

	let processor = AwaitCommitments1::<EthSigning>::new(
		common.clone(),
		SigningStateCommonInfo { data: MESSAGE_HASH.clone(), key },
	);

	let stage = Box::new(BroadcastStage::new(processor, common));

	CeremonyRunner::<SigningCeremony<EthSigning>>::new_authorised(
		DEFAULT_CEREMONY_ID,
		stage,
		logger,
	)
}

#[tokio::test]
async fn should_ignore_duplicate_message() {
	let own_idx = 0;
	let sender_idx = 1;
	let signing_idxs = BTreeSet::from_iter([own_idx, sender_idx]);

	// The relevant code path is the same for all stages,
	// so we just use a stage 1 state for this test.
	let mut stage_1_state = gen_stage_1_signing_state(own_idx, signing_idxs.clone());

	// Process a valid message
	assert_eq!(
		stage_1_state
			.process_or_delay_message(
				ACCOUNT_IDS[sender_idx as usize].clone(),
				gen_signing_data_stage1()
			)
			.await,
		None
	);

	// Process a duplicate of that message
	ensure_message_is_ignored(
		&mut stage_1_state,
		ACCOUNT_IDS[sender_idx as usize].clone(),
		gen_signing_data_stage1(),
	)
	.await;
}

#[tokio::test]
async fn should_ignore_message_from_non_participating_account() {
	let own_idx = 0;
	let sender_idx = 1;
	let signing_idxs = BTreeSet::from_iter([own_idx, sender_idx]);
	let non_participant_idx = 2;
	assert!(!signing_idxs.contains(&(non_participant_idx)));

	// The relevant code path is the same for all stages,
	// so we just use a stage 1 state for this test.
	let mut stage_1_state = gen_stage_1_signing_state(own_idx, signing_idxs.clone());

	// Process a message from a node that is not in the signing ceremony
	ensure_message_is_ignored(
		&mut stage_1_state,
		ACCOUNT_IDS[non_participant_idx as usize].clone(),
		gen_signing_data_stage1(),
	)
	.await;
}

#[tokio::test]
async fn should_ignore_message_from_unknown_account_id() {
	let own_idx = 0;
	let sender_idx = 1;
	let signing_idxs = BTreeSet::from_iter([own_idx, sender_idx]);
	let unknown_id = AccountId::new([0; 32]);

	// The relevant code path is the same for all stages,
	// so we just use a stage 1 state for this test.
	let mut stage_1_state = gen_stage_1_signing_state(own_idx, signing_idxs.clone());

	// Process a message from an unknown AccountId
	ensure_message_is_ignored(&mut stage_1_state, unknown_id, gen_signing_data_stage1()).await;
}

#[tokio::test]
async fn should_ignore_message_from_unexpected_stage() {
	let own_idx = 0;
	let sender_idx = 1;
	let signing_idxs = BTreeSet::from_iter([own_idx, sender_idx]);

	// The relevant code path is the same for all stages,
	// so we just use a stage 1 state for this test.
	let mut stage_1_state = gen_stage_1_signing_state(own_idx, signing_idxs.clone());

	// Process a message from an unexpected stage
	ensure_message_is_ignored(
		&mut stage_1_state,
		ACCOUNT_IDS[sender_idx as usize].clone(),
		gen_signing_data_stage4(signing_idxs.len() as u32),
	)
	.await;
}

#[tokio::test]
async fn should_not_timeout_unauthorised_ceremony() {
	let (task_handle, _channels) = spawn_signing_ceremony_runner();

	// Advance time, then check that the task did not end due to a timeout
	cause_ceremony_timeout().await;
	assert!(!task_handle.is_finished());
}

#[tokio::test]
async fn should_timeout_authorised_ceremony() {
	let (task_handle, (_message_sender, request_sender, _outcome_receiver)) =
		spawn_signing_ceremony_runner();

	// Send a signing request
	let (outgoing_p2p_sender, _outgoing_p2p_receiver) = tokio::sync::mpsc::unbounded_channel();
	let _res = request_sender.send(
		prepare_signing_request(
			DEFAULT_CEREMONY_ID,
			&ACCOUNT_IDS[0],
			BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
			get_key_data_for_test(BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned())),
			MESSAGE_HASH.clone(),
			&outgoing_p2p_sender,
			Rng::from_seed(DEFAULT_SIGNING_SEED),
			&new_test_logger(),
		)
		.unwrap(),
	);

	// Advance time, then check that the task was ended due to the timeout
	assert!(!task_handle.is_finished());
	cause_ceremony_timeout().await;
	assert!(task_handle.is_finished());
}
