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

fn abs_difff(a: u128, b: u128) -> u128 {
	if a > b {
		a - b
	} else {
		b - a
	}
}

#[test]
fn test_example_calculations() {
	let test_backup_nodes: Vec<(u128, u128)> = vec![
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
	];

	let mut backup_rewards: Vec<u128> = vec![
		3408412, 3408412, 3408412, 3408412, 3408412, 3408412, 3314286, 3183040, 3056992, 2935935,
		2819672, 2708013, 2600776, 2497785, 2398873, 2303877, 2212644, 2125023, 2040872, 1960054,
		1882435, 1807891, 1736298, 1667541, 1601506, 1538087, 1477179, 1418682, 1362502, 1308547,
		1256729, 1206962, 1159167, 1113264, 1069178, 1026839, 986176, 947124, 909618, 873597,
		839002, 805778, 773869, 743224, 713792, 685526, 658379, 632307, 607268, 583220,
	];

	const MUL_FACTOR: u128 = 100000;

	let test_backup_nodes = test_backup_nodes
		.into_iter()
		.map(|(node, reward)| (node, reward * MUL_FACTOR))
		.collect();

	const MAB: u128 = 11000000 * MUL_FACTOR;
	const BLOCKSPERYEAR: u128 = 14400 * 356;
	const BACKUP_EMISSIONS_CAP_PER_BLOCK: u128 = 90_000_000 * MUL_FACTOR / BLOCKSPERYEAR;
	const AUTHORITY_EMISSIONS_PER_BLOCK: u128 = 900_000_000 * MUL_FACTOR / BLOCKSPERYEAR;
	const AUTHORITY_COUNT: u128 = 150;

	let mut calculated_rewards = calculate_backup_rewards(
		test_backup_nodes,
		MAB,
		BLOCKSPERYEAR,
		BACKUP_EMISSIONS_CAP_PER_BLOCK,
		AUTHORITY_EMISSIONS_PER_BLOCK,
		AUTHORITY_COUNT,
	);

	for _ in 0..backup_rewards.len() {
		assert!(
			abs_difff(
				calculated_rewards.pop().unwrap().1 / MUL_FACTOR,
				backup_rewards.pop().unwrap()
			) <= 3_u128
		)
	}
	assert!(calculated_rewards.pop().is_none());
	assert!(backup_rewards.pop().is_none());
}
