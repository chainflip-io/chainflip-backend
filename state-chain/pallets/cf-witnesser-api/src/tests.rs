use crate::mock::*;
use frame_support::{assert_noop, assert_ok};

#[cfg(test)]
mod staking_witness_tests {
	use sp_core::U256;

	use super::*;
	const ETH_TX_HASH: [u8; 32] = [0; 32];
	const RETURN_ADDRESS: Option<[u8; 20]> = None;
	const STAKE: u128 = 100;
	const STAKER: u64 = 12345;
	const WITNESS: u64 = 67890;
	const DUMMY_MSG: U256 = U256::zero();

	#[test]
	fn test_staked() {
		new_test_ext().execute_with(|| {
			MockWitnesser::set_threshold(2);

			// The call we are witnessing.
			let call: Call =
				pallet_cf_staking::Call::staked(STAKER, STAKE, RETURN_ADDRESS, ETH_TX_HASH).into();

			// One vote.
			assert_ok!(WitnessApi::witness_staked(
				Origin::signed(WITNESS),
				STAKER,
				STAKE,
				RETURN_ADDRESS,
				ETH_TX_HASH
			));

			assert_eq!(MockWitnesser::get_vote_count_for(&call), 1);

			// Another.
			assert_ok!(WitnessApi::witness_staked(
				Origin::signed(WITNESS),
				STAKER,
				STAKE,
				RETURN_ADDRESS,
				ETH_TX_HASH
			));

			assert_eq!(MockWitnesser::get_vote_count_for(&call), 2);

			// Check the result.
			assert_eq!(MockStakeTransfer::get_balance(STAKER), STAKE);
		});
	}

	#[test]
	fn test_claimed() {
		new_test_ext().execute_with(|| {
			MockWitnesser::set_threshold(2);

			// The call we are witnessing.
			let call: Call = pallet_cf_staking::Call::claimed(STAKER, STAKE, ETH_TX_HASH).into();

			// One vote.
			assert_ok!(WitnessApi::witness_claimed(
				Origin::signed(WITNESS),
				STAKER,
				STAKE,
				ETH_TX_HASH
			));

			assert_eq!(MockWitnesser::get_vote_count_for(&call), 1);

			// Another. Should fail since we haven't registered any claims.
			assert_noop!(
				WitnessApi::witness_claimed(Origin::signed(WITNESS), STAKER, STAKE, ETH_TX_HASH),
				pallet_cf_staking::Error::<Test>::NoPendingClaim
			);

			assert_eq!(MockWitnesser::get_vote_count_for(&call), 2);
		});
	}

	#[test]
	fn test_post_claim_signature() {
		use cf_chains::eth;
		const DUMMY_SIG: eth::SchnorrSignature = eth::SchnorrSignature {
			s: [0xcf; 32],
			k_times_g_addr: [0xcf; 20],
		};

		new_test_ext().execute_with(|| {
			MockWitnesser::set_threshold(2);

			// The call we are witnessing.
			let call: Call =
				pallet_cf_staking::Call::post_claim_signature(STAKER, DUMMY_SIG).into();

			// One vote.
			assert_ok!(WitnessApi::witness_post_claim_signature(
				Origin::signed(WITNESS),
				STAKER,
				DUMMY_SIG,
			));

			assert_eq!(MockWitnesser::get_vote_count_for(&call), 1);

			// Second vote - fails because there is no claim when it gets executed - but thats okay
			assert_noop!(
				WitnessApi::witness_post_claim_signature(
					Origin::signed(WITNESS),
					STAKER,
					DUMMY_SIG,
				),
				pallet_cf_staking::Error::<Test>::NoPendingClaim
			);

			assert_eq!(MockWitnesser::get_vote_count_for(&call), 2);
		});
	}
}
