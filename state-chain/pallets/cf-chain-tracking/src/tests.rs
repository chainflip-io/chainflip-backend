use crate::{mock::*, Error};
use cf_chains::mocks::MockTrackedData;
use frame_support::{assert_noop, assert_ok};

#[test]
fn test_update_chain_state() {
	new_test_ext().execute_with(|| {
		assert_ok!(MockChainTracking::update_chain_state(Origin::signed(0), MockTrackedData(1000)));
		assert_noop!(
			MockChainTracking::update_chain_state(
				Origin::signed(0),
				MockTrackedData(1000 - SAFE_BLOCK_MARGIN)
			),
			Error::<Test>::StaleDataSubmitted
		);
		assert_ok!(MockChainTracking::update_chain_state(
			Origin::signed(0),
			MockTrackedData(1000 - SAFE_BLOCK_MARGIN + 1)
		));
		assert_ok!(MockChainTracking::update_chain_state(Origin::signed(0), MockTrackedData(1100)));
	})
}
