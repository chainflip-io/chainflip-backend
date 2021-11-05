mod tests {
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
			let new_public_key: Vec<u8> =
				GENESIS_ETHEREUM_AGG_PUB_KEY.iter().map(|x| x + 1).collect();

			assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));
			let ceremony_id = VaultsPallet::keygen_ceremony_id_counter();
			assert_ok!(VaultsPallet::keygen_success(
				Origin::root(),
				ceremony_id,
				ChainId::Ethereum,
				new_public_key.clone()
			));

			// Can't be reported twice
			assert_noop!(
				VaultsPallet::keygen_success(
					Origin::root(),
					ceremony_id,
					ChainId::Ethereum,
					new_public_key.clone()
				),
				Error::<MockRuntime>::InvalidRotationStatus
			);

			// Can't change our mind
			assert_noop!(
				VaultsPallet::keygen_failure(
					Origin::root(),
					ceremony_id,
					ChainId::Ethereum,
					vec![]
				),
				Error::<MockRuntime>::InvalidRotationStatus
			);
		});
	}

	#[test]
	fn keygen_failure() {
		new_test_ext().execute_with(|| {
			const BAD_CANDIDATES: &'static [<MockRuntime as Chainflip>::ValidatorId] =
				&[BOB, CHARLIE];

			assert_ok!(VaultsPallet::start_vault_rotation(ALL_CANDIDATES.to_vec()));

			let ceremony_id = VaultsPallet::keygen_ceremony_id_counter();

			// The ceremony failed.
			assert_ok!(VaultsPallet::keygen_failure(
				Origin::root(),
				ceremony_id,
				ChainId::Ethereum,
				BAD_CANDIDATES.to_vec()
			));

			// KeygenAborted event emitted.
			assert_eq!(
				last_event(),
				PalletEvent::KeygenAborted(vec![ChainId::Ethereum]).into()
			);

			// All rotations have been aborted.
			assert!(VaultsPallet::no_active_chain_vault_rotations());

			// Bad validators have been reported.
			assert_eq!(MockOfflineReporter::get_reported(), BAD_CANDIDATES);

			// Can't be reported twice
			assert_noop!(
				VaultsPallet::keygen_failure(
					Origin::root(),
					ceremony_id,
					ChainId::Ethereum,
					vec![]
				),
				Error::<MockRuntime>::NoActiveRotation
			);

			// Can't change our mind
			assert_noop!(
				VaultsPallet::keygen_success(
					Origin::root(),
					ceremony_id,
					ChainId::Ethereum,
					vec![]
				),
				Error::<MockRuntime>::NoActiveRotation
			);
		});
	}

	#[test]
	fn vault_key_rotated() {
		new_test_ext().execute_with(|| {
			const ROTATION_BLOCK_NUMBER: u64 = 42;
			const TX_HASH: [u8; 32] = [0xab; 32];
			let new_public_key: Vec<u8> =
				GENESIS_ETHEREUM_AGG_PUB_KEY.iter().map(|x| x + 1).collect();

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
			assert_ok!(VaultsPallet::keygen_success(
				Origin::root(),
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
			let current_epoch = <MockRuntime as crate::Config>::EpochInfo::epoch_index();

			let Vault {
				public_key,
				active_window,
			} = Vaults::<MockRuntime>::get(current_epoch, ChainId::Ethereum)
				.expect("Ethereum Vault should exist");

			assert_eq!(
				public_key, GENESIS_ETHEREUM_AGG_PUB_KEY,
				"we should have the old agg key in the genesis vault"
			);

			assert_eq!(
				active_window,
				BlockHeightWindow {
					from: 0,
					to: Some(ROTATION_BLOCK_NUMBER)
				},
				"we should have the block height set for the genesis or current epoch"
			);

			// The next epoch
			let next_epoch = current_epoch + 1;

			let Vault {
				public_key,
				active_window,
			} = Vaults::<MockRuntime>::get(next_epoch, ChainId::Ethereum)
				.expect("Ethereum Vault should exist in the next epoch");

			assert_eq!(
				public_key, new_public_key,
				"we should have the new public key in the new vault for the next epoch"
			);

			assert_eq!(
				active_window,
				BlockHeightWindow {
					from: ROTATION_BLOCK_NUMBER.saturating_add(1),
					to: None
				},
				"we should have set the starting point for the new vault's active window as the next
				after the reported block number"
			);

			// Status is complete.
			assert_eq!(
				PendingVaultRotations::<MockRuntime>::get(ChainId::Ethereum),
				Some(VaultRotationStatus::Complete {
					tx_hash: TX_HASH.to_vec()
				}),
			);
		});
	}
}
