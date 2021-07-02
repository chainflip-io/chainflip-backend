use crate::{mock::*, Pallet};
use pallet_cf_flip::Pallet as Flip;
use tuples::Tuple2Map;

macro_rules! balance_totals {
	( $( $acct:literal ),+ ) => {
		(
			$(
				Flip::<Test>::total_balance_of(&$acct),
			)+
		)
	};
}

#[test]
fn test_should_mint() {
	// If mint_frequency is zero, we mint on every block.
	assert!(Pallet::<Test>::should_mint(0, 0) == true);
	assert!(Pallet::<Test>::should_mint(1, 0) == true);
	// If not enough blocks have elapsed we don't mint.
	assert!(Pallet::<Test>::should_mint(0, 1) == false);
	// If we are at or above the mint frequency, we mint.
	assert!(Pallet::<Test>::should_mint(1, 1) == true);
	assert!(Pallet::<Test>::should_mint(2, 1) == true);
}

#[test]
fn test_should_mint_at() {
	new_test_ext(vec![], None, None).execute_with(|| {
		// It has been `MINT_FREQUENCY` blocks since the last mint.
		assert_eq!(Pallet::<Test>::should_mint_at(MINT_FREQUENCY).0, true);
		// It hasn't yet been `MINT_FREQUENCY` blocks since the last mint.
		assert_eq!(Pallet::<Test>::should_mint_at(MINT_FREQUENCY - 1).0, false);
		// It has been more than `MINT_FREQUENCY` blocks since the last mint.
		assert_eq!(Pallet::<Test>::should_mint_at(MINT_FREQUENCY + 1).0, true);
		// We have literally *just* minted.
		assert_eq!(Pallet::<Test>::should_mint_at(0).0, false);
	});
}

#[cfg(test)]
mod test_distribution {
	use super::*;

	fn test_with(reward_amount: u128, expected_share: u128, expected_remainder: u128) {
		new_test_ext(vec![1, 2], None, None).execute_with(|| {
			let before = balance_totals!(1, 2);
			let remainder = Pallet::<Test>::distribute_to_validators(reward_amount);
			let after = balance_totals!(1, 2);

			assert_eq!(before.map(|v| v + expected_share), after);
			assert_eq!(expected_remainder, remainder);
		});
	}

	#[test]
	fn test_even() {
		test_with(100, 50, 0);
	}

	#[test]
	fn test_with_remainder() {
		test_with(101, 50, 1);
	}

	#[test]
	fn test_with_zero() {
		test_with(0, 0, 0);
	}

	#[test]
	fn test_with_insufficient_amount() {
		test_with(1, 0, 1);
	}
}

#[cfg(test)]
mod test_block_rewards {
	use super::*;

	fn test_with(
		block_number: u64,
		emissions_per_block: u128,
		expected_mint: u128,
		expected_dust: u128,
	) {
		new_test_ext(vec![1, 2], Some(1000), Some(emissions_per_block)).execute_with(|| {
			let before = Flip::<Test>::total_issuance();
			let _weights = Pallet::<Test>::mint_rewards_for_block(block_number);
			let after = Flip::<Test>::total_issuance();

			assert_eq!(before + expected_mint, after);
			assert_eq!(expected_dust, Pallet::<Test>::dust());
		});
	}

	#[test]
	fn test_zero_block() {
		test_with(0, 1, 0, 0);
	}

	#[test]
	fn test_zero_emissions_rate() {
		test_with(1, 0, 0, 0);
	}

	#[test]
	fn test_even_with_non_zero() {
		test_with(5, 10, 50, 0);
	}

	#[test]
	fn test_uneven_with_non_zero() {
		test_with(5, 11, 54, 1);
	}
}

#[test]
fn test_block_time_conversion() {
	new_test_ext(vec![], None, None).execute_with(|| {
		// Our blocks are twice as a fast (half the time) so emission rate should be half.
		assert_eq!(Pallet::<Test>::convert_emissions_rate(1000u128), 500u128);
		assert_eq!(Pallet::<Test>::convert_emissions_rate(1001u128), 500u128);
		assert_eq!(Pallet::<Test>::convert_emissions_rate(0u128), 0u128);
	});
}
