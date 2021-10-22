mod tests {
	use crate::mock::*;
	use crate::*;
	use cf_traits::{EpochInfo, IsOnline};
	use frame_support::{assert_noop, assert_ok};

	// Move forward one heartbeat interval sending the heartbeat extrinsic for nodes
	fn run_heartbeat_interval(nodes: &[<Test as frame_system::Config>::AccountId]) {
		let start_block_number = System::block_number();
		for node in nodes {
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(*node)));
		}
		run_to_block(start_block_number + HEARTBEAT_BLOCK_INTERVAL);
	}

	// Move a heartbeat intervals forward with no heartbeat sent
	fn go_to_interval(interval: u64) {
		run_to_block(interval * HEARTBEAT_BLOCK_INTERVAL);
	}

	#[test]
	fn submitting_heartbeat_more_than_once_in_an_interval_should_fail() {
		new_test_ext().execute_with(|| {
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
			assert_noop!(
				OnlinePallet::heartbeat(Origin::signed(ALICE)),
				Error::<Test>::AlreadySubmittedHeartbeat
			);
			assert_noop!(
				OnlinePallet::heartbeat(Origin::signed(ALICE)),
				Error::<Test>::AlreadySubmittedHeartbeat
			);
			go_to_interval(1);
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
		});
	}

	#[test]
	fn we_should_be_online_when_submitting_heartbeats_and_offline_when_not() {
		new_test_ext().execute_with(|| {
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
			go_to_interval(2);
			assert_eq!(
				<OnlinePallet as IsOnline>::is_online(&ALICE),
				false,
				"Alice should be offline after 2 heartbeats"
			);
			run_heartbeat_interval(&[ALICE]);
			assert!(
				<OnlinePallet as IsOnline>::is_online(&ALICE),
				"Alice should be back online after submitting one heartbeat"
			);
			run_heartbeat_interval(&[ALICE]);
			assert!(
				<OnlinePallet as IsOnline>::is_online(&ALICE),
				"Alice is still online submitting another heartbeat"
			);
			go_to_interval(6);
			assert_eq!(
				<OnlinePallet as IsOnline>::is_online(&ALICE),
				false,
				"Alice goes offline after two heartbeat intervals"
			);
		});
	}

	#[test]
	fn we_should_see_missing_nodes_when_not_having_submitted_one_interval() {
		new_test_ext().execute_with(|| {
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
			assert!(
				<OnlinePallet as IsOnline>::is_online(&ALICE),
				"Alice should be online"
			);
			go_to_interval(2);
			assert_eq!(
				MockHeartbeat::network_state().awaiting,
				vec![ALICE],
				"Alice should be missing after missing one heartbeat"
			);
			assert_eq!(
				MockHeartbeat::network_state().number_of_nodes,
				1,
				"We should have one node"
			);
		});
	}

	#[test]
	fn we_should_see_offline_nodes_when_not_having_submitted_for_two_intervals() {
		new_test_ext().execute_with(|| {
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
			// Fail to submit for two heartbeats
			go_to_interval(2);
			assert_eq!(
				<OnlinePallet as IsOnline>::is_online(&ALICE),
				false,
				"Alice should be offline"
			);
			assert_eq!(
				MockHeartbeat::network_state().awaiting,
				vec![ALICE],
				"Alice was missing last heartbeat interval"
			);
			go_to_interval(3);
			assert!(
				MockHeartbeat::network_state().online.is_empty(),
				"Alice is not online"
			);
			assert_eq!(
				MockHeartbeat::network_state().number_of_nodes,
				1,
				"We should have one node"
			);
		});
	}

	#[test]
	fn non_validators_should_not_appear_in_network_state() {
		new_test_ext().execute_with(|| {
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(BOB)));
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));

			assert_eq!(
				false,
				MockEpochInfo::is_validator(&BOB),
				"Bob should not be a validator"
			);

			assert_eq!(
				true,
				MockEpochInfo::is_validator(&ALICE),
				"Alice should be a validator"
			);

			assert_eq!(
				<OnlinePallet as IsOnline>::is_online(&BOB),
				true,
				"Bob should be online"
			);

			assert_eq!(
				<OnlinePallet as IsOnline>::is_online(&ALICE),
				true,
				"Alice should be online"
			);

			go_to_interval(3);

			assert_eq!(
				<OnlinePallet as IsOnline>::is_online(&BOB),
				false,
				"Bob should be offline"
			);

			assert_eq!(
				<OnlinePallet as IsOnline>::is_online(&ALICE),
				false,
				"Alice should be offline"
			);

			assert!(
				MockHeartbeat::network_state().online.is_empty(),
				"Alice is now not online"
			);

			assert_eq!(
				MockHeartbeat::network_state().number_of_nodes,
				1,
				"We should have one node"
			);
		});
	}
}
