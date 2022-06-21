use frame_support::sp_runtime::traits::AtLeast32BitUnsigned;
use sp_runtime::helpers_128bit::multiply_by_rational;
use sp_std::{cmp::min, prelude::*};

pub fn calculate_backup_rewards<I, Q>(
	backup_nodes: Vec<(I, Q)>,
	minimum_active_bid: Q,
	heartbeat_block_interval: Q,
	backup_node_emission_per_block: Q,
	current_authority_emission_per_block: Q,
	current_authority_count: Q,
) -> Vec<(I, Q)>
where
	Q: AtLeast32BitUnsigned + From<u128> + Into<u128> + Copy,
	u128: From<Q>,
{
	// Our emission cap for this heartbeat interval
	let emissions_cap = backup_node_emission_per_block.saturating_mul(heartbeat_block_interval);

	// Emissions for this heartbeat interval for the active set
	let authority_rewards =
		current_authority_emission_per_block.saturating_mul(heartbeat_block_interval);

	// The average authority emission
	let average_authority_reward = authority_rewards.checked_div(&current_authority_count).unwrap();

	let mut total_rewards: Q = Q::from(0_u128);

	// Calculate rewards for each backup node and total rewards for capping
	let mut rewards: Vec<_> = backup_nodes
		.into_iter()
		.map(|(backup_node, backup_node_stake)| {
			let reward = min(
				average_authority_reward,
				average_authority_reward
					.saturating_mul(u128::from(backup_node_stake).pow(2).into())
					.checked_div(&u128::from(minimum_active_bid).pow(2).into())
					.unwrap(),
			)
			.saturating_mul(Q::from(8_u128))
			.checked_div(&Q::from(10_u128))
			.unwrap();
			total_rewards += reward;
			(backup_node, reward)
		})
		.collect();

	// Cap if needed
	if total_rewards > emissions_cap {
		rewards = rewards
			.into_iter()
			.map(|(validator_id, reward)| {
				(
					validator_id,
					Q::from(
						multiply_by_rational(
							reward.into(),
							emissions_cap.into(),
							total_rewards.into(),
						)
						.unwrap_or_default(),
					),
				)
			})
			.collect();
	}
	rewards
}
