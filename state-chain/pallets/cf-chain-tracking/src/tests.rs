use crate::{mock::*, Error};
use cf_chains::mocks::MockTrackedData;
use frame_support::{assert_noop, assert_ok};

#[test]
fn test_update_chain_state_within_safety_margin() {
	new_test_ext().execute_with(|| {
		const LATEST_BLOCK: u64 = 1000;
		assert_ok!(MockChainTracking::update_chain_state(
			Origin::signed(0),
			MockTrackedData(LATEST_BLOCK)
		));
		assert_ok!(MockChainTracking::update_chain_state(
			Origin::signed(0),
			MockTrackedData(LATEST_BLOCK - SAFE_BLOCK_MARGIN)
		));
		assert_ok!(MockChainTracking::update_chain_state(
			Origin::signed(0),
			MockTrackedData(LATEST_BLOCK + SAFE_BLOCK_MARGIN * 100)
		));
	})
}

#[test]
fn test_update_chain_state_outside_of_safety_margin() {
	new_test_ext().execute_with(|| {
		const LATEST_BLOCK: u64 = 1000;
		assert_ok!(MockChainTracking::update_chain_state(
			Origin::signed(0),
			MockTrackedData(LATEST_BLOCK)
		));
		assert_noop!(
			MockChainTracking::update_chain_state(
				Origin::signed(0),
				MockTrackedData(LATEST_BLOCK - SAFE_BLOCK_MARGIN - 1)
			),
			Error::<Test>::SafeDataSubmitted
		);
		assert_noop!(
			MockChainTracking::update_chain_state(Origin::signed(0), MockTrackedData(0)),
			Error::<Test>::SafeDataSubmitted
		);
	})
}
