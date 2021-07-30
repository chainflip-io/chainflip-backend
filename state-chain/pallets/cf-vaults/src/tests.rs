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
				VaultsPallet::on_completed(vec![], 0),
				Err(AuctionError::Abort)
			);
			// Everything ok with a set of numbers
			// Nothing running at the moment
			assert!(VaultsPallet::vaults_rotated());
			// Request index 2
			assert_ok!(VaultsPallet::on_completed(vec![ALICE, BOB, CHARLIE], 0));
			// Confirm we have a new vault rotation process running
			assert!(!VaultsPallet::vaults_rotated());
			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::KeygenRequestEvent(
					2,
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
			assert_ok!(VaultsPallet::on_completed(vec![ALICE, BOB, CHARLIE], 0));
			assert_ok!(VaultsPallet::witness_keygen_response(
				Origin::signed(ALICE),
				1,
				KeygenResponse::Success(vec![])
			));

			// Check our mock chain that this was processed
			assert!(OTHER_CHAIN_RESULT.with(|l| *l.borrow() == 1));

			// A subsequent key generation request
			assert_ok!(VaultsPallet::on_completed(vec![ALICE, BOB, CHARLIE], 0));

			// This time we respond with bad news
			assert_ok!(VaultsPallet::witness_keygen_response(
				Origin::signed(ALICE),
				2,
				KeygenResponse::Failure(vec![BOB, CHARLIE])
			));

			// This would have not got to the specialisation
			assert!(OTHER_CHAIN_RESULT.with(|l| *l.borrow() != 2));

			// We would have aborted this rotation and hence no rotations underway
			assert!(VaultsPallet::vaults_rotated());

			// Penalised bad validators would be now punished
			assert_eq!(bad_validators(), vec![BOB, CHARLIE]);
		});
	}

	#[test]
	fn vault_rotation_request() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::try_complete_vault_rotation(
				0,
				Ok(VaultRotationRequest {
					chain: ChainParams::Other(vec![])
				})
			));

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::VaultRotationRequest(
					0,
					VaultRotationRequest {
						chain: Other(vec![])
					}
				))
			);

			assert_eq!(
				VaultsPallet::try_complete_vault_rotation(
					0,
					Err(RotationError::BadValidators(vec![ALICE, BOB]))
				)
				.err(),
				Some(RotationError::VaultRotationCompletionFailed)
			);

			// We would have aborted this rotation and hence no rotations underway
			assert!(VaultsPallet::vaults_rotated());

			// Penalised bad validators would be now punished
			assert_eq!(bad_validators(), vec![ALICE, BOB]);
		});
	}

	#[test]
	fn vault_rotation_response() {
		new_test_ext().execute_with(|| {
			assert_ok!(VaultsPallet::try_complete_vault_rotation(
				0,
				Ok(VaultRotationRequest {
					chain: ChainParams::Other(vec![])
				})
			));

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::VaultRotationRequest(
					0,
					VaultRotationRequest {
						chain: Other(vec![])
					}
				))
			);

			assert_ok!(VaultsPallet::witness_vault_rotation_response(
				Origin::signed(ALICE),
				0,
				VaultRotationResponse {
					old_key: "old_key".as_bytes().to_vec(),
					new_key: "new_key".as_bytes().to_vec(),
					tx: "tx".as_bytes().to_vec(),
				}
			));

			// Check the event emitted
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_vaults(crate::Event::VaultRotationCompleted(0))
			);
		});
	}
}
