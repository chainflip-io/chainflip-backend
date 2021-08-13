mod test {
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
				VaultsPallet::on_auction_completed(vec![], 0),
				Err(RotationError::EmptyValidatorSet)
			);
			// Everything ok with a set of numbers
			// Nothing running at the moment
			assert!(VaultsPallet::rotations_complete());
			// Request index 2
			assert_ok!(VaultsPallet::on_auction_completed(
				vec![ALICE, BOB, CHARLIE],
				0
			));
			// Confirm we have a new vault rotation process running
			assert!(!VaultsPallet::rotations_complete());
			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::KeygenRequestEvent(
					VaultsPallet::current_request(),
					KeygenRequest {
						chain: Other(vec![]),
						validator_candidates: vec![ALICE, BOB, CHARLIE],
					}
				))
			);
		});
	}

	#[test]
	fn keygen_response() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::on_auction_completed(
				vec![ALICE, BOB, CHARLIE],
				0
			));
			let first_request_idx = VaultsPallet::current_request();
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				first_request_idx,
				KeygenResponse::Success(vec![])
			));

			// Check our mock chain that this was processed
			assert!(OTHER_CHAIN_RESULT.with(|l| *l.borrow() == VaultsPallet::current_request()));

			// A subsequent key generation request
			assert_ok!(VaultsPallet::on_auction_completed(
				vec![ALICE, BOB, CHARLIE],
				0
			));

			let second_request_idx = VaultsPallet::current_request();
			// This time we respond with bad news
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				second_request_idx,
				KeygenResponse::Failure(vec![BOB, CHARLIE])
			));

			// Check the event emitted of an aborted rotation with are two requests
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::RotationAborted(vec![
					first_request_idx,
					second_request_idx
				]))
			);

			// This would have not got to the specialisation but the request index would have incremented
			assert!(OTHER_CHAIN_RESULT.with(|l| *l.borrow() == VaultsPallet::current_request() - 1));

			// We would have aborted this rotation and hence no rotations underway
			assert!(VaultsPallet::rotations_complete());

			// Penalised bad validators would be now punished
			assert_eq!(bad_validators(), vec![BOB, CHARLIE]);
		});
	}

	#[test]
	fn vault_rotation_request() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::on_auction_completed(
				vec![ALICE, BOB, CHARLIE],
				0
			));
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				VaultsPallet::current_request(),
				KeygenResponse::Success(vec![])
			));
			assert_ok!(VaultsPallet::try_complete_vault_rotation(
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
				VaultsPallet::try_complete_vault_rotation(
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
	fn vault_rotation_response() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::on_auction_completed(
				vec![ALICE, BOB, CHARLIE],
				0
			));
			assert_ok!(VaultsPallet::keygen_response(
				Origin::root(),
				VaultsPallet::current_request(),
				KeygenResponse::Success(vec![])
			));
			assert_ok!(VaultsPallet::try_complete_vault_rotation(
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
				VaultRotationResponse {
					old_key: "old_key".as_bytes().to_vec(),
					new_key: "new_key".as_bytes().to_vec(),
					tx: "tx".as_bytes().to_vec(),
				}
			));

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::VaultRotationCompleted(
					VaultsPallet::current_request()
				))
			);
		});
	}
}
