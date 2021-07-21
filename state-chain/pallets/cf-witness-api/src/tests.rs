use crate::mock::*;
use frame_support::{assert_noop, assert_ok};

#[cfg(test)]
mod staking_witness_tests {
	use super::*;
	const ETH_TX_HASH: [u8; 32] = [0; 32];
	const STAKE: u128 = 100;
	const STAKER: u64 = 12345;
	const WITNESS: u64 = 67890;

	#[test]
	fn test_staked() {
		new_test_ext().execute_with(|| {
			MockWitnesser::set_threshold(2);

			// The call we are witnessing.
			let staked_call: Call =
				pallet_cf_staking::Call::staked(STAKER, STAKE, ETH_TX_HASH).into();
			
			// One vote.
			assert_ok!(WitnessApi::witness_staked(
				Origin::signed(WITNESS),
				STAKER,
				STAKE,
				ETH_TX_HASH
			));

			assert_eq!(MockWitnesser::get_vote_count_for(&staked_call), 1);

			// Another.
			assert_ok!(WitnessApi::witness_staked(
				Origin::signed(WITNESS),
				STAKER,
				STAKE,
				ETH_TX_HASH
			));
			
			assert_eq!(MockWitnesser::get_vote_count_for(&staked_call), 2);

			// Check the result.
			assert_eq!(MockStakeTransfer::get_balance(STAKER), STAKE);
		});
	}
}
