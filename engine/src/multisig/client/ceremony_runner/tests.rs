use crate::{
	logging::test_utils::new_test_logger,
	multisig::{
		client::{
			ceremony_manager::{prepare_signing_request, KeygenCeremony, SigningCeremony},
			common::SigningStageName,
			gen_keygen_data_verify_hash_comm2, get_key_data_for_test,
			helpers::{
				cause_ceremony_timeout, gen_invalid_keygen_stage_2_state, ACCOUNT_IDS,
				DEFAULT_KEYGEN_SEED, DEFAULT_SIGNING_SEED,
			},
			signing::{
				gen_signing_data_stage1, gen_signing_data_stage2, gen_signing_data_stage4,
				SigningData,
			},
			SigningFailureReason,
		},
		crypto::CryptoScheme,
		eth::{EthSigning, Point},
		Rng,
	},
	p2p::OutgoingMultisigStageMessages,
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
	let rng = Rng::from_seed(DEFAULT_KEYGEN_SEED);
	let num_of_participants = ACCOUNT_IDS.len() as u32;

	// This test only works on message stage data that can have incorrect size (ie. not first
	// stage), so we must create a stage 2 state and add it to the ceremony managers keygen states,
	// allowing us to process a stage 2 message.
	let mut stage_2_state = gen_invalid_keygen_stage_2_state::<<EthSigning as CryptoScheme>::Point>(
		DEFAULT_CEREMONY_ID,
		BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		rng,
		new_test_logger(),
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
		CeremonyRunner::new_unauthorised(mpsc::unbounded_channel().0, new_test_logger());

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
	let our_account_id = ACCOUNT_IDS[0].clone();
	let sender_account_id = ACCOUNT_IDS[2].clone();

	// Create an unauthorised ceremony
	let mut ceremony_runner: CeremonyRunner<SigningCeremony<EthSigning>> =
		CeremonyRunner::new_unauthorised(mpsc::unbounded_channel().0, new_test_logger());

	// Process a stage 1 message (It should get delayed)
	assert_eq!(
		ceremony_runner
			.process_or_delay_message(sender_account_id.clone(), gen_signing_data_stage1())
			.await,
		None
	);

	// Process a signing request with only 2 participants (us and one other)
	let participants = BTreeSet::from_iter([our_account_id.clone(), sender_account_id]);
	let (outgoing_p2p_sender, _outgoing_p2p_receiver) = tokio::sync::mpsc::unbounded_channel();
	let initial_stage = prepare_signing_request(
		DEFAULT_CEREMONY_ID,
		&our_account_id.clone(),
		participants.clone(),
		get_key_data_for_test::<EthSigning>(participants),
		EthSigning::signing_payload_for_test(),
		&outgoing_p2p_sender,
		Rng::from_seed(DEFAULT_SIGNING_SEED),
		&new_test_logger(),
	)
	.unwrap()
	.initial_stage;
	ceremony_runner.on_ceremony_request(initial_stage).await;

	// Check that the ceremony processed the delayed message and caused it to progress to the next
	// stage.
	assert_eq!(
		ceremony_runner.stage.unwrap().get_stage_name(),
		SigningStageName::VerifyCommitmentsBroadcast2
	);
}

#[tokio::test]
async fn should_process_delayed_messages_after_finishing_a_stage() {
	let our_account_id = ACCOUNT_IDS[0].clone();
	let sender_account_id = ACCOUNT_IDS[1].clone();
	// This test must only have 2 participants, so a single message from the sender will cause the
	// stage to complete.
	let participants = BTreeSet::from_iter([our_account_id.clone(), sender_account_id.clone()]);

	// The relevant code path is the same for all stages,
	// so we just start at stage 1 for this test.
	let (mut ceremony_runner, _outgoing_p2p_receiver) =
		gen_stage_1_signing_state(our_account_id, participants.clone()).await;

	// Process a stage 2 message (It should get delayed)
	assert_eq!(
		ceremony_runner
			.process_or_delay_message(
				sender_account_id.clone(),
				gen_signing_data_stage2(participants.len() as AuthorityCount)
			)
			.await,
		None
	);

	// Process a stage 1 message. This will cause the ceremony to progress to stage 2 and process
	// the delayed message. The processing of the delayed message will cause the completion of stage
	// 2 and therefore fail with BroadcastFailure because the data we used was invalid.
	assert!(matches!(
		ceremony_runner
			.process_or_delay_message(sender_account_id.clone(), gen_signing_data_stage1())
			.await,
		Some(Err((
			_,
			SigningFailureReason::BroadcastFailure(
				_,
				SigningStageName::VerifyCommitmentsBroadcast2
			)
		)))
	));
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

/// Create a ceremony runner and process a signing request
async fn gen_stage_1_signing_state(
	our_account_id: AccountId,
	participants: BTreeSet<AccountId>,
) -> (CeremonyRunner<SigningCeremony<EthSigning>>, UnboundedReceiver<OutgoingMultisigStageMessages>)
{
	let logger = new_test_logger();

	let mut ceremony_runner =
		CeremonyRunner::new_unauthorised(tokio::sync::mpsc::unbounded_channel().0, logger.clone());

	let (outgoing_p2p_sender, outgoing_p2p_receiver) = tokio::sync::mpsc::unbounded_channel();
	let initial_stage = prepare_signing_request(
		DEFAULT_CEREMONY_ID,
		&our_account_id.clone(),
		BTreeSet::from_iter(participants.clone()),
		get_key_data_for_test::<EthSigning>(BTreeSet::from_iter(participants)),
		EthSigning::signing_payload_for_test(),
		&outgoing_p2p_sender,
		Rng::from_seed(DEFAULT_SIGNING_SEED),
		&logger,
	)
	.unwrap()
	.initial_stage;
	ceremony_runner.on_ceremony_request(initial_stage).await;

	(ceremony_runner, outgoing_p2p_receiver)
}

#[tokio::test]
async fn should_ignore_duplicate_message() {
	let our_account_id = ACCOUNT_IDS[0].clone();
	let sender_account_id = ACCOUNT_IDS[1].clone();
	// This test must have more then 2 participants to stop the stage advancing after a single
	// message
	let participants = BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned());

	// The relevant code path is the same for all stages,
	// so we just use a stage 1 state for this test.
	let (mut stage_1_state, _) = gen_stage_1_signing_state(our_account_id, participants).await;

	// Process a valid message
	assert_eq!(
		stage_1_state
			.process_or_delay_message(sender_account_id.clone(), gen_signing_data_stage1())
			.await,
		None
	);

	// Process a duplicate of that message
	ensure_message_is_ignored(&mut stage_1_state, sender_account_id, gen_signing_data_stage1())
		.await;
}

#[tokio::test]
async fn should_ignore_message_from_non_participating_account() {
	let our_account_id = ACCOUNT_IDS[0].clone();
	let mut participants = BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned());
	let non_participant_id = ACCOUNT_IDS[2].clone();
	participants.remove(&non_participant_id);
	assert!(!participants.contains(&non_participant_id));

	// The relevant code path is the same for all stages,
	// so we just use a stage 1 state for this test.
	let (mut stage_1_state, _) = gen_stage_1_signing_state(our_account_id, participants).await;

	// Process a message from a node that is not in the signing ceremony
	ensure_message_is_ignored(&mut stage_1_state, non_participant_id, gen_signing_data_stage1())
		.await;
}

#[tokio::test]
async fn should_ignore_message_from_unknown_account_id() {
	let our_account_id = ACCOUNT_IDS[0].clone();
	let participants = BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned());
	let unknown_id = AccountId::new([0; 32]);
	assert!(!ACCOUNT_IDS.contains(&unknown_id));

	// The relevant code path is the same for all stages,
	// so we just use a stage 1 state for this test.
	let (mut stage_1_state, _) = gen_stage_1_signing_state(our_account_id, participants).await;

	// Process a message from an unknown AccountId
	ensure_message_is_ignored(&mut stage_1_state, unknown_id, gen_signing_data_stage1()).await;
}

#[tokio::test]
async fn should_ignore_message_from_unexpected_stage() {
	let our_account_id = ACCOUNT_IDS[0].clone();
	let sender_account_id = ACCOUNT_IDS[1].clone();
	let participants = BTreeSet::from_iter([our_account_id.clone(), sender_account_id.clone()]);

	// The relevant code path is the same for all stages,
	// so we just use a stage 1 state for this test.
	let (mut stage_1_state, _) =
		gen_stage_1_signing_state(our_account_id, participants.clone()).await;

	// Process a message from an unexpected stage
	ensure_message_is_ignored(
		&mut stage_1_state,
		sender_account_id,
		gen_signing_data_stage4(participants.len() as u32),
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
			get_key_data_for_test::<EthSigning>(BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned())),
			EthSigning::signing_payload_for_test(),
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
