use cf_chains::eth::TrackedData;
use frame_support::assert_ok;

use crate::mock::{new_test_ext, MockChainTracking, Origin};

#[test]
fn test_update_chain_state() {
	new_test_ext().execute_with(|| {
		let dummy_data = TrackedData { block_height: 1000, base_fee: 20, priority_fee: 2 };

		assert_ok!(MockChainTracking::update_chain_state(Origin::signed(0), dummy_data));
	})
}
