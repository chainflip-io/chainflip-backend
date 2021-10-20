/// For many of these tests we use
/// move_forward_by_heartbeat_intervals(1);
/// in order to progress past the first, genesis heartbeat interval
/// since nodes in the genesis interval have, by default, submitted a heartbeat
mod tests {
	use crate::mock::*;
	use crate::*;
	use cf_traits::IsOnline;
	use frame_support::{assert_noop, assert_ok};

	// Cycle heartbeat interval sending the heartbeat extrinsic in each
	fn run_heartbeat_intervals(
		validators: &[<Test as frame_system::Config>::AccountId],
		intervals: u64,
	) {
		let start_block_number = System::block_number();
		// Inclusive
		for interval in 1..=intervals {
			let block = interval * HEARTBEAT_BLOCK_INTERVAL;
			for validator in validators {
				assert_ok!(OnlinePallet::heartbeat(Origin::signed(*validator)));
			}
			run_to_block(start_block_number + block);
		}
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
			move_forward_by_heartbeat_intervals(1);
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
		});
	}

	#[test]
	fn submitting_heartbeat_from_unknown_validator_should_fail() {
		new_test_ext().execute_with(|| {
			move_forward_by_heartbeat_intervals(1);
			assert_noop!(
				OnlinePallet::heartbeat(Origin::signed(BOB)),
				Error::<Test>::AlreadySubmittedHeartbeat
			);
		});
	}

	#[test]
	fn we_should_be_online_when_submitting_heartbeats_and_offline_when_not() {
		new_test_ext().execute_with(|| {
			move_forward_by_heartbeat_intervals(1);
			run_heartbeat_intervals(&[ALICE], 1);
			assert!(<OnlinePallet as IsOnline>::is_online(&ALICE));
			run_heartbeat_intervals(&[ALICE], 1);
			assert!(<OnlinePallet as IsOnline>::is_online(&ALICE));
			// Fail to submit for two heartbeats
			move_forward_by_heartbeat_intervals(2);
			assert_eq!(<OnlinePallet as IsOnline>::is_online(&ALICE), false);
		});
	}

	#[test]
	fn we_should_see_missing_validators_when_not_having_submitted_one_interval() {
		new_test_ext().execute_with(|| {
			move_forward_by_heartbeat_intervals(1);
			run_heartbeat_intervals(&[ALICE], 1);
			assert!(<OnlinePallet as IsOnline>::is_online(&ALICE));
			run_heartbeat_intervals(&[ALICE], 1);
			assert!(<OnlinePallet as IsOnline>::is_online(&ALICE));
			// Fail to submit for two heartbeats
			move_forward_by_heartbeat_intervals(2);
			assert_eq!(<OnlinePallet as IsOnline>::is_online(&ALICE), false);
		});
	}

	#[test]
	fn we_should_see_offline_validators_when_not_having_submitted_one_interval() {
		new_test_ext().execute_with(|| {
			move_forward_by_heartbeat_intervals(1);
			run_heartbeat_intervals(&[ALICE], 1);
			assert!(<OnlinePallet as IsOnline>::is_online(&ALICE));
			run_heartbeat_intervals(&[ALICE], 1);
			assert!(<OnlinePallet as IsOnline>::is_online(&ALICE));
			// Fail to submit for two heartbeats
			move_forward_by_heartbeat_intervals(2);
			assert_eq!(<OnlinePallet as IsOnline>::is_online(&ALICE), false);
		});
	}
}
