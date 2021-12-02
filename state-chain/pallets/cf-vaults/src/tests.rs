use crate::{
	mock::*, BlockHeightWindow, Error, Event as PalletEvent, PendingVaultRotations, Vault,
	VaultRotationStatus, Vaults,
};
use cf_chains::ChainId;
use cf_traits::{Chainflip, EpochInfo, VaultRotator};
use frame_support::{assert_noop, assert_ok};

fn last_event() -> Event {
	frame_system::Pallet::<MockRuntime>::events()
		.pop()
		.expect("Event expected")
		.event
}

const ALL_CANDIDATES: &[<MockRuntime as Chainflip>::ValidatorId] = &[ALICE, BOB, CHARLIE];

#[test]
fn no_candidates_is_noop_and_error() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			VaultsPallet::start_vault_rotation(vec![]),
			Error::<MockRuntime>::EmptyValidatorSet
		);
		assert!(VaultsPallet::no_active_chain_vault_rotations());
	});
}

#[test]
fn keygen_request_emitted() {
	new_test_ext().execute_with(|| {
		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		// Confirm we have a new vault rotation process running
		assert!(!VaultsPallet::no_active_chain_vault_rotations());
		// Check the event emitted
		assert_eq!(
			last_event(),
			PalletEvent::<MockRuntime>::KeygenRequest(
				VaultsPallet::keygen_ceremony_id_counter(),
				ChainId::Ethereum,
				ALL_CANDIDATES.to_vec(),
			)
			.into()
		);
	});
}

#[test]
fn only_one_concurrent_request_per_chain() {
	new_test_ext().execute_with(|| {
		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		assert_noop!(
			VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()),
			Error::<MockRuntime>::DuplicateRotationRequest
		);
	});
}

#[test]
fn keygen_success() {
	new_test_ext().execute_with(|| {
		let new_public_key: Vec<u8> = GENESIS_ETHEREUM_AGG_PUB_KEY.iter().map(|x| x + 1).collect();

		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = VaultsPallet::keygen_ceremony_id_counter();

		assert_ok!(VaultsPallet::on_keygen_success(
			ceremony_id,
			ChainId::Ethereum,
			new_public_key.clone()
		));

		// Can't be reported twice
		// assert_noop!(
		// 	VaultsPallet::on_keygen_success(
		// 		ceremony_id,
		// 		ChainId::Ethereum,
		// 		new_public_key.clone()
		// 	),
		// 	Error::<MockRuntime>::InvalidRotationStatus
		// );

		// Can't change our mind
		// assert_noop!(
		// 	VaultsPallet::on_keygen_failure(
		// 		ceremony_id,
		// 		ChainId::Ethereum,
		// 		vec![]
		// 	),
		// 	Error::<MockRuntime>::InvalidRotationStatus
		// );
	});
}

#[test]
fn keygen_failure() {
	new_test_ext().execute_with(|| {
		const BAD_CANDIDATES: &'static [<MockRuntime as Chainflip>::ValidatorId] = &[BOB, CHARLIE];

		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));

		let ceremony_id = VaultsPallet::keygen_ceremony_id_counter();

		// The ceremony failed.
		VaultsPallet::on_keygen_failure(ceremony_id, ChainId::Ethereum, BAD_CANDIDATES.to_vec());

		// KeygenAborted event emitted.
		assert_eq!(last_event(), PalletEvent::KeygenFailure(ceremony_id, ChainId::Ethereum).into());

		// All rotations have been aborted.
		assert!(VaultsPallet::no_active_chain_vault_rotations());

		// Bad validators have been reported.
		assert_eq!(MockOfflineReporter::get_reported(), BAD_CANDIDATES);

		// Can't be reported twice
		// assert_noop!(
		// 	VaultsPallet::on_keygen_failure(
		// 		ceremony_id,
		// 		ChainId::Ethereum,
		// 		vec![]
		// 	),
		// 	Error::<MockRuntime>::NoActiveRotation
		// );

		// Can't change our mind
		// assert_noop!(
		// 	VaultsPallet::on_keygen_success(
		// 		ceremony_id,
		// 		ChainId::Ethereum,
		// 		vec![]
		// 	),
		// 	Error::<MockRuntime>::NoActiveRotation
		// );
	});
}

#[test]
fn vault_key_rotated() {
	new_test_ext().execute_with(|| {
		const ROTATION_BLOCK_NUMBER: u64 = 42;
		const TX_HASH: [u8; 32] = [0xab; 32];
		let new_public_key: Vec<u8> = GENESIS_ETHEREUM_AGG_PUB_KEY.iter().map(|x| x + 1).collect();

		assert_noop!(
			VaultsPallet::vault_key_rotated(
				Origin::root(),
				ChainId::Ethereum,
				new_public_key.clone(),
				ROTATION_BLOCK_NUMBER,
				TX_HASH.to_vec(),
			),
			Error::<MockRuntime>::NoActiveRotation
		);

		assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
		let ceremony_id = VaultsPallet::keygen_ceremony_id_counter();
		assert_ok!(VaultsPallet::on_keygen_success(
			ceremony_id,
			ChainId::Ethereum,
			new_public_key.clone()
		));

		assert_ok!(VaultsPallet::vault_key_rotated(
			Origin::root(),
			ChainId::Ethereum,
			new_public_key.clone(),
			ROTATION_BLOCK_NUMBER,
			TX_HASH.to_vec(),
		));

		// Can't repeat.
		assert_noop!(
			VaultsPallet::vault_key_rotated(
				Origin::root(),
				ChainId::Ethereum,
				new_public_key.clone(),
				ROTATION_BLOCK_NUMBER,
				TX_HASH.to_vec(),
			),
			Error::<MockRuntime>::InvalidRotationStatus
		);

		// We have yet to move to the new epoch
		let current_epoch = <MockRuntime as Chainflip>::EpochInfo::epoch_index();

		let Vault { public_key, active_window } =
			Vaults::<MockRuntime>::get(current_epoch, ChainId::Ethereum)
				.expect("Ethereum Vault should exist");

		assert_eq!(
			public_key, GENESIS_ETHEREUM_AGG_PUB_KEY,
			"we should have the old agg key in the genesis vault"
		);

		assert_eq!(
			active_window,
			BlockHeightWindow { from: 0, to: Some(ROTATION_BLOCK_NUMBER) },
			"we should have the block height set for the genesis or current epoch"
		);

		// The next epoch
		let next_epoch = current_epoch + 1;

		let Vault { public_key, active_window } =
			Vaults::<MockRuntime>::get(next_epoch, ChainId::Ethereum)
				.expect("Ethereum Vault should exist in the next epoch");

		assert_eq!(
			public_key, new_public_key,
			"we should have the new public key in the new vault for the next epoch"
		);

		assert_eq!(
			active_window,
			BlockHeightWindow { from: ROTATION_BLOCK_NUMBER.saturating_add(1), to: None },
			"we should have set the starting point for the new vault's active window as the next
			after the reported block number"
		);

		// Status is complete.
		assert_eq!(
			PendingVaultRotations::<MockRuntime>::get(ChainId::Ethereum),
			Some(VaultRotationStatus::Complete { tx_hash: TX_HASH.to_vec() }),
		);
	});
}

mod keygen_reporting {
	use super::*;
	use crate::{KeygenOutcome, KeygenResponseStatus};
	use frame_support::assert_err;
	use sp_std::{collections::btree_set::BTreeSet, iter::FromIterator};

	const TEST_KEY: &[u8; 9] = b"chainflip";

	macro_rules! assert_ok_no_repeat {
		($ex:expr) => {
			assert_ok!($ex);
			assert_err!($ex, Error::<MockRuntime>::InvalidRespondent);
		};
	}

	macro_rules! assert_success_outcome {
		($ex:expr) => {
			let outcome: Option<KeygenOutcome<Vec<u8>, u64>> = $ex;
			assert!(matches!(outcome, Some(KeygenOutcome::Success(_))));
		};
	}

	macro_rules! assert_failure_outcome {
		($ex:expr) => {
			let outcome: Option<KeygenOutcome<Vec<u8>, u64>> = $ex;
			assert!(matches!(outcome, Some(KeygenOutcome::Failure(_))));
		};
	}

	macro_rules! assert_no_outcome {
		($ex:expr) => {
			let outcome: Option<KeygenOutcome<Vec<u8>, u64>> = $ex;
			assert!(matches!(outcome, None));
		};
	}

	#[test]
	fn test_threshold() {
		// The threshold is the smallest number of participants that *can* reach consensus.
		assert_eq!(
			KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(0..144)).threshold(),
			96
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(0..145)).threshold(),
			97
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(0..146)).threshold(),
			98
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(0..147)).threshold(),
			98
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(0..148)).threshold(),
			99
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(0..149)).threshold(),
			100
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(0..150)).threshold(),
			100
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(0..151)).threshold(),
			101
		);
	}

	fn simple_success(
		num_candidates: u32,
		num_successes: u32,
	) -> Option<KeygenOutcome<Vec<u8>, u64>> {
		get_outcome(num_candidates, num_successes, 0, 0, None)
	}

	fn simple_failure(
		num_candidates: u32,
		num_failures: u32,
	) -> Option<KeygenOutcome<Vec<u8>, u64>> {
		get_outcome(num_candidates, 0, num_failures, 0, None)
	}

	fn get_outcome(
		num_candidates: u32,
		mut num_successes: u32,
		mut num_failures: u32,
		mut num_bad_keys: u32,
		report_one_in: Option<u64>,
	) -> Option<KeygenOutcome<Vec<u8>, u64>> {
		let key = TEST_KEY.to_vec();
		let mut status = KeygenResponseStatus::<MockRuntime>::new(BTreeSet::from_iter(
			1..=(num_candidates as u64),
		));

		let num_responses = num_successes + num_failures + num_bad_keys;
		assert!(
			num_responses <= num_candidates,
			"Can't have more responses than candidates: {} + {} + {} > {}.",
			num_successes,
			num_failures,
			num_bad_keys,
			num_candidates
		);

		for id in 1..=(num_responses as u64) {
			if num_failures > 0 {
				assert_ok_no_repeat!(status.add_failure_vote(
					&id,
					// Report one in 5 unless specified otherwise.
					BTreeSet::from_iter(
						(1..=(num_candidates as u64))
							.filter(|id| (id % report_one_in.unwrap_or(5) == 1))
					)
				));
				num_failures -= 1;
			} else if num_bad_keys > 0 {
				assert_ok_no_repeat!(status.add_success_vote(&id, b"wrong".to_vec()));
				num_bad_keys -= 1;
			} else if num_successes > 0 {
				assert_ok_no_repeat!(status.add_success_vote(&id, key.clone()));
				num_successes -= 1;
			} else {
				panic!("Should not reach here.")
			}
		}
		status.consensus_outcome()
	}

	#[test]
	fn test_success_consensus() {
		// Simple happy-path cases.
		assert_success_outcome!(simple_success(6, 6));
		assert_success_outcome!(simple_success(6, 5));
		assert_success_outcome!(simple_success(6, 4));
		assert_success_outcome!(simple_success(7, 7));
		assert_success_outcome!(simple_success(8, 8));
		assert_success_outcome!(simple_success(9, 9));

		assert_success_outcome!(simple_success(147, 147));
		assert_success_outcome!(simple_success(148, 148));
		assert_success_outcome!(simple_success(149, 149));
		assert_success_outcome!(simple_success(150, 150));
		assert_success_outcome!(simple_success(151, 151));

		// Minority dissent has no effect.
		assert_success_outcome!(get_outcome(6, 5, 1, 0, None));
		assert_success_outcome!(get_outcome(6, 4, 1, 1, None));
		assert_success_outcome!(get_outcome(6, 4, 2, 0, None));
	}

	#[test]
	fn test_failure_consensus() {
		// Simple happy-path cases.
		assert_failure_outcome!(simple_failure(6, 6));
		assert_failure_outcome!(simple_failure(6, 5));
		assert_failure_outcome!(simple_failure(6, 4));
		assert_failure_outcome!(simple_failure(7, 7));
		assert_failure_outcome!(simple_failure(8, 8));
		assert_failure_outcome!(simple_failure(9, 9));

		assert_failure_outcome!(simple_failure(147, 147));
		assert_failure_outcome!(simple_failure(148, 148));
		assert_failure_outcome!(simple_failure(149, 149));
		assert_failure_outcome!(simple_failure(150, 150));
		assert_failure_outcome!(simple_failure(151, 151));

		// Minority dissent has no effect.
		assert_failure_outcome!(get_outcome(6, 2, 4, 0, None));
		assert_failure_outcome!(get_outcome(6, 1, 4, 1, None));
		assert_failure_outcome!(get_outcome(6, 1, 5, 0, None));
		assert_failure_outcome!(get_outcome(6, 0, 6, 0, None));
	}

	#[test]
	fn test_no_consensus() {
		// No outcome unless there is threshold agreement.
		assert_no_outcome!(get_outcome(6, 3, 0, 3, None));
		assert_no_outcome!(get_outcome(6, 3, 3, 0, None));
		assert_no_outcome!(get_outcome(6, 3, 2, 1, None));
		assert_no_outcome!(get_outcome(6, 3, 1, 2, None));
		assert_no_outcome!(get_outcome(6, 2, 2, 2, None));

		// Missing responses have no effect.
		assert_no_outcome!(get_outcome(6, 0, 0, 0, None));
		assert_no_outcome!(get_outcome(6, 1, 0, 0, None));
		assert_no_outcome!(get_outcome(6, 0, 1, 0, None));
		assert_no_outcome!(get_outcome(6, 0, 0, 1, None));
		assert_no_outcome!(get_outcome(6, 3, 1, 1, None));
		assert_no_outcome!(get_outcome(6, 1, 3, 1, None));
		assert_no_outcome!(get_outcome(6, 3, 2, 0, None));
		assert_no_outcome!(get_outcome(6, 2, 3, 1, None));
	}

	fn test_blaming_aggregation() {
		todo!();
	}
}
