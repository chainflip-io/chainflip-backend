use crate::mock::*;
use cf_traits::{offence_reporting::Banned, EpochInfo, IsOnline, KeygenExclusionSet};
use frame_support::assert_ok;

// Move forward one heartbeat interval sending the heartbeat extrinsic for nodes
fn submit_heartbeat_for_current_interval(nodes: &[<Test as frame_system::Config>::AccountId]) {
	let start_block_number = System::block_number();
	for node in nodes {
		assert_ok!(OnlinePallet::heartbeat(Origin::signed(*node)));
	}
	run_to_block(start_block_number + HEARTBEAT_BLOCK_INTERVAL - 1);
}

// Move a heartbeat intervals forward with no heartbeat sent
fn go_to_interval(interval: u64) {
	run_to_block((interval * HEARTBEAT_BLOCK_INTERVAL) + 1);
}

#[test]
fn should_exclude_keygen_failures() {
	new_test_ext().execute_with(|| {
		<OnlinePallet as KeygenExclusionSet>::add_to_set(ALICE);
		assert!(<OnlinePallet as KeygenExclusionSet>::is_excluded(&ALICE));
		assert!(!<OnlinePallet as KeygenExclusionSet>::is_excluded(&BOB));
		<OnlinePallet as KeygenExclusionSet>::forgive_all();
		assert!(!<OnlinePallet as KeygenExclusionSet>::is_excluded(&ALICE));
	});
}

#[test]
fn submitting_heartbeat_more_than_once_in_an_interval() {
	new_test_ext().execute_with(|| {
		assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
		assert!(<OnlinePallet as IsOnline>::is_online(&ALICE), "Alice should be online");
		assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
		assert!(<OnlinePallet as IsOnline>::is_online(&ALICE), "Alice should be online");
		go_to_interval(1);
		assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
	});
}

#[test]
fn we_should_be_online_when_submitting_heartbeats_and_offline_when_not() {
	new_test_ext().execute_with(|| {
		assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
		go_to_interval(1);
		assert!(!<OnlinePallet as IsOnline>::is_online(&ALICE), "Alice should be offline");
		submit_heartbeat_for_current_interval(&[ALICE]);
		assert!(
			<OnlinePallet as IsOnline>::is_online(&ALICE),
			"Alice should be back online after submitting heartbeat"
		);
		go_to_interval(2);
		assert!(
			!<OnlinePallet as IsOnline>::is_online(&ALICE),
			"Alice goes offline after two heartbeat intervals"
		);
	});
}

#[test]
fn we_should_see_missing_nodes_when_not_having_submitted_one_interval() {
	new_test_ext().execute_with(|| {
		assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
		assert!(<OnlinePallet as IsOnline>::is_online(&ALICE), "Alice should be online");
		go_to_interval(2);
		assert_eq!(
			MockHeartbeat::network_state().offline,
			vec![ALICE],
			"Alice should be offline after missing one heartbeat"
		);
		assert_eq!(MockHeartbeat::network_state().number_of_nodes(), 1, "We should have one node");
	});
}

#[test]
fn non_validators_should_not_appear_in_network_state() {
	new_test_ext().execute_with(|| {
		assert_ok!(OnlinePallet::heartbeat(Origin::signed(BOB)));
		assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));

		assert!(!MockEpochInfo::is_validator(&BOB), "Bob should not be a validator");

		assert!(MockEpochInfo::is_validator(&ALICE), "Alice should be a validator");

		assert!(<OnlinePallet as IsOnline>::is_online(&BOB), "Bob should be online");

		assert!(<OnlinePallet as IsOnline>::is_online(&ALICE), "Alice should be online");

		go_to_interval(3);

		assert!(!<OnlinePallet as IsOnline>::is_online(&BOB), "Bob should be offline");

		assert!(!<OnlinePallet as IsOnline>::is_online(&ALICE), "Alice should be offline");

		assert!(MockHeartbeat::network_state().online.is_empty(), "Alice is now not online");

		assert_eq!(MockHeartbeat::network_state().number_of_nodes(), 1, "We should have one node");
	});
}

#[test]
fn submitting_heartbeats_should_not_lift_ban() {
	new_test_ext().execute_with(|| {
		// Ban Alice
		<OnlinePallet as Banned>::ban(&ALICE);
		// Send a series of heartbeats over N blocks
		let number_of_blocks = 10;
		for block_number in 1..=number_of_blocks {
			assert_ok!(OnlinePallet::heartbeat(Origin::signed(ALICE)));
			run_to_block(block_number);
			assert!(!<OnlinePallet as IsOnline>::is_online(&ALICE));
		}
		// Move to next interval
		go_to_interval(1);
		// Alice should be online
		assert!(<OnlinePallet as IsOnline>::is_online(&ALICE));
	});
}
