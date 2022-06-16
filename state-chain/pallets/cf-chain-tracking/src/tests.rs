use crate::{mock::*, Error};
use cf_chains::eth::TrackedData;
use frame_support::{assert_noop, assert_ok};

#[test]
fn test_update_chain_state() {
	new_test_ext().execute_with(|| {
		assert_ok!(MockChainTracking::update_chain_state(
			Origin::signed(0),
			TrackedData { block_height: 1000, base_fee: 20, priority_fee: 2 }
		));
		assert_noop!(
			MockChainTracking::update_chain_state(
				Origin::signed(0),
				TrackedData {
					block_height: 1000 - SAFE_BLOCK_MARGIN,
					base_fee: 20,
					priority_fee: 2
				}
			),
			Error::<Test>::StaleDataSubmitted
		);
		assert_ok!(MockChainTracking::update_chain_state(
			Origin::signed(0),
			TrackedData {
				block_height: 1000 - SAFE_BLOCK_MARGIN + 1,
				base_fee: 20,
				priority_fee: 2
			}
		));
		assert_ok!(MockChainTracking::update_chain_state(
			Origin::signed(0),
			TrackedData { block_height: 1100, base_fee: 20, priority_fee: 2 }
		));
	})
}
