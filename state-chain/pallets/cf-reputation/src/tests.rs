mod tests {
	use crate::mock::*;
	use crate::*;
	use frame_support::{assert_noop, assert_ok};
	use sp_runtime::BuildStorage;
	use sp_runtime::DispatchError::BadOrigin;
	use std::ops::Neg;

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	fn reputation_points(who: <Test as frame_system::Config>::AccountId) -> ReputationPoints {
		ReputationPallet::reputation(who).1
	}

	fn run_heartbeats_to_block(end_block: u64) {
		for block in (HEARTBEAT_BLOCK_INTERVAL..end_block + HEARTBEAT_BLOCK_INTERVAL)
			.step_by(HEARTBEAT_BLOCK_INTERVAL as usize)
		{
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			run_to_block(block);
		}
	}

	#[test]
	fn should_have_a_list_of_validators_at_genesis() {
		new_test_ext().execute_with(|| {
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
		});
	}

	#[test]
	#[should_panic]
	fn should_panic_if_accrual_rate_is_less_than_heartbeat_interval_at_genesis() {
		mock::GenesisConfig {
			frame_system: Default::default(),
			pallet_cf_reputation: Some(ReputationPalletConfig {
				accrual_ratio: (1, HEARTBEAT_BLOCK_INTERVAL - 1),
			}),
		}
		.build_storage()
		.unwrap();
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
			let points_to_earn = 10;
			// We will need to send heartbeats for the next ACCRUAL_BLOCKS_PER_REPUTATION_POINT blocks
			run_heartbeats_to_block(ACCRUAL_BLOCKS_PER_REPUTATION_POINT * points_to_earn);
			// Alice should now have 1 point
			assert_eq!(reputation_points(ALICE), points_to_earn as i32);
		});
	}

	#[test]
	fn missing_heartbeats_should_see_loss_of_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_eq!(reputation_points(ALICE), 0);
			// We will need to send heartbeats for the next ACCRUAL_BLOCKS_PER_REPUTATION_POINT blocks
			run_heartbeats_to_block(ACCRUAL_BLOCKS_PER_REPUTATION_POINT);
			// Alice should now have 1 point
			let current_reputation = reputation_points(ALICE);
			assert_eq!(current_reputation, 1);
			let points_to_lose = 100;
			for _ in 0..points_to_lose {
				// Lose a point, move a heartbeat interval forward with no heartbeat sent
				run_to_block(System::block_number() + HEARTBEAT_BLOCK_INTERVAL);
			}
			assert_eq!(
				reputation_points(ALICE),
				current_reputation - points_to_lose
			);
		});
	}

	#[test]
	fn missing_heartbeats_should_see_slashing_when_we_hit_negative() {
		new_test_ext().execute_with(|| {
			assert_eq!(reputation_points(ALICE), 0);
			let expected_slashes = 10;
			for _ in 0..expected_slashes {
				// Lose a point, move a heartbeat interval forward with no heartbeat sent
				run_to_block(System::block_number() + HEARTBEAT_BLOCK_INTERVAL);
			}
			assert_eq!(SLASH_COUNT.with(|count| *count.borrow()), expected_slashes);
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
				ReputationPallet::update_accrual_ratio(Origin::root(), 2, 0),
				Error::<Test>::InvalidAccrualReputationBlocks
			);
			assert_noop!(
				ReputationPallet::update_accrual_ratio(
					Origin::root(),
					2,
					HEARTBEAT_BLOCK_INTERVAL - 1
				),
				Error::<Test>::InvalidAccrualReputationBlocks
			);
			assert_noop!(
				ReputationPallet::update_accrual_ratio(Origin::root(), 0, 2),
				Error::<Test>::InvalidAccrualReputationPoints
			);
			assert_ok!(ReputationPallet::update_accrual_ratio(
				Origin::root(),
				2,
				ACCRUAL_BLOCKS_PER_REPUTATION_POINT
			));

			let target_points = 2;

			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_reputation(crate::Event::AccrualRateUpdated(
					target_points,
					ACCRUAL_BLOCKS_PER_REPUTATION_POINT
				))
			);

			run_heartbeats_to_block(ACCRUAL_BLOCKS_PER_REPUTATION_POINT);
			assert_eq!(reputation_points(ALICE), target_points);
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
