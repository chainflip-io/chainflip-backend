mod tests {
	use crate::{mock::*, Error, Event as PalletEvent};
	use frame_support::{assert_noop, assert_ok};
	use cf_chains::ChainId;
	use cf_traits::{Chainflip, VaultRotator};

	fn last_event() -> Event {
		frame_system::Pallet::<MockRuntime>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	const ALL_CANDIDATES: &[<MockRuntime as Chainflip>::ValidatorId] = &[
		ALICE, BOB, CHARLIE
	];

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
				).into()
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
			let first_ceremony_id = VaultsPallet::keygen_ceremony_id_counter();
			assert_ok!(VaultsPallet::keygen_success(
				Origin::root(),
				first_ceremony_id,
				ChainId::Ethereum,
				new_public_key
			));
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
		});
	}
}
