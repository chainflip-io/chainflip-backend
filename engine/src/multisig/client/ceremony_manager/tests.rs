use std::{collections::BTreeSet, pin::Pin, time::Duration};

use crate::{
	constants::CEREMONY_ID_WINDOW,
	logging::test_utils::new_test_logger,
	multisig::{
		client::{
			self,
			ceremony_manager::{
				CeremonyHandle, CeremonyManager, CeremonyRequestState, SigningCeremony,
			},
			ceremony_runner::CeremonyRunner,
			common::{BroadcastFailureReason, SigningFailureReason, SigningStageName},
			gen_keygen_data_hash_comm1, get_key_data_for_test,
			helpers::{
				ACCOUNT_IDS, CEREMONY_TIMEOUT_DURATION, DEFAULT_KEYGEN_SEED, DEFAULT_SIGNING_SEED,
				INITIAL_LATEST_CEREMONY_ID,
			},
			keygen::KeygenData,
			CeremonyRequest, CeremonyRequestDetails, KeygenRequestDetails, MultisigData,
			SigningRequestDetails,
		},
		crypto::{CryptoScheme, Rng},
		eth::{EthSchnorrSignature, EthSigning},
	},
	p2p::OutgoingMultisigStageMessages,
	task_scope::task_scope,
};
use anyhow::Result;
use cf_primitives::{AccountId, CeremonyId};
use client::MultisigMessage;
use futures::{Future, FutureExt};
use rand_legacy::SeedableRng;
use sp_runtime::AccountId32;
use tokio::sync::{mpsc, oneshot};
use utilities::threshold_from_share_count;

/// Run on_request_to_sign on a ceremony manager, using a junk key and default ceremony id and data.
async fn run_on_request_to_sign<C: CryptoScheme>(
	ceremony_manager: &mut CeremonyManager<C>,
	participants: BTreeSet<sp_runtime::AccountId32>,
	ceremony_id: CeremonyId,
) -> oneshot::Receiver<
	Result<<C as CryptoScheme>::Signature, (BTreeSet<AccountId32>, SigningFailureReason)>,
> {
	let (result_sender, result_receiver) = oneshot::channel();
	task_scope(|scope| {
		let future: Pin<Box<dyn Future<Output = Result<()>> + Send>> = async {
			ceremony_manager.on_request_to_sign(
				ceremony_id,
				participants,
				C::signing_payload_for_test(),
				get_key_data_for_test::<C>(BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned())),
				Rng::from_seed(DEFAULT_SIGNING_SEED),
				result_sender,
				scope,
			);
			anyhow::bail!("End the future so we can complete the test");
		}
		.boxed();
		future
	})
	.await
	.unwrap_err();
	result_receiver
}

/// Create an Eth ceremony manager with a dropped p2p receiver.
fn new_ceremony_manager_for_test(
	our_account_id: AccountId,
	latest_ceremony_id: CeremonyId,
) -> CeremonyManager<EthSigning> {
	CeremonyManager::<EthSigning>::new(
		our_account_id,
		tokio::sync::mpsc::unbounded_channel().0,
		latest_ceremony_id,
		&new_test_logger(),
	)
}

/// Sends a signing request to the ceremony manager with a junk key and some default values.
fn send_signing_request(
	ceremony_request_sender: &tokio::sync::mpsc::UnboundedSender<CeremonyRequest<EthSigning>>,
	participants: BTreeSet<AccountId32>,
	ceremony_id: CeremonyId,
) -> tokio::sync::oneshot::Receiver<
	Result<EthSchnorrSignature, (BTreeSet<AccountId32>, SigningFailureReason)>,
> {
	let (result_sender, result_receiver) = oneshot::channel();

	let request = CeremonyRequest {
		ceremony_id,
		details: Some(CeremonyRequestDetails::Sign(SigningRequestDetails::<EthSigning> {
			participants,
			payload: EthSigning::signing_payload_for_test(),
			keygen_result_info: get_key_data_for_test::<EthSigning>(BTreeSet::from_iter(
				ACCOUNT_IDS.iter().cloned(),
			)),
			rng: Rng::from_seed(DEFAULT_SIGNING_SEED),
			result_sender,
		})),
	};

	let _result = ceremony_request_sender.send(request);

	result_receiver
}

fn spawn_ceremony_manager(
	our_account_id: AccountId,
	latest_ceremony_id: CeremonyId,
) -> (
	mpsc::UnboundedSender<CeremonyRequest<EthSigning>>,
	mpsc::UnboundedSender<(AccountId32, Vec<u8>)>,
	mpsc::UnboundedReceiver<OutgoingMultisigStageMessages>,
) {
	let (ceremony_request_sender, ceremony_request_receiver) = mpsc::unbounded_channel();
	let (incoming_p2p_sender, incoming_p2p_receiver) = mpsc::unbounded_channel();
	let (outgoing_p2p_sender, outgoing_p2p_receiver) = mpsc::unbounded_channel();
	let ceremony_manager = CeremonyManager::<EthSigning>::new(
		our_account_id,
		outgoing_p2p_sender,
		latest_ceremony_id,
		&new_test_logger(),
	);
	tokio::spawn(ceremony_manager.run(ceremony_request_receiver, incoming_p2p_receiver));

	(ceremony_request_sender, incoming_p2p_sender, outgoing_p2p_receiver)
}

#[tokio::test]
#[should_panic]
async fn should_panic_keygen_request_if_not_participating() {
	let non_participating_id = AccountId::new([0; 32]);
	assert!(!ACCOUNT_IDS.contains(&non_participating_id));

	// Create a new ceremony manager with the non_participating_id
	let mut ceremony_manager =
		new_ceremony_manager_for_test(non_participating_id, INITIAL_LATEST_CEREMONY_ID);

	// Send a keygen request where participants doesn't include non_participating_id
	let (result_sender, _result_receiver) = oneshot::channel();
	task_scope(|scope| {
		async {
			ceremony_manager.on_keygen_request(
				INITIAL_LATEST_CEREMONY_ID + 1,
				BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
				Rng::from_seed(DEFAULT_KEYGEN_SEED),
				result_sender,
				scope,
			);
			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

#[tokio::test]
#[should_panic]
async fn should_panic_rts_if_not_participating() {
	let non_participating_id = AccountId::new([0; 32]);
	assert!(!ACCOUNT_IDS.contains(&non_participating_id));

	// Create a new ceremony manager with the non_participating_id
	let mut ceremony_manager =
		new_ceremony_manager_for_test(non_participating_id, INITIAL_LATEST_CEREMONY_ID);

	// Send a signing request where participants doesn't include non_participating_id
	run_on_request_to_sign(
		&mut ceremony_manager,
		BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		INITIAL_LATEST_CEREMONY_ID + 1,
	)
	.await;
}

#[tokio::test]
async fn should_ignore_rts_with_insufficient_number_of_signers() {
	// Create a list of signers that is equal to the threshold (not enough to generate a signature)
	let threshold = threshold_from_share_count(ACCOUNT_IDS.len() as u32) as usize;
	let not_enough_participants = BTreeSet::from_iter(ACCOUNT_IDS[0..threshold].iter().cloned());

	let mut ceremony_manager =
		new_ceremony_manager_for_test(ACCOUNT_IDS[0].clone(), INITIAL_LATEST_CEREMONY_ID);

	// Send a signing request with not enough participants
	let mut result_receiver = run_on_request_to_sign(
		&mut ceremony_manager,
		not_enough_participants,
		INITIAL_LATEST_CEREMONY_ID + 1,
	)
	.await;

	// Receive the NotEnoughSigners error result
	assert_eq!(
		result_receiver.try_recv().expect("Failed to receive ceremony result"),
		Err((BTreeSet::default(), SigningFailureReason::NotEnoughSigners,))
	);
}

#[tokio::test]
async fn should_ignore_rts_with_unknown_signer_id() {
	let our_account_id_idx = 0;
	let unknown_signer_idx = 1;
	assert_ne!(
		our_account_id_idx, unknown_signer_idx,
		"The unknown id must not be our own id or the test is invalid"
	);

	// Create a new ceremony manager with an account id that is in ACCOUNT_IDS
	let mut ceremony_manager = new_ceremony_manager_for_test(
		ACCOUNT_IDS[our_account_id_idx].clone(),
		INITIAL_LATEST_CEREMONY_ID,
	);

	// Replace one of the signers with an unknown id
	let unknown_signer_id = AccountId::new([0; 32]);
	assert!(!ACCOUNT_IDS.contains(&unknown_signer_id));
	let mut participants = ACCOUNT_IDS.clone();
	participants[unknown_signer_idx] = unknown_signer_id;

	// Send a signing request with the modified participants
	let mut result_receiver = run_on_request_to_sign(
		&mut ceremony_manager,
		BTreeSet::from_iter(participants.into_iter()),
		INITIAL_LATEST_CEREMONY_ID + 1,
	)
	.await;

	// Receive the InvalidParticipants error result
	assert_eq!(
		result_receiver.try_recv().expect("Failed to receive ceremony result"),
		Err((BTreeSet::default(), SigningFailureReason::InvalidParticipants,))
	);
}

#[tokio::test]
async fn should_not_create_unauthorized_ceremony_with_invalid_ceremony_id() {
	let latest_ceremony_id = 1; // Invalid, because the CeremonyManager starts with this value as the latest
	let past_ceremony_id = latest_ceremony_id - 1; // Invalid, because it was used in the past
	let future_ceremony_id = latest_ceremony_id + CEREMONY_ID_WINDOW; // Valid, because its within the window
	let future_ceremony_id_too_large = latest_ceremony_id + CEREMONY_ID_WINDOW + 1; // Invalid, because its too far in the future

	// Junk stage 1 data to use for the test
	let stage_1_data = MultisigData::Keygen(KeygenData::HashComm1(client::keygen::HashComm1(
		sp_core::H256::default(),
	)));

	// Create a new ceremony manager and set the latest_ceremony_id
	let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
		ACCOUNT_IDS[0].clone(),
		tokio::sync::mpsc::unbounded_channel().0,
		latest_ceremony_id,
		&new_test_logger(),
	);

	task_scope(|scope| {
		let future: Pin<Box<dyn Future<Output = Result<()>> + Send>> = async {
			// Process a stage 1 message with a ceremony id that is in the past
			ceremony_manager.process_p2p_message(
				ACCOUNT_IDS[0].clone(),
				MultisigMessage { ceremony_id: past_ceremony_id, data: stage_1_data.clone() },
				scope,
			);

			// Process a stage 1 message with a ceremony id that is too far in the future
			ceremony_manager.process_p2p_message(
				ACCOUNT_IDS[0].clone(),
				MultisigMessage {
					ceremony_id: future_ceremony_id_too_large,
					data: stage_1_data.clone(),
				},
				scope,
			);

			// Check that the messages were ignored and no unauthorised ceremonies were created
			assert_eq!(ceremony_manager.keygen_states.ceremony_handles.len(), 0);

			// Process a stage 1 message with a ceremony id that in the future but still within the
			// window
			ceremony_manager.process_p2p_message(
				ACCOUNT_IDS[0].clone(),
				MultisigMessage { ceremony_id: future_ceremony_id, data: stage_1_data },
				scope,
			);

			// Check that the message was not ignored and an unauthorised ceremony was created
			assert_eq!(ceremony_manager.keygen_states.ceremony_handles.len(), 1);

			anyhow::bail!("End the future so we can complete the test");
		}
		.boxed();
		future
	})
	.await
	.unwrap_err();
}

#[tokio::test(start_paused = true)]
async fn should_send_outcome_of_authorised_ceremony() {
	let (ceremony_request_sender, _incoming_p2p_sender, _outgoing_p2p_receiver) =
		spawn_ceremony_manager(ACCOUNT_IDS[0].clone(), INITIAL_LATEST_CEREMONY_ID);

	// Send a signing request in order to create an authorised ceremony
	let mut result_receiver = send_signing_request(
		&ceremony_request_sender,
		BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		INITIAL_LATEST_CEREMONY_ID + 1,
	);

	// Cause a timeout, then check that the correct ceremony outcome was received
	tokio::time::sleep(CEREMONY_TIMEOUT_DURATION).await;
	assert_eq!(
		result_receiver.try_recv().unwrap(),
		Err((
			BTreeSet::default(),
			SigningFailureReason::BroadcastFailure(
				BroadcastFailureReason::InsufficientVerificationMessages,
				SigningStageName::VerifyCommitmentsBroadcast2
			),
		))
	);
}

#[tokio::test]
async fn should_cleanup_unauthorised_ceremony_if_not_participating() {
	task_scope(|scope| {
		async {
			let our_account_id = ACCOUNT_IDS[0].clone();

			// Create a ceremony manager but don't run it yet
			let (_incoming_p2p_sender, incoming_p2p_receiver) =
				tokio::sync::mpsc::unbounded_channel();
			let (ceremony_request_sender, ceremony_request_receiver) =
				tokio::sync::mpsc::unbounded_channel();
			let (outgoing_p2p_sender, _outgoing_p2p_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let mut ceremony_manager = CeremonyManager::<EthSigning>::new(
				our_account_id.clone(),
				outgoing_p2p_sender,
				INITIAL_LATEST_CEREMONY_ID,
				&new_test_logger(),
			);

			// Manually spawn a ceremony runner in an unauthorised state
			let (ceremony_runner_p2p_sender, ceremony_runner_p2p_receiver) =
				mpsc::unbounded_channel();
			let (_ceremony_runner_request_sender, ceremony_runner_request_receiver) =
				oneshot::channel();
			const CEREMONY_ID: CeremonyId = INITIAL_LATEST_CEREMONY_ID + 1;

			let task_handle =
				scope.spawn_with_handle(CeremonyRunner::<SigningCeremony<EthSigning>>::run(
					CEREMONY_ID,
					ceremony_runner_p2p_receiver,
					ceremony_runner_request_receiver,
					mpsc::unbounded_channel().0,
					new_test_logger(),
				));

			// Turn the task handle into a ceremony handle and insert it into the ceremony manager
			let ceremony_handle = CeremonyHandle {
				message_sender: ceremony_runner_p2p_sender.clone(),
				request_state: CeremonyRequestState::Unauthorised(oneshot::channel().0),
				_task_handle: task_handle,
			};
			ceremony_manager
				.signing_states
				.ceremony_handles
				.insert(CEREMONY_ID, ceremony_handle);

			// Start the ceremony manager running
			tokio::spawn(ceremony_manager.run(ceremony_request_receiver, incoming_p2p_receiver));

			// Sanity check that the channel to the ceremony runner task is open
			assert!(!ceremony_runner_p2p_sender.is_closed());

			// Send a signing request that we are not participating in
			// which has the same ceremony id as the unauthorised ceremony
			let mut participants = BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned());
			participants.remove(&our_account_id);

			let _result_receiver =
				send_signing_request(&ceremony_request_sender, participants, CEREMONY_ID);

			// Small delay to let the ceremony manager process the request
			tokio::time::sleep(Duration::from_millis(50)).await;

			// Check that the channel to the ceremony runner task is closed, so the task must have
			// been aborted.
			assert!(ceremony_runner_p2p_sender.is_closed());

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

// Test that the ceremony manager will take an incoming p2p message and give it to the ceremony
// runner. Also checks that the ceremony runner processes incoming messages.
#[tokio::test]
async fn should_route_p2p_message() {
	let our_account_id = ACCOUNT_IDS[0].clone();
	let sender_account_id = ACCOUNT_IDS[1].clone();

	let (ceremony_request_sender, incoming_p2p_sender, mut outgoing_p2p_receiver) =
		spawn_ceremony_manager(our_account_id.clone(), INITIAL_LATEST_CEREMONY_ID);

	// Send a keygen request with only 2 participants, us and one other node.
	// So we will only need to receive one p2p message to complete the stage and advance.
	let ceremony_id = INITIAL_LATEST_CEREMONY_ID + 1;
	let participants = vec![our_account_id, sender_account_id.clone()].into_iter().collect();
	let (result_sender, _result_receiver) = oneshot::channel();
	let request = CeremonyRequest {
		ceremony_id,
		details: Some(CeremonyRequestDetails::Keygen(KeygenRequestDetails::<EthSigning> {
			rng: Rng::from_seed(DEFAULT_KEYGEN_SEED),
			participants,
			result_sender,
		})),
	};

	let _result = ceremony_request_sender.send(request);

	// Small delay to let the ceremony start
	tokio::time::sleep(Duration::from_millis(50)).await;

	// Receive the stage 1 broadcast
	let _stage_1_broadcast = outgoing_p2p_receiver.try_recv().unwrap();

	// Send a stage 1 p2p message
	let message = bincode::serialize(&MultisigMessage {
		ceremony_id,
		data: MultisigData::Keygen(gen_keygen_data_hash_comm1()),
	})
	.unwrap();

	incoming_p2p_sender.send((sender_account_id, message)).unwrap();

	// Small delay to let the ceremony manager process the message
	tokio::time::sleep(Duration::from_millis(50)).await;

	// Check that a broadcast was sent out. Meaning that the ceremony received the message and moved
	// to stage 2.
	assert!(matches!(
		outgoing_p2p_receiver.try_recv().unwrap(),
		OutgoingMultisigStageMessages::Broadcast(..)
	))
}
