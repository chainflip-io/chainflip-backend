use cf_traits::Bid;
use frame_support::sp_runtime::{helpers_128bit::multiply_by_rational_with_rounding, Rounding};
use sp_std::{cmp::min, prelude::*};

//TODO: The u128 is not big enough for some calculations (for example this one) which involve
// intermediate steps of the calculation create values that saturate the u128. In this and in
// similar cases we might have to convert the values to BigInt for calculation and then convert it
// back to u128 after calculation. In this case, the saturation problem can lead to upto 0.03 - 0.05
// Flip error in calculation.

pub fn calculate_backup_rewards<Id, Amount>(
	backup_nodes: Vec<Bid<Id, u128>>,
	current_epoch_bond: u128,
	reward_interval: u128,
	backup_node_emission_per_block: u128,
	current_authority_emission_per_block: u128,
	current_authority_count: u128,
) -> Vec<(Id, Amount)>
where
	Amount: From<u128>,
{
	const QUANTISATION_FACTOR: u128 = 100_000_000;

	let (bond, backup_node_emission_per_block, current_authority_emission_per_block) = (
		current_epoch_bond / QUANTISATION_FACTOR,
		backup_node_emission_per_block / QUANTISATION_FACTOR,
		current_authority_emission_per_block / QUANTISATION_FACTOR,
	);

	// Emissions for this heartbeat interval for the active set
	let authority_rewards = current_authority_emission_per_block.saturating_mul(reward_interval);

	// The average authority emission
	let average_authority_reward = authority_rewards
		.checked_div(current_authority_count)
		.expect("we always have more than one authority");

	let mut total_rewards = 0_u128;

	// Calculate rewards for each backup node and total rewards for capping
	let rewards: Vec<_> = backup_nodes
		.into_iter()
		.map(|Bid { bidder_id, amount }| {
			let backup_balance = amount / QUANTISATION_FACTOR;
			let reward = min(
				average_authority_reward,
				multiply_by_rational_with_rounding(
					average_authority_reward.saturating_mul(backup_balance),
					backup_balance,
					bond,
					Rounding::Down,
				)
				.unwrap_or(u128::MAX) // behaves like saturating mul
				.checked_div(bond)
				.unwrap(),
			)
			.saturating_mul(8_u128)
			.checked_div(10_u128)
			.unwrap();
			total_rewards += reward;
			(bidder_id, reward)
		})
		.collect();

	// Our emission cap for this heartbeat interval
	let emissions_cap = backup_node_emission_per_block.saturating_mul(reward_interval);

	// Cap if needed
	if total_rewards > emissions_cap {
		rewards
			.into_iter()
			.map(|(id, reward)| {
				(
					id,
					multiply_by_rational_with_rounding(
						reward,
						emissions_cap,
						total_rewards,
						sp_runtime::Rounding::Up,
					)
					.unwrap_or_default(),
				)
			})
			.map(|(id, reward)| (id, (reward.saturating_mul(QUANTISATION_FACTOR)).into()))
			.collect()
	} else {
		rewards
			.into_iter()
			.map(|(id, reward)| (id, (reward.saturating_mul(QUANTISATION_FACTOR)).into()))
			.collect()
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use crate::constants::common::FLIPPERINOS_PER_FLIP;

	const AUTHORITY_COUNT: u128 = 100;
	const ANNUAL_BACKUP_REWARDS_CAP: u128 = 100_000;
	const BOND: u128 = 100_000;

	fn annual_backup_rewards(
		backup_amounts: &[u128],
		annual_authority_emissions: u128,
	) -> Vec<(usize, u128)> {
		const BLOCKS_PER_YEAR: u128 = 14_400 * 365;

		let backup_emissions_cap_per_block =
			ANNUAL_BACKUP_REWARDS_CAP * FLIPPERINOS_PER_FLIP / BLOCKS_PER_YEAR;
		let authority_emissions_per_block =
			annual_authority_emissions * FLIPPERINOS_PER_FLIP / BLOCKS_PER_YEAR;

		let backup_nodes = backup_amounts
			.iter()
			.enumerate()
			.map(|(bidder_id, amount)| Bid { bidder_id, amount: amount * FLIPPERINOS_PER_FLIP })
			.collect::<Vec<_>>();

		let bond = BOND * FLIPPERINOS_PER_FLIP;

		calculate_backup_rewards::<_, u128>(
			backup_nodes,
			bond,
			BLOCKS_PER_YEAR,
			backup_emissions_cap_per_block,
			authority_emissions_per_block,
			AUTHORITY_COUNT,
		)
		.into_iter()
		.map(|(id, reward)| (id, reward / FLIPPERINOS_PER_FLIP))
		.collect()
	}

	#[test]
	fn backup_rewards_global_cap() {
		// Ensure that authority rewards are high enough that the backup rewards cap is hit:
		const ANNUAL_AUTHORITY_EMISSIONS: u128 = ANNUAL_BACKUP_REWARDS_CAP * AUTHORITY_COUNT * 2;

		// A single node with BOND amount
		let backup_amounts_1 = vec![BOND];
		// Large number of backup nodes with BOND amount
		let backup_amounts_2: Vec<_> = (0..100).map(|_| BOND).collect();
		// Large number of backup nodes with amounts larger than BOND
		let backup_amounts_3: Vec<_> = (0..100).map(|idx| BOND + idx * 1000).collect();

		for amounts in [backup_amounts_1, backup_amounts_2, backup_amounts_3] {
			let backup_rewards = annual_backup_rewards(&amounts, ANNUAL_AUTHORITY_EMISSIONS);

			let total_backup_rewards = backup_rewards.iter().map(|reward| reward.1).sum::<u128>();

			assert!(
				total_backup_rewards <= ANNUAL_BACKUP_REWARDS_CAP &&
					total_backup_rewards > ANNUAL_BACKUP_REWARDS_CAP * 99 / 100
			);
		}
	}

	#[test]
	fn backup_rewards_never_exceed_authority_rewards() {
		// Authority rewards are low, so we should hit the 80% average authority reward limit:
		const ANNUAL_AUTHORITY_EMISSIONS: u128 = ANNUAL_BACKUP_REWARDS_CAP;
		const EXPECTED_AUTHORITY_REWARD: u128 = ANNUAL_AUTHORITY_EMISSIONS / AUTHORITY_COUNT;

		let backup_amounts =
			vec![u128::MAX / FLIPPERINOS_PER_FLIP, BOND, BOND * 2, BOND * 3, BOND / 2];

		let backup_rewards = annual_backup_rewards(&backup_amounts, ANNUAL_AUTHORITY_EMISSIONS);

		const MAX_EXPECTED_REWARD: u128 = EXPECTED_AUTHORITY_REWARD * 8 / 10;

		// Sanity check that we are actually hitting the limit
		assert!(MAX_EXPECTED_REWARD - backup_rewards[0].1 <= 1);

		for (_, reward) in backup_rewards {
			// Backup rewards are at most 80% of the average authority reward
			assert!(reward <= EXPECTED_AUTHORITY_REWARD * 8 / 10);
			// Sanity check:
			assert!(reward > 0);
		}
	}
}

#[test]
fn test_example_calculations() {
	use crate::constants::common::FLIPPERINOS_PER_FLIP;
	const FLIPPERINOS_PER_CENTIFLIP: u128 = FLIPPERINOS_PER_FLIP / 100;

	// The example calculation is taken from here: https://www.notion.so/chainflip/Calculating-Backup-Validator-Rewards-8c42dee6bbc842ab99b1c4f0065b19fe
	let test_backup_nodes = [
		(1, 15000000),
		(2, 12000000),
		(3, 11760000),
		(4, 11524800),
		(5, 11294304),
		(6, 11068418),
		(7, 10847050),
		(8, 10630109),
		(9, 10417506),
		(10, 10209156),
		(11, 10004973),
		(12, 9804874),
		(13, 9608776),
		(14, 9416601),
		(15, 9228269),
		(16, 9043703),
		(17, 8862829),
		(18, 8685573),
		(19, 8511861),
		(20, 8341624),
		(21, 8174791),
		(22, 8011296),
		(23, 7851070),
		(24, 7694048),
		(25, 7540167),
		(26, 7389364),
		(27, 7241577),
		(28, 7096745),
		(29, 6954810),
		(30, 6815714),
		(31, 6679400),
		(32, 6545812),
		(33, 6414896),
		(34, 6286598),
		(35, 6160866),
		(36, 6037648),
		(37, 5916895),
		(38, 5798558),
		(39, 5682586),
		(40, 5568935),
		(41, 5457556),
		(42, 5348405),
		(43, 5241437),
		(44, 5136608),
		(45, 5033876),
		(46, 4933198),
		(47, 4834534),
		(48, 4737844),
		(49, 4643087),
		(50, 4550225),
	]
	.into_iter()
	.map(|(bidder_id, amount)| Bid { bidder_id, amount: amount * FLIPPERINOS_PER_CENTIFLIP })
	.collect::<Vec<_>>();

	let backup_rewards = [
		3408412, 3408412, 3408412, 3408412, 3408412, 3408412, 3314286, 3183040, 3056992, 2935935,
		2819672, 2708013, 2600776, 2497785, 2398873, 2303877, 2212644, 2125023, 2040872, 1960054,
		1882435, 1807891, 1736298, 1667541, 1601506, 1538087, 1477179, 1418682, 1362502, 1308547,
		1256729, 1206962, 1159167, 1113264, 1069178, 1026839, 986176, 947124, 909618, 873597,
		839002, 805778, 773869, 743224, 713792, 685526, 658379, 632307, 607268, 583220,
	];

	const BOND: u128 = 110_000 * FLIPPERINOS_PER_FLIP;
	const BLOCKSPERYEAR: u128 = 14_400 * 365;
	const BACKUP_EMISSIONS_CAP_PER_BLOCK: u128 = 900_000 * FLIPPERINOS_PER_FLIP / BLOCKSPERYEAR;
	const AUTHORITY_EMISSIONS_PER_BLOCK: u128 = 9_000_000 * FLIPPERINOS_PER_FLIP / BLOCKSPERYEAR;
	const AUTHORITY_COUNT: u128 = 150;

	let calculated_rewards: Vec<(_, u128)> = calculate_backup_rewards(
		test_backup_nodes,
		BOND,
		BLOCKSPERYEAR,
		BACKUP_EMISSIONS_CAP_PER_BLOCK,
		AUTHORITY_EMISSIONS_PER_BLOCK,
		AUTHORITY_COUNT,
	);

	use core::iter::zip;
	for ((_node_id, backup_reward), expected_reward) in zip(calculated_rewards, backup_rewards) {
		let diff = (backup_reward / FLIPPERINOS_PER_CENTIFLIP).abs_diff(expected_reward);
		assert!(diff <= 1_u128);
	}
}
