mod test {
	use crate::ethereum::EthereumChain;
	use crate::mock::*;
	use crate::rotation::ChainParams::Other;
	use crate::*;
	use ethabi::Token;
	use frame_support::{assert_err, assert_ok};
	use sp_core::Hasher;
	use sp_runtime::traits::Keccak256;

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
						chain: Chain::Ethereum,
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
				// this key is different to the genesis key
				KeygenResponse::Success(vec![1; 33])
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
				KeygenResponse::Error(vec![BOB, CHARLIE])
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
				KeygenResponse::Success(vec![1; 33])
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
			let new_public_key = vec![1; 33];
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
				KeygenResponse::Success(vec![1; 33])
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
				VaultRotationResponse::Error
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
			let mut new_public_key = hex::decode("1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d").unwrap();
			new_public_key.push(1);
			let validators = vec![ALICE, BOB, CHARLIE];
			assert_ok!(EthereumChain::<MockRuntime>::rotate_vault(
				0,
				new_public_key.clone(),
				validators.clone()
			));
			// let new_public_key_x = Token::Uint(10521316663921629387264629518161886172223783929820773409615991397525613232925);
			// let new_public_key_x = new_public_key_x.to_be_bytes();
			// let new_public_key_y_parity: u8 = 1;
			let call_data_no_sig = hex::decode("24969d5d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap();
			println!("Call data as bytes: {:?}", call_data_no_sig);
			let expected_signing_request = ThresholdSignatureRequest {
				payload: Keccak256::hash(
					&call_data_no_sig
				)
				.0
				.into(),
				// The CFE stores the pubkey as the compressed 33 byte pubkey
				// therefore the SC must emit like this
				public_key: vec![0; 33],
				validators,
			};
			// we need to set the previous key on genesis
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::ThresholdSignatureRequest(
					0,
					expected_signing_request
				))
			);
		});
	}

	#[test]
	fn should_error_when_attempting_to_use_use_unset_new_public_key() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));

			assert_err!(
				VaultsPallet::threshold_signature_response(
					Origin::root(),
					1,
					ThresholdSignatureResponse::Success(
						[0; 32],
						SchnorrSigTruncPubkey {
							eth_pub_key: [0; 20],
							s: [0; 32],
						}
					)
				),
				crate::Error::<MockRuntime>::NewPublicKeyNotSet,
			);
		});
	}

	#[test]
	fn attempting_to_call_threshold_sig_resp_on_uninitialised_ceremony_id_fails_with_invalid_ceremony_id(
	) {
		new_test_ext().execute_with(|| {
			assert_err!(
				VaultsPallet::threshold_signature_response(
					Origin::root(),
					// we haven't started a new rotation, so ceremony 1 has not been initialised
					1,
					ThresholdSignatureResponse::Success(
						[0; 32],
						SchnorrSigTruncPubkey {
							eth_pub_key: [0; 20],
							s: [0; 32],
						}
					)
				),
				Error::<MockRuntime>::InvalidCeremonyId,
			);
		});
	}

	#[test]
	fn attempting_to_call_vault_rotation_response_on_uninitialised_ceremony_id_fails_with_invalid_ceremony_id(
	) {
		new_test_ext().execute_with(|| {
			assert_err!(
				VaultsPallet::vault_rotation_response(
					Origin::root(),
					// we haven't started a new rotation, so ceremony 1 has not been initialised
					1,
					VaultRotationResponse::Success {
						tx_hash: vec![0; 32].into()
					}
				),
				Error::<MockRuntime>::InvalidCeremonyId,
			);
		});
	}

	#[test]
	fn should_encode_set_agg_key_with_agg_key() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::start_vault_rotation(vec![
				ALICE, BOB, CHARLIE
			]));
			let first_ceremony_id = VaultsPallet::current_request();
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				first_ceremony_id,
				// this key is different to the genesis key
				KeygenResponse::Success(vec![1; 33])
			));
			// we have never created a request to sign, but we received a response?
			// this is at least better than before
			assert_ok!(VaultsPallet::threshold_signature_response(
				Origin::root(),
				first_ceremony_id,
				ThresholdSignatureResponse::Success(
					[0; 32],
					SchnorrSigTruncPubkey {
						eth_pub_key: [0; 20],
						s: [0; 32],
					}
				)
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
