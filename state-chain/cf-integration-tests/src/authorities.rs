use crate::{
	genesis, get_validator_state, network, ChainflipAccountState, NodeId, HEARTBEAT_BLOCK_INTERVAL,
	VAULT_ROTATION_BLOCKS,
};
use cf_primitives::{AuthorityCount, FlipBalance, GENESIS_EPOCH};
use cf_traits::EpochInfo;
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

			let funded_amounts = || {
				genesis_authorities
					.iter()
					.map(|id| (id.clone(), Flip::total_balance_of(id)))
					.collect()
			};

			let funded_amounts_before: Vec<(AccountId32, u128)> = funded_amounts();

			// each authority should author a block and mint FLIP to themselves
			testnet.move_forward_blocks(MAX_AUTHORITIES);

			// Each node should have more rewards now than before, since they've each authored a
			// block
			let funded_amounts_after = funded_amounts();

			// Ensure all nodes have increased the same amount
			let first_amount = funded_amounts_after.first().unwrap().1;
			funded_amounts_after.iter().all(|(_node, amount)| amount == &first_amount);

			// Ensure all nodes have a higher balance than before
			funded_amounts_before.into_iter().zip(funded_amounts_after).for_each(
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
			// Create MAX_AUTHORITIES backup nodes and fund them above our genesis
			// authorities The result will be our newly created nodes will be authorities
			// and the genesis authorities will become backup nodes
			let genesis_authorities = Validator::current_authorities();
			let (mut testnet, init_backup_nodes) =
				network::Network::create(MAX_AUTHORITIES as u8, &genesis_authorities);

			// An initial balance which is greater than the genesis balances
			// We intend for these initially backup nodes to win the auction
			const INITIAL_FUNDING: FlipBalance = genesis::GENESIS_BALANCE * 2;
			// Fund these backup nodes so that they are included in the next epoch
			for node in &init_backup_nodes {
				testnet.state_chain_gateway_contract.fund_account(
					node.clone(),
					INITIAL_FUNDING,
					GENESIS_EPOCH,
				);
			}

			// Allow the funds to be registered, then initialise the account keys and peer
			// ids.
			testnet.move_forward_blocks(1);

			for node in &init_backup_nodes {
				network::Cli::register_as_validator(node);
				network::setup_account_and_peer_mapping(node);
				network::Cli::start_bidding(node);
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
			let current_authorities = Validator::current_authorities();

			assert_eq!(
				init_backup_nodes, current_authorities,
				"our new initial backup nodes should be the new authorities"
			);

			current_authorities.iter().for_each(|account_id| {
				assert_eq!(
					get_validator_state(account_id),
					ChainflipAccountState::CurrentAuthority
				);
				// TODO: Check historical epochs
			});

			// assert list of backup validators as being the genesis authorities
			let highest_funded_backup_nodes =
				Validator::highest_funded_qualified_backup_nodes_lookup();

			assert_eq!(
				genesis_authorities, highest_funded_backup_nodes,
				"the genesis authorities should now be the backup nodes"
			);

			highest_funded_backup_nodes.iter().for_each(|account_id| {
				// we were active in the first epoch
				assert_eq!(get_validator_state(account_id), ChainflipAccountState::Backup);
				// TODO: Check historical epochs
			});

			let backup_node_balances: HashMap<NodeId, FlipBalance> = highest_funded_backup_nodes
				.iter()
				.map(|validator_id| (validator_id.clone(), Flip::total_balance_of(validator_id)))
				.collect::<Vec<(NodeId, FlipBalance)>>()
				.into_iter()
				.collect();

			// Move forward a heartbeat, emissions should be shared to backup nodes
			testnet.move_forward_blocks(HEARTBEAT_BLOCK_INTERVAL);

			// We won't calculate the exact emissions but they should be greater than their
			// initial balance
			for (backup_node, pre_balance) in backup_node_balances {
				assert!(pre_balance < Flip::total_balance_of(&backup_node));
			}
		});
}
