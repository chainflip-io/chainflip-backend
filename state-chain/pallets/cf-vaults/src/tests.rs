mod tests {
	use crate::{
		mock::*, ActiveWindows, BlockHeightWindow, Error, Event as PalletEvent,
		PendingVaultRotations, VaultRotationStatus, Vaults,
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

			// Vault is updated.
			assert_eq!(
				Vaults::<MockRuntime>::get(ChainId::Ethereum)
					.expect("Ethereum Vault should exists")
					.public_key,
				new_public_key,
			);

			// Status is complete.
			assert_eq!(
				PendingVaultRotations::<MockRuntime>::get(ChainId::Ethereum),
				Some(VaultRotationStatus::Complete {
					tx_hash: TX_HASH.to_vec()
				}),
			);

			// Active windows have been updated.
			let epoch = <MockRuntime as crate::Config>::EpochInfo::epoch_index();
			assert_eq!(
				ActiveWindows::<MockRuntime>::get(epoch, ChainId::Ethereum),
				BlockHeightWindow {
					from: 0,
					to: Some(ROTATION_BLOCK_NUMBER + crate::ETHEREUM_LEEWAY_IN_BLOCKS)
				}
			);
			assert_eq!(
				ActiveWindows::<MockRuntime>::get(epoch + 1, ChainId::Ethereum),
				BlockHeightWindow {
					from: ROTATION_BLOCK_NUMBER,
					to: None
				}
			);
		});
	}
}
