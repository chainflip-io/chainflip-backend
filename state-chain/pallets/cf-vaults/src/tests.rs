mod test {
	use crate::ethereum::EthereumChain;
	use crate::mock::*;
	use crate::rotation::ChainParams::Other;
	use crate::*;
	use frame_support::assert_ok;

	fn last_event() -> mock::Event {
		frame_system::Pallet::<MockRuntime>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	#[test]
	fn keygen_request() {
		new_test_ext().execute_with(|| {
			// An empty set and an error is thrown back, request index 1
			assert_eq!(
				VaultsPallet::start_vault_rotation(vec![]),
				Err(RotationError::EmptyValidatorSet)
			);
			// Everything ok with a set of numbers
			// Nothing running at the moment
			assert!(VaultsPallet::rotations_complete());
			// Request index 2
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));
			// Confirm we have a new vault rotation process running
			assert!(!VaultsPallet::rotations_complete());
			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::KeygenRequest(
					VaultsPallet::current_request(),
					KeygenRequest {
						chain_type: ChainType::Ethereum,
						validator_candidates: vec![ALICE, BOB, CHARLIE],
					}
				))
			);
		});
	}

	#[test]
	fn keygen_response() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));
			let first_ceremony_id = VaultsPallet::current_request();
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				first_ceremony_id,
				KeygenResponse::Success(vec![1, 2, 3])
			));

			// A subsequent key generation request
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));

			let second_ceremony_id = VaultsPallet::current_request();
			// This time we respond with bad news
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				second_ceremony_id,
				KeygenResponse::Failure(vec![BOB, CHARLIE])
			));

			// Check the event emitted of an aborted rotation with are two requests
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::RotationAborted(vec![
					first_ceremony_id,
					second_ceremony_id
				]))
			);

			// We would have aborted this rotation and hence no rotations underway
			assert!(VaultsPallet::rotations_complete());

			// Penalised bad validators would be now punished
			assert_eq!(bad_validators(), vec![BOB, CHARLIE]);
		});
	}

	#[test]
	fn vault_rotation_request() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				VaultsPallet::current_request(),
				KeygenResponse::Success(vec![1, 2, 3])
			));
			assert_ok!(VaultsPallet::request_vault_rotation(
				VaultsPallet::current_request(),
				Ok(VaultRotationRequest {
					chain: ChainParams::Other(vec![])
				})
			));

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::VaultRotationRequest(
					1,
					VaultRotationRequest {
						chain: Other(vec![])
					}
				))
			);

			assert_eq!(
				VaultsPallet::request_vault_rotation(
					VaultsPallet::current_request(),
					Err(RotationError::BadValidators(vec![ALICE, BOB]))
				)
				.err(),
				Some(RotationError::VaultRotationCompletionFailed)
			);

			// We would have aborted this rotation and hence no rotations underway
			assert!(VaultsPallet::rotations_complete());

			// Penalised bad validators would be now punished
			assert_eq!(bad_validators(), vec![ALICE, BOB]);
		});
	}

	#[test]
	fn should_vault_rotation_response_receiving_success() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));
			let new_public_key = vec![1, 2, 3];
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				VaultsPallet::current_request(),
				KeygenResponse::Success(new_public_key.clone())
			));
			assert_ok!(VaultsPallet::request_vault_rotation(
				VaultsPallet::current_request(),
				Ok(VaultRotationRequest {
					chain: ChainParams::Other(vec![])
				})
			));

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::VaultRotationRequest(
					VaultsPallet::current_request(),
					VaultRotationRequest {
						chain: Other(vec![])
					}
				))
			);

			let tx_hash = "tx_hash".as_bytes().to_vec();
			assert_ok!(VaultsPallet::vault_rotation_response(
				Origin::root(),
				VaultsPallet::current_request(),
				VaultRotationResponse::Success {
					tx_hash: tx_hash.clone(),
				}
			));

			// Confirm we have rotated the keys
			assert_eq!(VaultsPallet::eth_vault().tx_hash, tx_hash);
			assert_eq!(
				VaultsPallet::eth_vault().previous_key,
				ethereum_public_key()
			);
			assert_eq!(VaultsPallet::eth_vault().current_key, new_public_key);

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::VaultRotationCompleted(
					VaultsPallet::current_request()
				))
			);
		});
	}

	#[test]
	fn should_abort_vault_rotation_response_receiving_error() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				VaultsPallet::current_request(),
				KeygenResponse::Success(vec![1, 2, 3])
			));
			assert_ok!(VaultsPallet::request_vault_rotation(
				VaultsPallet::current_request(),
				Ok(VaultRotationRequest {
					chain: ChainParams::Other(vec![])
				})
			));

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::VaultRotationRequest(
					VaultsPallet::current_request(),
					VaultRotationRequest {
						chain: Other(vec![])
					}
				))
			);

			assert_ok!(VaultsPallet::vault_rotation_response(
				Origin::root(),
				VaultsPallet::current_request(),
				VaultRotationResponse::Failure
			));

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::RotationAborted(vec![
					VaultsPallet::current_request()
				]))
			);
		});
	}

	// Ethereum tests
	#[test]
	fn try_starting_a_vault_rotation() {
		new_test_ext().execute_with(|| {
			assert_ok!(EthereumChain::<MockRuntime>::rotate_vault(
				0,
				vec![],
				vec![ALICE, BOB, CHARLIE]
			));
			let signing_request = ThresholdSignatureRequest {
				payload: EthereumChain::<MockRuntime>::encode_set_agg_key_with_agg_key(
					vec![],
					SchnorrSignature::default(),
				)
				.unwrap(),
				public_key: vec![],
				validators: vec![ALICE, BOB, CHARLIE],
			};
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::ThresholdSignatureRequest(
					0,
					signing_request
				))
			);
		});
	}

	#[test]
	fn witness_eth_signing_tx_response() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));

			assert_ok!(VaultsPallet::threshold_signature_response(
				Origin::root(),
				1,
				ThresholdSignatureResponse::Success(SchnorrSignature {
					r: [0; 20],
					s: [0; 32],
				})
			));
		});
	}

	#[test]
	fn should_increment_nonce_for_ethereum_and_other_chain_independently() {
		new_test_ext().execute_with(|| {
			assert_eq!(VaultsPallet::next_nonce(NonceIdentifier::Ethereum), 1u64);
			assert_eq!(VaultsPallet::next_nonce(NonceIdentifier::Ethereum), 2u64);
			assert_eq!(VaultsPallet::next_nonce(NonceIdentifier::Bitcoin), 1u64);
			assert_eq!(VaultsPallet::next_nonce(NonceIdentifier::Dot), 1u64);
		});
	}
}
