mod tests {
	use crate::mock::*;
	use crate::OfflineCondition::*;
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
		ReputationPallet::reputation(who).reputation_points
	}

	// Cycle heartbeat interval sending the heartbeat extrinsic in each
	fn run_heartbeat_intervals(
		validator: <Test as frame_system::Config>::AccountId,
		intervals: u64,
	) {
		let start_block_number = System::block_number();
		// Inclusive
		for interval in 1..=intervals {
			let block = interval * HEARTBEAT_BLOCK_INTERVAL;
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(validator)));
			run_to_block(start_block_number + block);
		}
	}

	// We will need to send heartbeats for the next ACCRUAL_BLOCKS_PER_REPUTATION_POINT blocks
	fn submit_heartbeats_for_accrual_blocks(
		validator: <Test as frame_system::Config>::AccountId,
		number_of_accruals: u64,
	) {
		let intervals = ACCRUAL_BLOCKS * number_of_accruals / HEARTBEAT_BLOCK_INTERVAL;
		run_heartbeat_intervals(validator, intervals + 1 /* roundup */);
	}

	// Move a heartbeat interval forward with no heartbeat sent
	fn move_forward_by_heartbeat_intervals(heartbeats: u64) {
		for _ in 0..heartbeats {
			run_to_block(System::block_number() + HEARTBEAT_BLOCK_INTERVAL);
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
			let number_of_accruals = 10;
			submit_heartbeats_for_accrual_blocks(ALICE, number_of_accruals);
			// Alice should now have 10 points
			assert_eq!(
				reputation_points(ALICE),
				number_of_accruals as i32 * ACCRUAL_POINTS
			);
		});
	}

	#[test]
	fn missing_heartbeats_should_see_loss_of_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_eq!(reputation_points(ALICE), 0);
			// We will need to send heartbeats for the next ACCRUAL_BLOCKS_PER_REPUTATION_POINT blocks
			submit_heartbeats_for_accrual_blocks(ALICE, 1);
			// Alice should now have 1 point
			let current_reputation = reputation_points(ALICE);
			assert_eq!(current_reputation, 1 * ACCRUAL_POINTS);
			let heartbeats = 100;
			move_forward_by_heartbeat_intervals(heartbeats);
			assert_eq!(
				reputation_points(ALICE),
				current_reputation
					- (heartbeats as u32
						* POINTS_PER_BLOCK_PENALTY.points as u32
						* HEARTBEAT_BLOCK_INTERVAL as u32
						/ POINTS_PER_BLOCK_PENALTY.blocks as u32) as ReputationPoints
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
				Error::<Test>::InvalidAccrualOnlineCredits
			);
			assert_noop!(
				ReputationPallet::update_accrual_ratio(
					Origin::root(),
					2,
					HEARTBEAT_BLOCK_INTERVAL - 1
				),
				Error::<Test>::InvalidAccrualOnlineCredits
			);
			assert_noop!(
				ReputationPallet::update_accrual_ratio(Origin::root(), 0, 2),
				Error::<Test>::InvalidAccrualReputationPoints
			);
			let accrual_points = 2;
			assert_ok!(ReputationPallet::update_accrual_ratio(
				Origin::root(),
				accrual_points,
				ACCRUAL_BLOCKS
			));

			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_reputation(crate::Event::AccrualRateUpdated(
					accrual_points,
					ACCRUAL_BLOCKS
				))
			);
			let number_of_accruals = 2;
			submit_heartbeats_for_accrual_blocks(ALICE, number_of_accruals);
			assert_eq!(
				reputation_points(ALICE),
				accrual_points * number_of_accruals as i32
			);
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
			let ReputationPenalty { points, blocks } = POINTS_PER_BLOCK_PENALTY;
			// We are starting out with zero points
			assert_eq!(reputation_points(ALICE), 0);
			// Interval 1 - with no heartbeat we will lose `points` per `block`
			run_to_block(HEARTBEAT_BLOCK_INTERVAL);
			assert_eq!(
				reputation_points(ALICE),
				(HEARTBEAT_BLOCK_INTERVAL as i32 / blocks as i32 * points as i32).neg()
			);
			// Interval 2 - with no heartbeat this will continue
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
				ReputationPallet::report(OfflineCondition::ParticipateSigningFailed, 100, &BOB),
				ReportError::UnknownValidator
			);
			assert_noop!(
				ReputationPallet::report(OfflineCondition::BroadcastOutputFailed, 100, &BOB),
				ReportError::UnknownValidator
			);
		});
	}

	#[test]
	fn reporting_any_offline_condition_for_known_validator_without_reputation_recorded_should_produce_error(
	) {
		new_test_ext().execute_with(|| {
			assert_noop!(
				ReputationPallet::report(OfflineCondition::ParticipateSigningFailed, 100, &ALICE),
				ReportError::UnknownValidator
			);
			assert_noop!(
				ReputationPallet::report(OfflineCondition::BroadcastOutputFailed, 100, &ALICE),
				ReportError::UnknownValidator
			);
		});
	}

	#[test]
	fn reporting_any_offline_condition_should_penalise_reputation_points() {
		new_test_ext().execute_with(|| {
			let offline_test = |offline_condition: OfflineCondition,
			                    who: <Test as frame_system::Config>::AccountId,
			                    penalty: ReputationPoints| {
				let points_before = reputation_points(who);
				assert_ok!(ReputationPallet::report(
					offline_condition.clone(),
					penalty,
					&who
				));
				assert_eq!(reputation_points(who), points_before - penalty);
				assert_eq!(
					last_event(),
					mock::Event::pallet_cf_reputation(crate::Event::OfflineConditionPenalty(
						who,
						offline_condition,
						penalty
					))
				);
			};
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			offline_test(ParticipateSigningFailed, ALICE, 100);
			offline_test(BroadcastOutputFailed, ALICE, 100);
			offline_test(ContradictingSelfDuringSigningCeremony, ALICE, 100);
			offline_test(NotEnoughPerformanceCredits, ALICE, 100);
		});
	}

	#[test]
	fn reporting_participate_in_signing_offline_condition_should_penalise_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
			let points_before = reputation_points(ALICE);
			let penalty = 100;
			assert_ok!(ReputationPallet::report(
				ParticipateSigningFailed,
				penalty,
				&ALICE
			));
			assert_eq!(reputation_points(ALICE), points_before - penalty);
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_reputation(crate::Event::OfflineConditionPenalty(
					ALICE,
					ParticipateSigningFailed,
					penalty
				))
			);
		});
	}

	#[test]
	fn on_new_epoch_should_see_new_set_of_validators_and_those_before_maintain_reputation() {
		new_test_ext().execute_with(|| {
			let number_of_accruals = 10;
			submit_heartbeats_for_accrual_blocks(ALICE, number_of_accruals);
			assert_eq!(
				reputation_points(ALICE),
				number_of_accruals as i32 * ACCRUAL_POINTS
			);
			// Rotation to Bob
			ReputationPallet::on_new_epoch(&vec![BOB], 0);
			submit_heartbeats_for_accrual_blocks(BOB, number_of_accruals);
			assert_eq!(
				reputation_points(ALICE),
				number_of_accruals as i32 * ACCRUAL_POINTS
			);
			assert_eq!(
				reputation_points(BOB),
				number_of_accruals as i32 * ACCRUAL_POINTS
			);
		});
	}
}
