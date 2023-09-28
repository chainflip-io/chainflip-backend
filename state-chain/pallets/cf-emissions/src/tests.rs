#![cfg(test)]

use crate::{mock::*, BlockEmissions, LastSupplyUpdateBlock, Pallet};
use cf_primitives::SECONDS_PER_BLOCK;
use cf_traits::{mocks::egress_handler::MockEgressHandler, RewardsDistribution, SetSafeMode};
use frame_support::traits::OnInitialize;
use pallet_cf_flip::Pallet as Flip;

use cf_chains::AnyChain;

type Emissions = Pallet<Test>;

#[test]
fn test_should_mint_at() {
	new_test_ext().execute_with(|| {
		// It has been `SUPPLY_UPDATE_INTERVAL` blocks since the last broadcast.
		assert!(Emissions::should_update_supply_at(SUPPLY_UPDATE_INTERVAL.into()));
		// It hasn't yet been `SUPPLY_UPDATE_INTERVAL` blocks since the last broadcast.
		assert!(!Emissions::should_update_supply_at((SUPPLY_UPDATE_INTERVAL - 1).into()));
		// It has been more than `SUPPLY_UPDATE_INTERVAL` blocks since the last broadcast.
		assert!(Emissions::should_update_supply_at((SUPPLY_UPDATE_INTERVAL + 1).into()));
		// We have literally *just* broadcasted.
		assert!(!Emissions::should_update_supply_at(0));
	});
}

#[cfg(test)]
mod test_block_rewards {
	use cf_traits::RewardsDistribution;

	use super::*;

	fn test_with(emissions_per_block: u128) {
		new_test_ext().execute_with(|| {
			Emissions::update_authority_block_emission(emissions_per_block);

			let before = Flip::<Test>::total_issuance();
			MockRewardsDistribution::distribute();
			let after = Flip::<Test>::total_issuance();

			assert_eq!(before + emissions_per_block, after);
		});
	}

	#[test]
	fn test_emissions_rates() {
		test_with(0);
		test_with(1);
		test_with(TOTAL_ISSUANCE / 100_000_000);
		test_with(TOTAL_ISSUANCE / 100_000);
	}
}

#[test]
fn test_duplicate_emission_should_be_noop() {
	new_test_ext().execute_with(|| {
		Emissions::update_authority_block_emission(EMISSION_RATE);

		let before = Flip::<Test>::total_issuance();
		MockRewardsDistribution::distribute();
		let after = Flip::<Test>::total_issuance();

		assert_eq!(before + EMISSION_RATE, after);

		// Minting again at the same block should have no effect.
		let before = after;
		MockRewardsDistribution::distribute();
		let after = Flip::<Test>::total_issuance();

		assert_eq!(before + EMISSION_RATE, after);
	});
}

#[test]
fn should_calculate_block_emissions() {
	new_test_ext().execute_with(|| {
		// Block emissions are calculated at genesis.
		assert!(Emissions::current_authority_emission_per_block() > 0);
		assert!(Emissions::backup_node_emission_per_block() > 0);
	});
}

#[test]
fn should_mint_but_not_broadcast() {
	new_test_ext().execute_with(|| {
		let prev_supply_update_block = LastSupplyUpdateBlock::<Test>::get();
		MockRewardsDistribution::distribute();
		assert_eq!(prev_supply_update_block, LastSupplyUpdateBlock::<Test>::get());
	});
}

#[test]
fn should_mint_and_initiate_broadcast() {
	new_test_ext().execute_with(|| {
		let before = Flip::<Test>::total_issuance();
		assert!(MockBroadcast::get_called().is_none());
		Emissions::on_initialize(SUPPLY_UPDATE_INTERVAL.into());
		let after = Flip::<Test>::total_issuance();
		assert!(after > before, "Expected {after:?} > {before:?}");
		assert_eq!(
			MockBroadcast::get_called().unwrap().new_total_supply,
			Flip::<Test>::total_issuance()
		);
	});
}

#[test]
fn no_update_of_update_total_supply_during_safe_mode_code_red() {
	new_test_ext().execute_with(|| {
		// Activate code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		// Try send a broadcast to update the total supply
		Emissions::on_initialize(SUPPLY_UPDATE_INTERVAL.into());
		// Expect nothing to be sent
		assert!(MockBroadcast::get_called().is_none());
		// Deactivate code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();
		// Try send a broadcast to update the total supply
		Emissions::on_initialize((SUPPLY_UPDATE_INTERVAL * 2).into());
		// Expect the broadcast to be sendt
		assert_eq!(
			MockBroadcast::get_called().unwrap().new_total_supply,
			Flip::<Test>::total_issuance()
		);
	});
}

#[test]
fn test_example_block_reward_calcaulation() {
	use crate::calculate_inflation_to_block_reward;
	let issuance: u128 = 100_000_000_000_000_000_000_000_000; // 100m Flip
	let inflation: u128 = 2720; // perbill
	let expected: u128 = 1_813_333_333_333_333_333;
	assert_eq!(calculate_inflation_to_block_reward(issuance, inflation, 150u128), expected);
}

const BLOCKS_PER_YEAR: u64 = (365 * 24 + 6) * 60 * 60 / SECONDS_PER_BLOCK;

#[test]
fn rewards_calculation_compounding() {
	const INITIAL_ISSUANCE: u128 = 100_000_000_000_000_000_000_000_000; // 100m Flip

	let mut total_issuance: u128 = INITIAL_ISSUANCE;
	const TARGET_ANNUAL_INFLATION: f64 = 0.001; // 0.1%
	const COMPOUNDING_INTERVAL: u64 = 150;

	for _ in 0..(BLOCKS_PER_YEAR / COMPOUNDING_INTERVAL) {
		let block_reward = crate::calculate_inflation_to_block_reward(
			total_issuance,
			chainflip_node::chain_spec::common::CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL.into(),
			COMPOUNDING_INTERVAL as u128,
		);

		// For `COMPOUNDING_INTERVAL` blocks, block reward is the same
		total_issuance += block_reward * COMPOUNDING_INTERVAL as u128;
	}

	let minted_actual = total_issuance.checked_sub(INITIAL_ISSUANCE).unwrap();
	let inflation_actual = minted_actual as f64 / INITIAL_ISSUANCE as f64;
	let error = inflation_actual / TARGET_ANNUAL_INFLATION;

	assert!(error > 0.98 && error < 1.02);
}

#[test]
fn burn_flip() {
	new_test_ext().execute_with(|| {
		Emissions::on_initialize(SUPPLY_UPDATE_INTERVAL.into());
		assert_eq!(
			MockBroadcast::get_called().unwrap().new_total_supply,
			Flip::<Test>::total_issuance()
		);
		let egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
		assert!(egresses.len() == 1);
		assert_eq!(egresses.first().expect("should exist").amount(), FLIP_TO_BURN);
	});
}
