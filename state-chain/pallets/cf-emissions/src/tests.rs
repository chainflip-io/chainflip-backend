use crate::{mock::*, BlockEmissions, LastSupplyUpdateBlock, Pallet};
use cf_traits::{
	mocks::{egress_handler::MockEgressHandler, system_state_info::MockSystemStateInfo},
	RewardsDistribution,
};
use frame_support::traits::OnInitialize;
use pallet_cf_flip::Pallet as Flip;

use cf_chains::AnyChain;

type Emissions = Pallet<Test>;

#[test]
fn test_should_mint_at() {
	new_test_ext(vec![], None).execute_with(|| {
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
		new_test_ext(vec![1, 2], Some(1000)).execute_with(|| {
			Emissions::update_authority_block_emission(emissions_per_block);

			let before = Flip::<Test>::total_issuance();
			MockRewardsDistribution::distribute();
			let after = Flip::<Test>::total_issuance();

			assert_eq!(before + emissions_per_block, after);
		});
	}

	#[test]
	fn test_zero_block() {
		test_with(1);
	}

	#[test]
	fn test_zero_emissions_rate() {
		test_with(0);
	}

	#[test]
	fn test_non_zero_rate() {
		test_with(10);
	}
}

#[test]
fn test_duplicate_emission_should_be_noop() {
	new_test_ext(vec![1, 2], None).execute_with(|| {
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
	new_test_ext(vec![1, 2], None).execute_with(|| {
		// Block emissions are calculated at genesis.
		assert!(Emissions::current_authority_emission_per_block() > 0);
		assert!(Emissions::backup_node_emission_per_block() > 0);
	});
}

#[test]
fn should_mint_but_not_broadcast() {
	new_test_ext(vec![1, 2], None).execute_with(|| {
		let prev_supply_update_block = LastSupplyUpdateBlock::<Test>::get();
		MockRewardsDistribution::distribute();
		assert_eq!(prev_supply_update_block, LastSupplyUpdateBlock::<Test>::get());
	});
}

#[test]
fn should_mint_and_initiate_broadcast() {
	new_test_ext(vec![1, 2], None).execute_with(|| {
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
fn no_update_of_update_total_supply_during_maintanance() {
	new_test_ext(vec![1, 2], None).execute_with(|| {
		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		// Try send a broadcast to update the total supply
		Emissions::on_initialize(SUPPLY_UPDATE_INTERVAL.into());
		// Expect nothing to be sent
		assert!(MockBroadcast::get_called().is_none());
		// Deactivate maintenance mode
		MockSystemStateInfo::set_maintenance(false);
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

#[test]
fn burn_flip() {
	new_test_ext(vec![1, 2], None).execute_with(|| {
		Emissions::on_initialize(SUPPLY_UPDATE_INTERVAL.into());
		assert_eq!(
			MockBroadcast::get_called().unwrap().new_total_supply,
			Flip::<Test>::total_issuance()
		);
		let egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
		assert!(egresses.len() == 1);
		assert_eq!(egresses.first().expect("should exist").1, FLIP_TO_BURN);
	});
}
