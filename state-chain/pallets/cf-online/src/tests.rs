use crate::mock::*;
use cf_traits::{EpochInfo, IsOnline};
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

		let current_epoch = MockEpochInfo::epoch_index();
		assert!(
			!MockEpochInfo::authority_index(current_epoch, &BOB).is_some(),
			"Bob should not be an authority"
		);

		assert!(
			MockEpochInfo::authority_index(current_epoch, &ALICE).is_some(),
			"Alice should be an authority"
		);

		assert!(<OnlinePallet as IsOnline>::is_online(&BOB), "Bob should be online");

		assert!(<OnlinePallet as IsOnline>::is_online(&ALICE), "Alice should be online");

		go_to_interval(3);

		assert!(!<OnlinePallet as IsOnline>::is_online(&BOB), "Bob should be offline");

		assert!(!<OnlinePallet as IsOnline>::is_online(&ALICE), "Alice should be offline");

		assert!(MockHeartbeat::network_state().online.is_empty(), "Alice is now not online");

		assert_eq!(MockHeartbeat::network_state().number_of_nodes(), 1, "We should have one node");
	});
}
