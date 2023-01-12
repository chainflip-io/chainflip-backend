use crate::{mock::*, Error};
use cf_chains::mocks::MockTrackedData;
use frame_support::{assert_noop, assert_ok};

#[test]
fn test_update_chain_state_within_age_limit() {
	new_test_ext().execute_with(|| {
		const LATEST_BLOCK: u64 = 1000;
		assert_ok!(MockChainTracking::update_chain_state(
			RuntimeOrigin::signed(0),
			MockTrackedData(LATEST_BLOCK)
		));
		assert_ok!(MockChainTracking::update_chain_state(
			RuntimeOrigin::signed(0),
			MockTrackedData(LATEST_BLOCK - AGE_LIMIT + 1)
		));
		assert_ok!(MockChainTracking::update_chain_state(
			RuntimeOrigin::signed(0),
			MockTrackedData(LATEST_BLOCK + AGE_LIMIT * 100)
		));
	})
}

#[test]
fn test_update_chain_state_outside_of_age_limit() {
	new_test_ext().execute_with(|| {
		const LATEST_BLOCK: u64 = 1000;
		assert_ok!(MockChainTracking::update_chain_state(
			RuntimeOrigin::signed(0),
			MockTrackedData(LATEST_BLOCK)
		));
		assert_noop!(
			MockChainTracking::update_chain_state(
				RuntimeOrigin::signed(0),
				MockTrackedData(LATEST_BLOCK - AGE_LIMIT)
			),
			Error::<Test>::StaleDataSubmitted
		);
		assert_noop!(
			MockChainTracking::update_chain_state(RuntimeOrigin::signed(0), MockTrackedData(0)),
			Error::<Test>::StaleDataSubmitted
		);
	})
}
