use crate::{
	genesis, network, NodeId, GENESIS_EPOCH, HEARTBEAT_BLOCK_INTERVAL, VAULT_ROTATION_BLOCKS,
};
use cf_primitives::{AuthorityCount, ChainflipAccountState};
use cf_traits::{EpochInfo, FlipBalance, StakeTransfer};
use sp_runtime::AccountId32;
use state_chain_runtime::{Flip, Validator};
use std::collections::HashMap;

#[test]
fn authorities_earn_rewards_for_authoring_blocks() {
	// We want to have at least one heartbeat within our reduced epoch
	const EPOCH_BLOCKS: u32 = 1000;
	// Reduce our validating set and hence the number of nodes we need to have a backup
	// set
	const MAX_AUTHORITIES: AuthorityCount = 3;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let genesis_authorities = Validator::current_authorities();
			let (mut testnet, _) = network::Network::create(0, &genesis_authorities);

			let staked_amounts = || {
				genesis_authorities
					.iter()
					.map(|id| (id.clone(), Flip::staked_balance(id)))
					.collect()
			};

			let staked_amounts_before: Vec<(AccountId32, u128)> = staked_amounts();

			// each authority should author a block and mint FLIP to themselves
			testnet.move_forward_blocks(MAX_AUTHORITIES);

			// Each node should have more rewards now than before, since they've each authored a
			// block
			let staked_amounts_after = staked_amounts();

			// Ensure all nodes have increased the same amount
			let first_node_stake = staked_amounts_after.first().unwrap().1;
			staked_amounts_after.iter().all(|(_node, amount)| amount == &first_node_stake);

			// Ensure all nodes have a higher stake than before
			staked_amounts_before.into_iter().zip(staked_amounts_after).for_each(
				|((_node, amount_before), (_node2, amount_after))| {
					assert!(amount_before < amount_after)
				},
			);
		});
}

#[test]
fn genesis_nodes_rotated_out_accumulate_rewards_correctly() {
	// We want to have at least one heartbeat within our reduced epoch
	const EPOCH_BLOCKS: u32 = 1000;
	// Reduce our validating set and hence the number of nodes we need to have a backup
	// set
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			// Create MAX_AUTHORITIES backup nodes and stake them above our genesis
			// authorities The result will be our newly created nodes will be authorities
			// and the genesis authorities will become backup nodes
			let mut genesis_authorities = Validator::current_authorities();
			let (mut testnet, mut init_backup_nodes) =
				network::Network::create(MAX_AUTHORITIES as u8, &genesis_authorities);

			// An initial stake which is greater than the genesis stakes
			// We intend for these initially backup nodes to win the auction
			const INITIAL_STAKE: FlipBalance = genesis::GENESIS_BALANCE * 2;
			// Stake these backup nodes so that they are included in the next epoch
			for node in &init_backup_nodes {
				testnet.stake_manager_contract.stake(node.clone(), INITIAL_STAKE, GENESIS_EPOCH);
			}

			// Allow the stakes to be registered, then initialise the account keys and peer
			// ids.
			testnet.move_forward_blocks(1);

			for node in &init_backup_nodes {
				network::setup_account_and_peer_mapping(node);
				network::Cli::activate_account(node);
			}

			// Start an auction
			testnet.move_to_next_epoch();
			testnet.submit_heartbeat_all_engines();
			testnet.move_forward_blocks(1);

			assert_eq!(
				GENESIS_EPOCH,
				Validator::epoch_index(),
				"We should still be in the genesis epoch"
			);

			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
			assert_eq!(GENESIS_EPOCH + 1, Validator::epoch_index(), "We should be in a new epoch");

			// assert list of authorities as being the new nodes
			let mut current_authorities: Vec<NodeId> = Validator::current_authorities();

			current_authorities.sort();
			init_backup_nodes.sort();

			assert_eq!(
				init_backup_nodes, current_authorities,
				"our new initial backup nodes should be the new authorities"
			);

			current_authorities.iter().for_each(|account_id| {
				let account_data = Validator::get_validator_state(account_id.clone());
				assert_eq!(account_data, ChainflipAccountState::CurrentAuthority);
				// we were active in teh first epoch

				// TODO: Check historical epochs
			});

			// assert list of backup validators as being the genesis authorities
			let mut highest_staked_backup_nodes: Vec<NodeId> =
				Validator::highest_staked_qualified_backup_nodes_lookup().into_iter().collect();

			highest_staked_backup_nodes.sort();
			genesis_authorities.sort();

			assert_eq!(
				genesis_authorities, highest_staked_backup_nodes,
				"the genesis authorities should now be the backup nodes"
			);

			highest_staked_backup_nodes.iter().for_each(|account_id| {
				let account_data = Validator::get_validator_state(account_id.clone());
				// we were active in the first epoch
				assert_eq!(account_data, ChainflipAccountState::Backup);
				// TODO: Check historical epochs
			});

			let backup_node_balances: HashMap<NodeId, FlipBalance> = highest_staked_backup_nodes
				.iter()
				.map(|validator_id| (validator_id.clone(), Flip::staked_balance(validator_id)))
				.collect::<Vec<(NodeId, FlipBalance)>>()
				.into_iter()
				.collect();

			// Move forward a heartbeat, emissions should be shared to backup nodes
			testnet.move_forward_blocks(HEARTBEAT_BLOCK_INTERVAL);

			// We won't calculate the exact emissions but they should be greater than their
			// initial stake
			for (backup_node, pre_balance) in backup_node_balances {
				assert!(pre_balance < Flip::staked_balance(&backup_node));
			}
		});
}
