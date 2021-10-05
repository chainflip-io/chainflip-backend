use crate::{mock::*, BlockEmissions, Pallet};
use pallet_cf_flip::Pallet as Flip;

type Emissions = Pallet<Test>;

#[test]
fn test_should_mint() {
	// If mint_interval is zero, we mint on every block.
	assert!(Emissions::should_mint(0, 0) == true);
	assert!(Emissions::should_mint(1, 0) == true);
	// If not enough blocks have elapsed we don't mint.
	assert!(Emissions::should_mint(0, 1) == false);
	// If we are at or above the mint interval, we mint.
	assert!(Emissions::should_mint(1, 1) == true);
	assert!(Emissions::should_mint(2, 1) == true);
}

#[test]
fn test_should_mint_at() {
	new_test_ext(vec![], None).execute_with(|| {
		// It has been `MINT_INTERVAL` blocks since the last mint.
		assert_eq!(Emissions::should_mint_at(MINT_INTERVAL).0, true);
		// It hasn't yet been `MINT_INTERVAL` blocks since the last mint.
		assert_eq!(Emissions::should_mint_at(MINT_INTERVAL - 1).0, false);
		// It has been more than `MINT_INTERVAL` blocks since the last mint.
		assert_eq!(Emissions::should_mint_at(MINT_INTERVAL + 1).0, true);
		// We have literally *just* minted.
		assert_eq!(Emissions::should_mint_at(0).0, false);
	});
}

#[cfg(test)]
mod test_block_rewards {
	use super::*;

	fn test_with(block_number: u64, emissions_per_block: u128, expected_mint: u128) {
		new_test_ext(vec![1, 2], Some(1000)).execute_with(|| {
			Emissions::update_validator_block_emission(emissions_per_block);

			let before = Flip::<Test>::total_issuance();
			let _weights = Emissions::mint_rewards_for_block(block_number);
			let after = Flip::<Test>::total_issuance();

			assert_eq!(before + expected_mint, after);
		});
	}

	#[test]
	fn test_zero_block() {
		test_with(0, 1, 0);
	}

	#[test]
	fn test_zero_emissions_rate() {
		test_with(1, 0, 0);
	}

	#[test]
	fn test_non_zero_rate() {
		test_with(5, 10, 50);
	}
}

#[test]
fn test_duplicate_emission_should_be_noop() {
	const EMISSION_RATE: u128 = 10;

	new_test_ext(vec![1, 2], None).execute_with(|| {
		const BLOCK_NUMBER: u64 = 5;

		Emissions::update_validator_block_emission(EMISSION_RATE);

		let before = Flip::<Test>::total_issuance();
		let _weights = Emissions::mint_rewards_for_block(BLOCK_NUMBER);
		let after = Flip::<Test>::total_issuance();

		assert_eq!(before + EMISSION_RATE * BLOCK_NUMBER as u128, after);

		// Minting again at the same block should have no effect.
		let before = after;
		let _weights = Emissions::mint_rewards_for_block(BLOCK_NUMBER);
		let after = Flip::<Test>::total_issuance();

		assert_eq!(before, after);
	});
}

#[test]
fn should_calculate_block_emissions() {
	new_test_ext(vec![1, 2], None).execute_with(|| {
		// At genesis we have no emissions calculated
		assert_eq!(Emissions::validator_emission_per_block(), 0);
		assert_eq!(Emissions::backup_validator_emission_per_block(), 0);
		Emissions::calculate_block_emissions();
		// Emissions updated in storage
		assert!(Emissions::validator_emission_per_block() > 0);
		assert!(Emissions::backup_validator_emission_per_block() > 0);
		// The emissions for validators should be `ValidatorEmissionInflation` times greater
		assert_eq!(
			Emissions::validator_emission_per_block() / ValidatorEmissionInflation::get() as u128,
			Emissions::backup_validator_emission_per_block()
		);
	});
}
