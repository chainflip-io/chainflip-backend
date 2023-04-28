use crate::{
	genesis::GENESIS_BALANCE,
	network::{create_testnet_with_new_funder, NEW_FUNDING_AMOUNT},
};

use super::{genesis, network, *};
use cf_primitives::GENESIS_EPOCH;
use cf_traits::EpochInfo;
use pallet_cf_funding::pallet::Error;
use pallet_cf_validator::Backups;
#[test]
// Nodes cannot redeem when we are out of the redeeming period (50% of the epoch)
// We have a set of nodes that are funded and can redeem in the redeeming period and
// not redeem when out of the period
fn cannot_redeem_funds_out_of_redemption_period() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 3;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let mut nodes = Validator::current_authorities();
			let (mut testnet, mut backup_nodes) = network::Network::create(0, &nodes);

			for backup_node in backup_nodes.clone() {
				network::Cli::start_bidding(&backup_node);
			}

			nodes.append(&mut backup_nodes);

			// Fund these nodes so that they are included in the next epoch
			let funding_amount = genesis::GENESIS_BALANCE;
			for node in &nodes {
				testnet.state_chain_gateway_contract.fund_account(
					node.clone(),
					funding_amount,
					GENESIS_EPOCH,
				);
			}

			// Move forward one block to process events
			testnet.move_forward_blocks(1);

			assert_eq!(
				GENESIS_EPOCH,
				Validator::epoch_index(),
				"We should be in the genesis epoch"
			);

			// We should be able to redeem outside of an auction
			for node in &nodes {
				assert_ok!(Funding::redeem(
					RuntimeOrigin::signed(node.clone()),
					1.into(),
					ETH_DUMMY_ADDR
				));
			}

			let end_of_redemption_period =
				EPOCH_BLOCKS * PERCENT_OF_EPOCH_PERIOD_REDEEMABLE as u32 / 100;
			// Move to end of the redemption period
			System::set_block_number(end_of_redemption_period + 1);
			// We will try to redeem
			for node in &nodes {
				assert_noop!(
					Funding::redeem(
						RuntimeOrigin::signed(node.clone()),
						funding_amount.into(),
						ETH_DUMMY_ADDR
					),
					Error::<Runtime>::AuctionPhase
				);
			}

			assert_eq!(1, Validator::epoch_index(), "We should still be in the first epoch");

			// Move to new epoch
			testnet.move_to_next_epoch();
			// Run things to a successful vault rotation
			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);

			assert_eq!(2, Validator::epoch_index(), "We are in a new epoch");

			// We should be able to redeem again outside of the auction
			// At the moment we have a pending redemption so we would expect an error here for
			// this.
			// TODO implement Redemptions in Contract/Network
			for node in &nodes {
				assert_noop!(
					Funding::redeem(RuntimeOrigin::signed(node.clone()), 1.into(), ETH_DUMMY_ADDR),
					Error::<Runtime>::PendingRedemption
				);
			}
		});
}

#[test]
fn funded_node_is_added_to_backups() {
	const EPOCH_BLOCKS: u32 = 10_000_000;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		// As we run a rotation at genesis we will need accounts to support
		// having 5 authorities as the default is 3 (Alice, Bob and Charlie)
		.accounts(vec![
			(AccountId::from(ALICE), GENESIS_BALANCE),
			(AccountId::from(BOB), GENESIS_BALANCE),
			(AccountId::from(CHARLIE), GENESIS_BALANCE),
		])
		.min_authorities(3)
		.build()
		.execute_with(|| {
			let (_, new_backup) = create_testnet_with_new_funder();
			let backups_map = Backups::<Runtime>::get();
			assert_eq!(backups_map.len(), 1);
			assert_eq!(backups_map.get(&new_backup).unwrap(), &NEW_FUNDING_AMOUNT);
		});
}
