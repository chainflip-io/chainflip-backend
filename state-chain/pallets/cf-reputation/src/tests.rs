mod tests {
	use crate::mock::*;
	use crate::*;
	use frame_support::{assert_noop, assert_ok};
	use sp_runtime::DispatchError::BadOrigin;
	use std::ops::Neg;
	use sp_runtime::BuildStorage;

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	fn reputation_points(who: <Test as frame_system::Config>::AccountId) -> ReputationPoints {
		ReputationPallet::reputation(who).1
	}

	#[test]
	fn should_have_a_list_of_validators_at_genesis() {
		new_test_ext().execute_with(|| {
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
		});
	}

	#[test]
	#[should_panic]
	fn should_panic_if_accrual_rate_is_more_than_heartbeat_interval_at_genesis() {
		mock::GenesisConfig {
			frame_system: Default::default(),
			pallet_cf_reputation: Some(
				ReputationPalletConfig {
					accrual_ratio: (1, HEARTBEAT_BLOCK_INTERVAL + 1)
				}
			),
		}.build_storage().unwrap();
	}

	#[test]
	fn submitting_heartbeat_from_unknown_validator_should_fail() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				ReputationPallet::heartbeat(Origin::signed(BOB)),
				<Error<Test>>::AlreadySubmittedHeartbeat
			);
		});
	}

	#[test]
	fn submitting_heartbeat_should_reward_reputation_points() {
		new_test_ext().execute_with(|| {
			// Interval 0 - expecting 0 points
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			assert_eq!(reputation_points(ALICE), 0);
			// Interval 1 - expecting 1 point
			run_to_block(HEARTBEAT_BLOCK_INTERVAL);
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			assert_eq!(reputation_points(ALICE), 1);
			// Interval 2 - expecting 2 points
			run_to_block(HEARTBEAT_BLOCK_INTERVAL * 2);
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			assert_eq!(reputation_points(ALICE), 2);
		});
	}

	#[test]
	fn updating_accrual_rate_should_affect_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				ReputationPallet::update_accrual_ratio(
					Origin::signed(ALICE),
					2,
					HEARTBEAT_BLOCK_INTERVAL
				),
				BadOrigin
			);
			assert_noop!(
				ReputationPallet::update_accrual_ratio(
					Origin::root(),
					2,
					0
				),
				Error::<Test>::InvalidReputationBlocks
			);
			assert_noop!(
				ReputationPallet::update_accrual_ratio(
					Origin::root(),
					2,
					HEARTBEAT_BLOCK_INTERVAL + 1
				),
				Error::<Test>::InvalidReputationBlocks
			);
			assert_noop!(
				ReputationPallet::update_accrual_ratio(
					Origin::root(),
					0,
					2
				),
				Error::<Test>::InvalidReputationPoints
			);
			assert_ok!(ReputationPallet::update_accrual_ratio(
				Origin::root(),
				2,
				ACCRUAL_BLOCKS_PER_REPUTATION_POINT
			));

			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_reputation(crate::Event::AccrualRateUpdated(
					2,
					ACCRUAL_BLOCKS_PER_REPUTATION_POINT
				))
			);

			// Interval 0 - expecting 0 points
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			assert_eq!(reputation_points(ALICE), 0);
			// Interval 1 - expecting 2 points
			run_to_block(HEARTBEAT_BLOCK_INTERVAL);
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			assert_eq!(reputation_points(ALICE), 2);
		});
	}

	#[test]
	fn submitting_heartbeats_in_same_heartbeat_interval_should_fail() {
		new_test_ext().execute_with(|| {
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			assert_noop!(
				ReputationPallet::heartbeat(Origin::signed(ALICE)),
				Error::<Test>::AlreadySubmittedHeartbeat
			);
		});
	}

	#[test]
	fn missing_a_heartbeat_submission_should_penalise_reputation_points() {
		new_test_ext().execute_with(|| {
			let (points, blocks) = POINTS_PER_BLOCK_PENALTY;
			// We are starting out with zero points
			assert_eq!(reputation_points(ALICE), 0);
			// Interval 1 - with no heartbeat we will lose `points` per `block`
			run_to_block(HEARTBEAT_BLOCK_INTERVAL);
			assert_eq!(
				reputation_points(ALICE),
				(HEARTBEAT_BLOCK_INTERVAL as i32 / blocks as i32 * points as i32).neg()
			);
			// Interval 1 - with no heartbeat this will continue
			run_to_block(HEARTBEAT_BLOCK_INTERVAL * 2);
			assert_eq!(
				reputation_points(ALICE),
				2 * (HEARTBEAT_BLOCK_INTERVAL as i32 / blocks as i32 * points as i32).neg()
			);
		});
	}

	#[test]
	fn reporting_any_offline_condition_for_unknown_validator_should_produce_error() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				ReputationPallet::report(OfflineCondition::ParticipateSigningFailed(100), &BOB),
				ReportError::UnknownValidator
			);
			assert_noop!(
				ReputationPallet::report(OfflineCondition::BroadcastOutputFailed(100), &BOB),
				ReportError::UnknownValidator
			);
		});
	}

	#[test]
	fn reporting_any_offline_condition_for_known_validator_without_reputation_recorded_should_produce_error(
	) {
		new_test_ext().execute_with(|| {
			assert_noop!(
				ReputationPallet::report(OfflineCondition::ParticipateSigningFailed(100), &ALICE),
				ReportError::UnknownValidator
			);
			assert_noop!(
				ReputationPallet::report(OfflineCondition::BroadcastOutputFailed(100), &ALICE),
				ReportError::UnknownValidator
			);
		});
	}

	#[test]
	fn reporting_broadcast_output_failed_offline_condition_should_penalise_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			let points_before = reputation_points(ALICE);
			let penalty = 100;
			assert_ok!(ReputationPallet::report(
				OfflineCondition::BroadcastOutputFailed(penalty),
				&ALICE
			));
			assert_eq!(reputation_points(ALICE), points_before - penalty);
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_reputation(crate::Event::BroadcastOutputFailed(
					ALICE, penalty
				))
			);
		});
	}

	#[test]
	fn reporting_participate_in_signing_offline_condition_should_penalise_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			let points_before = reputation_points(ALICE);
			let penalty = 100;
			assert_ok!(ReputationPallet::report(
				OfflineCondition::ParticipateSigningFailed(penalty),
				&ALICE
			));
			assert_eq!(reputation_points(ALICE), points_before - penalty);
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_reputation(crate::Event::ParticipateSigningFailed(
					ALICE, penalty
				))
			);
		});
	}
}
