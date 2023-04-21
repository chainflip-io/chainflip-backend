use cf_primitives::{AccountId, AuthorityCount};
use rand_legacy::{FromEntropy, SeedableRng};
use std::collections::BTreeSet;

use crate::multisig::{
	client::{
		common::{BroadcastFailureReason, KeygenFailureReason, KeygenStageName, ResharingContext},
		helpers::{
			gen_invalid_keygen_comm1, get_invalid_hash_comm, new_nodes, run_keygen, run_stages,
			standard_signing, KeygenCeremonyRunner, PayloadAndKeyData, SigningCeremonyRunner,
			ACCOUNT_IDS, DEFAULT_KEYGEN_CEREMONY_ID, DEFAULT_KEYGEN_SEED,
			DEFAULT_SIGNING_CEREMONY_ID,
		},
		keygen::{self, Complaints6, VerifyComplaints7, VerifyHashComm2},
		utils::PartyIdxMapping,
	},
	crypto::{bitcoin::BtcSigning, ECPoint, Rng},
	eth::EthSigning,
	CryptoScheme,
};

use crate::multisig::crypto::eth::Point;
type CoeffComm3 = keygen::CoeffComm3<Point>;
type VerifyCoeffComm4 = keygen::VerifyCoeffComm4<Point>;
type SecretShare5 = keygen::SecretShare5<Point>;
type BlameResponse8 = keygen::BlameResponse8<Point>;
type VerifyBlameResponses9 = keygen::VerifyBlameResponses9<Point>;
type KeygenData = keygen::KeygenData<Point>;
pub type KeygenCeremonyRunnerEth = KeygenCeremonyRunner<EthSigning>;

/// If all nodes are honest and behave as expected we should
/// generate a key without entering a blaming stage
#[tokio::test]
async fn happy_path_results_in_valid_key() {
	let (_, _) = run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
}

/// If at least one party is blamed during the "Complaints" stage, we
/// should enter a blaming stage, where the blamed party sends a valid
/// share, so the ceremony should be successful in the end
#[tokio::test]
async fn should_enter_blaming_stage_on_invalid_secret_shares() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let messages = ceremony.request().await;

	let mut messages = run_stages!(
		ceremony,
		messages,
		keygen::VerifyHashComm2,
		CoeffComm3,
		VerifyCoeffComm4,
		SecretShare5
	);

	// One party sends another a bad secret share to cause entering the blaming stage
	let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
	*messages
		.get_mut(bad_share_sender_id)
		.unwrap()
		.get_mut(bad_share_receiver_id)
		.unwrap() = SecretShare5::create_random(&mut ceremony.rng);

	let messages = run_stages!(
		ceremony,
		messages,
		keygen::Complaints6,
		keygen::VerifyComplaints7,
		BlameResponse8,
		VerifyBlameResponses9
	);
	ceremony.distribute_messages(messages).await;
	ceremony.complete().await;
}

#[tokio::test]
async fn should_enter_blaming_stage_on_timeout_secret_shares() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let messages = ceremony.request().await;

	let mut messages = run_stages!(
		ceremony,
		messages,
		keygen::VerifyHashComm2,
		CoeffComm3,
		VerifyCoeffComm4,
		SecretShare5
	);

	// One party fails to send a secret share to another causing everyone to later enter the blaming
	// stage
	let [non_sending_party_id, timed_out_party_id] = &ceremony.select_account_ids();
	messages.get_mut(non_sending_party_id).unwrap().remove(timed_out_party_id);

	ceremony.distribute_messages(messages).await;

	// This node doesn't receive non_sending_party_id's message, so must timeout
	ceremony.get_mut_node(timed_out_party_id).force_stage_timeout().await;

	let messages = ceremony.gather_outgoing_messages::<Complaints6, KeygenData>().await;

	let messages =
		run_stages!(ceremony, messages, VerifyComplaints7, BlameResponse8, VerifyBlameResponses9);
	ceremony.distribute_messages(messages).await;
	ceremony.complete().await;
}

/// If one or more parties send an invalid secret share both the first
/// time and during the blaming stage, the ceremony is aborted with these
/// parties reported
#[tokio::test]
async fn should_report_on_invalid_blame_response6() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();
	let party_idx_mapping =
		PartyIdxMapping::from_participants(BTreeSet::from_iter(ceremony.nodes.keys().cloned()));
	let [bad_node_id_1, bad_node_id_2, target_node_id] = ceremony.select_account_ids();

	let messages = ceremony.request().await;

	let mut messages = run_stages!(
		ceremony,
		messages,
		VerifyHashComm2,
		CoeffComm3,
		VerifyCoeffComm4,
		SecretShare5
	);

	// bad_node_id_1 and bad_node_id_2 send a bad secret share
	*messages.get_mut(&bad_node_id_1).unwrap().get_mut(&target_node_id).unwrap() =
		SecretShare5::create_random(&mut ceremony.rng);

	*messages.get_mut(&bad_node_id_2).unwrap().get_mut(&target_node_id).unwrap() =
		SecretShare5::create_random(&mut ceremony.rng);

	let mut messages =
		run_stages!(ceremony, messages, Complaints6, VerifyComplaints7, BlameResponse8);

	// bad_node_id_1 also sends a bad blame responses, and so gets blamed when ceremony finished
	let secret_share = SecretShare5::create_random(&mut ceremony.rng);
	for message in messages.get_mut(&bad_node_id_1).unwrap().values_mut() {
		*message = keygen::BlameResponse8(
			std::iter::once((
				party_idx_mapping.get_idx(&bad_node_id_2).unwrap(),
				secret_share.clone(),
			))
			.collect(),
		)
	}

	let messages = ceremony.run_stage::<VerifyBlameResponses9, _, _>(messages).await;
	ceremony.distribute_messages(messages).await;
	ceremony
		.complete_with_error(&[bad_node_id_1.clone()], KeygenFailureReason::InvalidBlameResponse)
		.await;
}

/// If party is blamed by one or more peers, its BlameResponse sent in
/// the next stage must be complete, that is, it must contain a (valid)
/// entry for *every* peer it is blamed by. Otherwise the blamed party
/// get reported.
#[tokio::test]
async fn should_report_on_incomplete_blame_response() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let [bad_node_id_1, target_node_id] = ceremony.select_account_ids();

	let messages = ceremony.request().await;

	let mut messages = run_stages!(
		ceremony,
		messages,
		VerifyHashComm2,
		CoeffComm3,
		VerifyCoeffComm4,
		SecretShare5
	);

	// bad_node_id_1 sends a bad secret share
	*messages.get_mut(&bad_node_id_1).unwrap().get_mut(&target_node_id).unwrap() =
		SecretShare5::create_random(&mut ceremony.rng);

	let mut messages =
		run_stages!(ceremony, messages, Complaints6, VerifyComplaints7, BlameResponse8);

	// bad_node_id_1 sends an empty BlameResponse
	for message in messages.get_mut(&bad_node_id_1).unwrap().values_mut() {
		*message = keygen::BlameResponse8::<Point>(std::collections::BTreeMap::default())
	}

	let messages = ceremony.run_stage::<VerifyBlameResponses9, _, _>(messages).await;
	ceremony.distribute_messages(messages).await;
	ceremony
		.complete_with_error(&[bad_node_id_1.clone()], KeygenFailureReason::InvalidBlameResponse)
		.await;
}

// If one of more parties (are thought to) broadcast data inconsistently,
// the ceremony should be aborted and all faulty parties should be reported.
// Fail on `verify_broadcasts` during `VerifyCommitmentsBroadcast2`
#[tokio::test]
async fn should_report_on_inconsistent_broadcast_comm1() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let messages = ceremony.request().await;
	let mut messages = run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

	let [bad_account_id] = &ceremony.select_account_ids();

	// Make one of the nodes send a different commitment to half of the others
	// Note: the bad node must send different comm1 to more than 1/3 of the participants
	let commitment =
		gen_invalid_keygen_comm1(&mut ceremony.rng, ACCOUNT_IDS.len() as AuthorityCount);
	for message in messages.get_mut(bad_account_id).unwrap().values_mut().step_by(2) {
		*message = commitment.clone();
	}

	let messages = ceremony.run_stage::<VerifyCoeffComm4, _, _>(messages).await;
	ceremony.distribute_messages(messages).await;
	ceremony
		.complete_with_error(
			&[bad_account_id.clone()],
			KeygenFailureReason::BroadcastFailure(
				BroadcastFailureReason::Inconsistency,
				KeygenStageName::VerifyCommitmentsBroadcast4,
			),
		)
		.await;
}

#[tokio::test]
async fn should_report_on_inconsistent_broadcast_hash_comm1a() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let mut messages = ceremony.request().await;

	let bad_account_id = &ACCOUNT_IDS[1];

	// Make one of the nodes send a different hash commitment to half of the others
	// Note: the bad node must send different values to more than 1/3 of the participants
	let hash_comm = get_invalid_hash_comm(&mut ceremony.rng);
	for message in messages.get_mut(bad_account_id).unwrap().values_mut().step_by(2) {
		*message = hash_comm.clone();
	}

	let messages = run_stages!(ceremony, messages, VerifyHashComm2,);

	ceremony.distribute_messages(messages).await;
	ceremony
		.complete_with_error(
			&[bad_account_id.clone()],
			KeygenFailureReason::BroadcastFailure(
				BroadcastFailureReason::Inconsistency,
				KeygenStageName::VerifyHashCommitmentsBroadcast2,
			),
		)
		.await;
}

// If one or more parties reveal invalid coefficients that don't correspond
// to the hash commitments sent earlier, the ceremony should be aborted with
// those parties reported.
#[tokio::test]
async fn should_report_on_invalid_hash_comm1a() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let messages = ceremony.request().await;
	let mut messages = run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

	let [bad_account_id] = ceremony.select_account_ids();

	// Make a node send a bad commitment to the others
	// Note: we must send the same bad commitment to all of the nodes,
	// or we will fail on the `inconsistent` error instead of the validation error.
	let corrupted_message = {
		let mut original_message =
			messages.get(&bad_account_id).unwrap().values().next().unwrap().clone();
		original_message.corrupt_secondary_coefficient(&mut ceremony.rng);
		original_message
	};
	for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
		*message = corrupted_message.clone();
	}

	let messages = ceremony.run_stage::<VerifyCoeffComm4, _, _>(messages).await;
	ceremony.distribute_messages(messages).await;

	ceremony
		.complete_with_error(&[bad_account_id], KeygenFailureReason::InvalidCommitment)
		.await;
}

#[tokio::test]
async fn should_report_on_inconsistent_broadcast_complaints4() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let messages = ceremony.request().await;

	let mut messages = run_stages!(
		ceremony,
		messages,
		VerifyHashComm2,
		CoeffComm3,
		VerifyCoeffComm4,
		SecretShare5,
		Complaints6
	);

	let [bad_account_id] = &ceremony.select_account_ids();

	// Make one of the nodes send 2 different complaints evenly to the others
	// Note: the bad node must send different complaints to more than 1/3 of the participants
	for (counter, message) in messages.get_mut(bad_account_id).unwrap().values_mut().enumerate() {
		let counter = counter as AuthorityCount;
		*message = Complaints6(BTreeSet::from_iter(
			counter % 2..((counter % 2) + ACCOUNT_IDS.len() as AuthorityCount),
		));
	}

	let messages = ceremony.run_stage::<keygen::VerifyComplaints7, _, _>(messages).await;
	ceremony.distribute_messages(messages).await;
	ceremony
		.complete_with_error(
			&[bad_account_id.clone()],
			KeygenFailureReason::BroadcastFailure(
				BroadcastFailureReason::Inconsistency,
				KeygenStageName::VerifyComplaintsBroadcastStage7,
			),
		)
		.await;
}

#[tokio::test]
async fn should_report_on_inconsistent_broadcast_blame_responses6() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let party_idx_mapping =
		PartyIdxMapping::from_participants(BTreeSet::from_iter(ceremony.nodes.keys().cloned()));

	let messages = ceremony.request().await;

	let mut messages = run_stages!(
		ceremony,
		messages,
		VerifyHashComm2,
		CoeffComm3,
		VerifyCoeffComm4,
		SecretShare5
	);

	let [bad_node_id, blamed_node_id] = &ceremony.select_account_ids();

	// One party sends another a bad secret share to cause entering the blaming stage
	let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
	*messages
		.get_mut(bad_share_sender_id)
		.unwrap()
		.get_mut(bad_share_receiver_id)
		.unwrap() = SecretShare5::create_random(&mut ceremony.rng);

	let mut messages =
		run_stages!(ceremony, messages, Complaints6, VerifyComplaints7, BlameResponse8);

	let [bad_account_id] = &ceremony.select_account_ids();

	// Make one of the nodes send 2 different blame responses evenly to the others
	// Note: the bad node must send different blame response to more than 1/3 of the participants
	let secret_share = SecretShare5::create_random(&mut ceremony.rng);
	for message in messages.get_mut(bad_node_id).unwrap().values_mut().step_by(2) {
		*message = keygen::BlameResponse8::<Point>(
			std::iter::once((
				party_idx_mapping.get_idx(blamed_node_id).unwrap(),
				secret_share.clone(),
			))
			.collect(),
		)
	}

	let messages = ceremony.run_stage::<VerifyBlameResponses9, _, _>(messages).await;
	ceremony.distribute_messages(messages).await;
	ceremony
		.complete_with_error(
			&[bad_account_id.clone()],
			KeygenFailureReason::BroadcastFailure(
				BroadcastFailureReason::Inconsistency,
				KeygenStageName::VerifyBlameResponsesBroadcastStage9,
			),
		)
		.await;
}

// If one or more parties send invalid commitments, the ceremony should be aborted.
// Fail on `validate_commitments` during `VerifyCommitmentsBroadcast2`.
#[tokio::test]
async fn should_report_on_invalid_comm1() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let messages = ceremony.request().await;
	let mut messages = run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

	let [bad_account_id] = ceremony.select_account_ids();

	// Make a node send a bad commitment to the others
	// Note: we must send the same bad commitment to all of the nodes,
	// or we will fail on the `inconsistent` error instead of the validation error.
	let corrupted_message = {
		let mut original_message =
			messages.get(&bad_account_id).unwrap().values().next().unwrap().clone();
		original_message.corrupt_primary_coefficient(&mut ceremony.rng);
		original_message
	};
	for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
		*message = corrupted_message.clone();
	}

	let messages = ceremony.run_stage::<VerifyCoeffComm4, _, _>(messages).await;
	ceremony.distribute_messages(messages).await;

	ceremony
		.complete_with_error(&[bad_account_id], KeygenFailureReason::InvalidCommitment)
		.await;
}

#[tokio::test]
async fn should_report_on_invalid_complaints4() {
	let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

	let messages = ceremony.request().await;

	let mut messages = run_stages!(
		ceremony,
		messages,
		VerifyHashComm2,
		CoeffComm3,
		VerifyCoeffComm4,
		SecretShare5,
		Complaints6
	);

	let [bad_account_id] = ceremony.select_account_ids();

	// This complaint is invalid because it has an invalid index
	let invalid_complaint: Complaints6 = keygen::Complaints6([1, u32::MAX].into_iter().collect());

	for message in messages.get_mut(&bad_account_id).unwrap().values_mut() {
		*message = invalid_complaint.clone();
	}

	let messages = ceremony.run_stage::<keygen::VerifyComplaints7, _, _>(messages).await;
	ceremony.distribute_messages(messages).await;
	ceremony
		.complete_with_error(&[bad_account_id], KeygenFailureReason::InvalidComplaint)
		.await;
}

mod timeout {

	use super::*;

	mod during_regular_stage {

		use super::*;

		#[tokio::test]
		async fn should_recover_if_party_appears_offline_to_minority_stage1a() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let mut messages = ceremony.request().await;

			let [non_sending_party_id, timed_out_party_id] = ceremony.select_account_ids();

			messages.get_mut(&non_sending_party_id).unwrap().remove(&timed_out_party_id);

			ceremony.distribute_messages(messages).await;

			// This node doesn't receive non_sending_party's message, so must timeout
			ceremony.get_mut_node(&timed_out_party_id).force_stage_timeout().await;

			let messages = ceremony.gather_outgoing_messages::<VerifyHashComm2, KeygenData>().await;

			let messages = run_stages!(
				ceremony,
				messages,
				CoeffComm3,
				VerifyCoeffComm4,
				SecretShare5,
				Complaints6,
				VerifyComplaints7
			);
			ceremony.distribute_messages(messages).await;
			ceremony.complete().await;
		}

		#[tokio::test]
		async fn should_recover_if_party_appears_offline_to_minority_stage1() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let mut messages = run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

			let [non_sending_party_id, timed_out_party_id] = ceremony.select_account_ids();

			messages.get_mut(&non_sending_party_id).unwrap().remove(&timed_out_party_id);

			ceremony.distribute_messages(messages).await;

			// This node doesn't receive non_sending_party's message, so must timeout
			ceremony.get_mut_node(&timed_out_party_id).force_stage_timeout().await;

			let messages =
				ceremony.gather_outgoing_messages::<VerifyCoeffComm4, KeygenData>().await;

			let messages =
				run_stages!(ceremony, messages, SecretShare5, Complaints6, VerifyComplaints7);
			ceremony.distribute_messages(messages).await;
			ceremony.complete().await;
		}

		#[tokio::test]
		async fn should_recover_if_party_appears_offline_to_minority_stage4() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let mut messages = run_stages!(
				ceremony,
				messages,
				VerifyHashComm2,
				CoeffComm3,
				VerifyCoeffComm4,
				SecretShare5,
				Complaints6
			);

			let [non_sending_party_id, timed_out_party_id] = ceremony.select_account_ids();

			messages.get_mut(&non_sending_party_id).unwrap().remove(&timed_out_party_id);

			ceremony.distribute_messages(messages).await;

			// This node doesn't receive non_sending_party's message, so must timeout
			ceremony.get_mut_node(&timed_out_party_id).force_stage_timeout().await;

			let messages =
				ceremony.gather_outgoing_messages::<VerifyComplaints7, KeygenData>().await;

			ceremony.distribute_messages(messages).await;
			ceremony.complete().await;
		}

		#[tokio::test]
		async fn should_recover_if_party_appears_offline_to_minority_stage6() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let mut messages = run_stages!(
				ceremony,
				messages,
				VerifyHashComm2,
				CoeffComm3,
				VerifyCoeffComm4,
				SecretShare5
			);

			// One party sends another a bad secret share to cause entering the blaming stage
			let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
			*messages
				.get_mut(bad_share_sender_id)
				.unwrap()
				.get_mut(bad_share_receiver_id)
				.unwrap() = SecretShare5::create_random(&mut ceremony.rng);

			let [non_sending_party_id, timed_out_party_id] = ceremony.select_account_ids();

			let mut messages =
				run_stages!(ceremony, messages, Complaints6, VerifyComplaints7, BlameResponse8);

			messages.get_mut(&non_sending_party_id).unwrap().remove(&timed_out_party_id);

			ceremony.distribute_messages(messages).await;

			// This node doesn't receive non_sending_party's message, so must timeout
			ceremony.get_mut_node(&timed_out_party_id).force_stage_timeout().await;

			let messages =
				ceremony.gather_outgoing_messages::<VerifyBlameResponses9, KeygenData>().await;

			ceremony.distribute_messages(messages).await;
			ceremony.complete().await;
		}
	}

	mod during_broadcast_verification_stage {

		use super::*;

		#[tokio::test]
		async fn should_recover_if_agree_on_values_stage2a() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let messages = run_stages!(ceremony, messages, VerifyHashComm2,);

			let [non_sender_id] = &ceremony.select_account_ids();
			let messages = ceremony
				.run_stage_with_non_sender::<CoeffComm3, _, _>(messages, non_sender_id)
				.await;

			let messages = run_stages!(
				ceremony,
				messages,
				VerifyCoeffComm4,
				SecretShare5,
				Complaints6,
				VerifyComplaints7
			);

			ceremony.distribute_messages(messages).await;
			ceremony.complete().await;
		}

		#[tokio::test]
		async fn should_recover_if_agree_on_values_stage2() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let messages =
				run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3, VerifyCoeffComm4);

			let [non_sender_id] = &ceremony.select_account_ids();
			let messages = ceremony
				.run_stage_with_non_sender::<SecretShare5, _, _>(messages, non_sender_id)
				.await;

			let messages = run_stages!(ceremony, messages, Complaints6, VerifyComplaints7);

			ceremony.distribute_messages(messages).await;
			ceremony.complete().await;
		}

		#[tokio::test]
		async fn should_recover_if_agree_on_values_stage5() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let messages = run_stages!(
				ceremony,
				messages,
				VerifyHashComm2,
				CoeffComm3,
				VerifyCoeffComm4,
				SecretShare5,
				Complaints6,
				VerifyComplaints7
			);

			let [non_sender_id] = ceremony.select_account_ids();
			ceremony.distribute_messages_with_non_sender(messages, &non_sender_id).await;

			ceremony.complete().await;
		}

		#[tokio::test]
		async fn should_recover_if_agree_on_values_stage7() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let mut messages = run_stages!(
				ceremony,
				messages,
				VerifyHashComm2,
				CoeffComm3,
				VerifyCoeffComm4,
				SecretShare5
			);

			// One party sends another a bad secret share to cause entering the blaming stage
			let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
			*messages
				.get_mut(bad_share_sender_id)
				.unwrap()
				.get_mut(bad_share_receiver_id)
				.unwrap() = SecretShare5::create_random(&mut ceremony.rng);

			let messages = run_stages!(
				ceremony,
				messages,
				Complaints6,
				VerifyComplaints7,
				BlameResponse8,
				VerifyBlameResponses9
			);

			let [non_sender_id] = ceremony.select_account_ids();
			ceremony.distribute_messages_with_non_sender(messages, &non_sender_id).await;

			ceremony.complete().await;
		}

		#[tokio::test]
		async fn should_report_if_insufficient_messages_stage2a() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let [non_sending_party_id_1, non_sending_party_id_2] = ceremony.select_account_ids();

			// bad party 1 times out during a broadcast stage. It should be reported
			let messages = ceremony
				.run_stage_with_non_sender::<VerifyHashComm2, _, _>(
					messages,
					&non_sending_party_id_1,
				)
				.await;

			// bad party 2 times out during a broadcast verification stage. It won't get reported.
			ceremony
				.distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
				.await;

			ceremony
				.complete_with_error(
					&[non_sending_party_id_1],
					KeygenFailureReason::BroadcastFailure(
						BroadcastFailureReason::InsufficientMessages,
						KeygenStageName::VerifyHashCommitmentsBroadcast2,
					),
				)
				.await
		}

		#[tokio::test]
		async fn should_report_if_insufficient_messages_stage2() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let [non_sending_party_id_1, non_sending_party_id_2] = ceremony.select_account_ids();

			let messages = run_stages!(ceremony, messages, VerifyHashComm2, CoeffComm3);

			// bad party 1 times out during a broadcast stage. It should be reported
			let messages = ceremony
				.run_stage_with_non_sender::<VerifyCoeffComm4, _, _>(
					messages,
					&non_sending_party_id_1,
				)
				.await;

			// bad party 2 times out during a broadcast verification stage. It won't get reported.
			ceremony
				.distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
				.await;

			ceremony
				.complete_with_error(
					&[non_sending_party_id_1],
					KeygenFailureReason::BroadcastFailure(
						BroadcastFailureReason::InsufficientMessages,
						KeygenStageName::VerifyCommitmentsBroadcast4,
					),
				)
				.await
		}

		#[tokio::test]
		async fn should_report_if_insufficient_messages_stage5() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let messages = run_stages!(
				ceremony,
				messages,
				VerifyHashComm2,
				CoeffComm3,
				VerifyCoeffComm4,
				SecretShare5,
				Complaints6
			);

			let [non_sending_party_id_1, non_sending_party_id_2] = ceremony.select_account_ids();

			// bad party 1 times out during a broadcast stage. It should be reported
			let messages = ceremony
				.run_stage_with_non_sender::<VerifyComplaints7, _, _>(
					messages,
					&non_sending_party_id_1,
				)
				.await;

			// bad party 2 times out during a broadcast verification stage. It won't get reported.
			ceremony
				.distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
				.await;

			ceremony
				.complete_with_error(
					&[non_sending_party_id_1],
					KeygenFailureReason::BroadcastFailure(
						BroadcastFailureReason::InsufficientMessages,
						KeygenStageName::VerifyComplaintsBroadcastStage7,
					),
				)
				.await
		}

		#[tokio::test]
		async fn should_report_if_insufficient_messages_stage7() {
			let mut ceremony = KeygenCeremonyRunnerEth::new_with_default();

			let messages = ceremony.request().await;

			let mut messages = run_stages!(
				ceremony,
				messages,
				VerifyHashComm2,
				CoeffComm3,
				VerifyCoeffComm4,
				SecretShare5
			);

			// One party sends another a bad secret share to cause entering the blaming stage
			let [bad_share_sender_id, bad_share_receiver_id] = &ceremony.select_account_ids();
			*messages
				.get_mut(bad_share_sender_id)
				.unwrap()
				.get_mut(bad_share_receiver_id)
				.unwrap() = SecretShare5::create_random(&mut ceremony.rng);

			let messages =
				run_stages!(ceremony, messages, Complaints6, VerifyComplaints7, BlameResponse8);

			let [non_sending_party_id_1, non_sending_party_id_2] = ceremony.select_account_ids();

			// bad party 1 times out during a broadcast stage. It should be reported
			let messages = ceremony
				.run_stage_with_non_sender::<VerifyBlameResponses9, _, _>(
					messages,
					&non_sending_party_id_1,
				)
				.await;

			// bad party 2 times out during a broadcast verification stage. It won't get reported.
			ceremony
				.distribute_messages_with_non_sender(messages, &non_sending_party_id_2)
				.await;

			ceremony
				.complete_with_error(
					&[non_sending_party_id_1],
					KeygenFailureReason::BroadcastFailure(
						BroadcastFailureReason::InsufficientMessages,
						KeygenStageName::VerifyBlameResponsesBroadcastStage9,
					),
				)
				.await
		}
	}
}

#[tokio::test]
async fn genesis_keys_can_sign() {
	use crate::multisig::crypto::eth::EthSigning;

	let account_ids: BTreeSet<_> = [1, 2, 3, 4].iter().map(|i| AccountId::new([*i; 32])).collect();

	let mut rng = Rng::from_entropy();
	let (public_key_bytes, key_data) =
		keygen::generate_key_data::<EthSigning>(account_ids.clone(), &mut rng);

	let (mut signing_ceremony, _non_signing_nodes) =
		SigningCeremonyRunner::<EthSigning>::new_with_threshold_subset_of_signers(
			new_nodes(account_ids),
			DEFAULT_SIGNING_CEREMONY_ID,
			vec![PayloadAndKeyData::new(
				EthSigning::signing_payload_for_test(),
				public_key_bytes,
				key_data,
			)],
			Rng::from_entropy(),
		);
	standard_signing(&mut signing_ceremony).await;
}

#[tokio::test]
// Test that a key that was initially incompatible with Eth contract
// was correctly turned into a compatible one, which can be used for
// multiparty signing.
async fn initially_incompatible_keys_can_sign() {
	use crate::multisig::crypto::eth::EthSigning;

	let account_ids: BTreeSet<_> = [1, 2, 3, 4].iter().map(|i| AccountId::new([*i; 32])).collect();

	let mut rng = Rng::from_entropy();
	let (public_key_bytes, key_data) =
		keygen::generate_key_data_with_initial_incompatibility(account_ids.clone(), &mut rng);

	let (mut signing_ceremony, _non_signing_nodes) =
		SigningCeremonyRunner::<EthSigning>::new_with_threshold_subset_of_signers(
			new_nodes(account_ids),
			DEFAULT_SIGNING_CEREMONY_ID,
			vec![PayloadAndKeyData::new(
				EthSigning::signing_payload_for_test(),
				public_key_bytes,
				key_data,
			)],
			Rng::from_entropy(),
		);
	standard_signing(&mut signing_ceremony).await;
}

mod key_handover {

	use crate::multisig::client::helpers::PayloadAndKeyData;

	use super::*;

	#[tokio::test]
	async fn key_handover() {
		// The high level idea of this test is to generate some key with some
		// nodes, then introduce new nodes who the key will be handed over to.
		// There is an overlap between the two sets of nodes, which is going
		// to be common in practice. The resulting aggregate keys should match.

		type Scheme = BtcSigning;
		type Point = <Scheme as CryptoScheme>::Point;
		type Scalar = <Point as ECPoint>::Scalar;

		let all_account_ids: Vec<AccountId> =
			[1, 2, 3, 4, 5].iter().map(|i| AccountId::new([*i; 32])).collect();

		// Accounts (1), (2) and (3) will hold the original key
		let original_set: BTreeSet<_> = all_account_ids.iter().take(3).cloned().collect();

		// Accounts (3), (4) and (5) will receive the key as the result of this ceremony.
		// (Note that (3) appears in both sets.)
		let new_set: BTreeSet<_> = all_account_ids.iter().skip(2).take(3).cloned().collect();

		// Perform a regular keygen to generate initial keys:
		let (initial_key, mut key_infos) = keygen::generate_key_data::<Scheme>(
			original_set.clone().into_iter().collect(),
			&mut Rng::from_seed(DEFAULT_KEYGEN_SEED),
		);

		// Only 2 and 3 will contribute their secret shares
		let sharing_participants: BTreeSet<AccountId> =
			original_set.clone().into_iter().skip(1).collect();

		// Sanity check: we have (just) enough participants to re-share the key
		assert_eq!(
			key_infos.values().next().as_ref().unwrap().params.threshold + 1,
			sharing_participants.len() as u32
		);

		let receiving_participants: BTreeSet<AccountId> = new_set.clone().into_iter().collect();
		// Accounts (2), (3), (4) and (5) will participate, with (2) and (3)
		// re-sharing their key to (3), (4) and (5)
		let all_participants: BTreeSet<_> =
			sharing_participants.union(&receiving_participants).cloned().collect();

		let mut ceremony = KeygenCeremonyRunner::<Scheme>::new(
			new_nodes(all_participants),
			DEFAULT_KEYGEN_CEREMONY_ID,
			Rng::from_seed(DEFAULT_KEYGEN_SEED),
		);

		let ceremony_details = ceremony.keygen_ceremony_details();

		for (id, node) in &mut ceremony.nodes {
			// Give the right context type depending on whether they have keys
			let context = if sharing_participants.contains(id) {
				let key_info = key_infos.remove(id).unwrap();
				ResharingContext::from_key(
					&key_info,
					id,
					&sharing_participants,
					&receiving_participants,
				)
			} else {
				ResharingContext::without_key(&sharing_participants, &receiving_participants)
			};
			node.request_key_handover(ceremony_details.clone(), context).await;
		}

		let messages = ceremony.gather_outgoing_messages::<keygen::PubkeyShares0<Point>, _>().await;

		let messages = run_stages!(
			ceremony,
			messages,
			keygen::HashComm1,
			keygen::VerifyHashComm2,
			CoeffComm3,
			VerifyCoeffComm4,
			SecretShare5,
			Complaints6,
			VerifyComplaints7
		);

		ceremony.distribute_messages(messages).await;
		let (new_key, new_shares) = ceremony.complete().await;

		assert_eq!(new_key, initial_key);

		// Ensure that the new key shares can be used for signing:
		let mut signing_ceremony = SigningCeremonyRunner::<Scheme>::new_with_all_signers(
			new_nodes(receiving_participants),
			DEFAULT_SIGNING_CEREMONY_ID,
			vec![PayloadAndKeyData::new(Scheme::signing_payload_for_test(), new_key, new_shares)],
			Rng::from_entropy(),
		);
		standard_signing(&mut signing_ceremony).await;
	}

	// Test that a party who doesn't perform re-sharing correctly
	// (commits to an unexpected secret) gets reported
	#[tokio::test]
	async fn key_handover_with_incorrect_commitment() {
		use crate::multisig::client::common::ParticipantStatus;
		type Scheme = BtcSigning;
		type Point = <Scheme as CryptoScheme>::Point;
		type Scalar = <Point as ECPoint>::Scalar;

		let all_account_ids: Vec<AccountId> =
			[1, 2, 3, 4, 5].iter().map(|i| AccountId::new([*i; 32])).collect();

		// Accounts (1), (2) and (3) will hold the original key
		let original_set: BTreeSet<_> = all_account_ids.iter().take(3).cloned().collect();

		// Accounts (3), (4) and (5) will receive the key as the result of this ceremony.
		// (Note that (3) appears in both sets.)
		let new_set: BTreeSet<_> = all_account_ids.iter().skip(2).take(3).cloned().collect();

		// Perform a regular keygen to generate initial keys:
		let (_initial_key, mut key_infos) = keygen::generate_key_data::<Scheme>(
			original_set.clone().into_iter().collect(),
			&mut Rng::from_seed(DEFAULT_KEYGEN_SEED),
		);

		// Only 2 and 3 will contribute their secret shares
		let sharing_participants: BTreeSet<AccountId> =
			original_set.clone().into_iter().skip(1).collect();

		// Sanity check: we have (just) enough participants to re-share the key
		assert_eq!(
			key_infos.values().next().as_ref().unwrap().params.threshold + 1,
			sharing_participants.len() as u32
		);

		let receiving_participants: BTreeSet<AccountId> = new_set.clone().into_iter().collect();
		// Accounts (2), (3), (4) and (5) will participate, with (2) and (3)
		// re-sharing their key to (3), (4) and (5)
		let all_participants: BTreeSet<_> =
			sharing_participants.union(&receiving_participants).cloned().collect();

		// Now perform a key hand-over ceremony where one of the participants
		// commits to an unexpected secret

		// This account id will commit to an unexpected secret
		let bad_account_id = all_participants.iter().next().unwrap().clone();

		let mut ceremony = KeygenCeremonyRunner::<Scheme>::new(
			new_nodes(all_participants),
			DEFAULT_KEYGEN_CEREMONY_ID,
			Rng::from_seed(DEFAULT_KEYGEN_SEED),
		);

		let ceremony_details = ceremony.keygen_ceremony_details();

		for (id, node) in &mut ceremony.nodes {
			// Give the right context type depending on whether they have keys
			let mut context = if sharing_participants.contains(id) {
				let key_info = key_infos.remove(id).unwrap();
				ResharingContext::from_key(
					&key_info,
					id,
					&sharing_participants,
					&receiving_participants,
				)
			} else {
				ResharingContext::without_key(&sharing_participants, &receiving_participants)
			};

			if id == &bad_account_id {
				// Adding a small tweak to the share to make it incorrect
				match &mut context.party_status {
					ParticipantStatus::Sharing(secret_share, _) => {
						*secret_share = &*secret_share + &Scalar::from(1);
					},
					_ => panic!("Unexpected status"),
				}
			}

			node.request_key_handover(ceremony_details.clone(), context).await;
		}

		let messages = ceremony.gather_outgoing_messages::<keygen::PubkeyShares0<Point>, _>().await;

		let messages = run_stages!(
			ceremony,
			messages,
			keygen::HashComm1,
			keygen::VerifyHashComm2,
			CoeffComm3,
			VerifyCoeffComm4
		);

		ceremony.distribute_messages(messages).await;

		ceremony
			.complete_with_error(&[bad_account_id], KeygenFailureReason::InvalidCommitment)
			.await;
	}
}
