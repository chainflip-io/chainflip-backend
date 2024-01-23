use std::collections::BTreeSet;

use crate::{
	genesis::GENESIS_BALANCE,
	network::{create_testnet_with_new_funder, NEW_FUNDING_AMOUNT},
};

use super::{genesis, network, *};
use cf_primitives::{AccountRole, GENESIS_EPOCH};
use cf_traits::{offence_reporting::OffenceReporter, AccountInfo, Bid, EpochInfo};
use mock_runtime::MIN_FUNDING;
use pallet_cf_funding::pallet::Error;
use pallet_cf_validator::{Backups, CurrentRotationPhase};
use sp_runtime::{FixedPointNumber, FixedU64};
use state_chain_runtime::{
	chainflip::{backup_node_rewards::calculate_backup_rewards, calculate_account_apy, Offence},
	RuntimeEvent,
};

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
					(MIN_FUNDING + 1).into(),
					ETH_DUMMY_ADDR,
					Default::default()
				));
			}

			let end_of_redemption_period =
				EPOCH_BLOCKS * REDEMPTION_PERIOD_AS_PERCENTAGE as u32 / 100;
			// Move to end of the redemption period
			System::set_block_number(end_of_redemption_period + 1);
			// We will try to redeem
			for node in &nodes {
				assert_noop!(
					Funding::redeem(
						RuntimeOrigin::signed(node.clone()),
						funding_amount.into(),
						ETH_DUMMY_ADDR,
						Default::default()
					),
					Error::<Runtime>::AuctionPhase
				);
			}

			assert_eq!(1, Validator::epoch_index(), "We should still be in the first epoch");

			// Move to new epoch
			testnet.move_to_the_next_epoch();
			// TODO: figure out how to avoid this.
			<pallet_cf_reputation::Pallet<Runtime> as OffenceReporter>::forgive_all(
				Offence::MissedAuthorshipSlot,
			);
			<pallet_cf_reputation::Pallet<Runtime> as OffenceReporter>::forgive_all(
				Offence::GrandpaEquivocation,
			);

			assert_eq!(
				2,
				Validator::epoch_index(),
				"Rotation still in phase {:?}",
				CurrentRotationPhase::<Runtime>::get(),
			);

			// We should be able to redeem again outside of the auction
			// At the moment we have a pending redemption so we would expect an error here for
			// this.
			// TODO implement Redemptions in Contract/Network
			for node in &nodes {
				assert_noop!(
					Funding::redeem(
						RuntimeOrigin::signed(node.clone()),
						(MIN_FUNDING + 1).into(),
						ETH_DUMMY_ADDR,
						Default::default()
					),
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
			(AccountId::from(ALICE), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(BOB), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(CHARLIE), AccountRole::Validator, GENESIS_BALANCE),
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

#[test]
fn backup_reward_is_calculated_linearly() {
	const EPOCH_BLOCKS: u32 = 1_000;
	const MAX_AUTHORITIES: u32 = 10;
	const NUM_BACKUPS: u32 = 20;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut network, _, _) =
				crate::authorities::fund_authorities_and_join_auction(NUM_BACKUPS);
			network.move_to_the_next_epoch();

			// 3 backup will split the backup reward.
			assert_eq!(Validator::highest_funded_qualified_backup_node_bids().count(), 3);
			const N: u128 = 100;

			let rewards_per_heartbeat = &calculate_backup_rewards::<AccountId, FlipBalance>(
				Validator::highest_funded_qualified_backup_node_bids().collect::<Vec<_>>(),
				Validator::bond(),
				HEARTBEAT_BLOCK_INTERVAL as u128,
				Emissions::backup_node_emission_per_block(),
				Emissions::current_authority_emission_per_block(),
				Validator::current_authority_count() as u128,
			);

			let rewards_per_n_heartbeats = &calculate_backup_rewards::<AccountId, FlipBalance>(
				Validator::highest_funded_qualified_backup_node_bids().collect::<Vec<_>>(),
				Validator::bond(),
				HEARTBEAT_BLOCK_INTERVAL as u128 * N,
				Emissions::backup_node_emission_per_block(),
				Emissions::current_authority_emission_per_block(),
				Validator::current_authority_count() as u128,
			);

			for i in 0..rewards_per_heartbeat.len() {
				// Validator account should match
				assert_eq!(rewards_per_heartbeat[i].0, rewards_per_heartbeat[i].0);
				// Reward per heartbeat should be scaled linearly.
				assert_eq!(rewards_per_n_heartbeats[i].1, rewards_per_heartbeat[i].1 * N);
			}
		});
}

#[test]
fn can_calculate_account_apy() {
	const EPOCH_BLOCKS: u32 = 1_000;
	const MAX_AUTHORITIES: u32 = 10;
	const NUM_BACKUPS: u32 = 20;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut network, _, _) =
				crate::authorities::fund_authorities_and_join_auction(NUM_BACKUPS);
			network.move_to_the_next_epoch();

			let mut backup_earning_rewards = Validator::highest_funded_qualified_backup_node_bids();
			let all_backups = Validator::backups();
			let validator = Validator::current_authorities().into_iter().next().unwrap();
			let Bid { bidder_id: backup, amount: backup_staked } =
				backup_earning_rewards.next().unwrap();

			// Normal account returns None
			let no_reward = AccountId::from([0xff; 32]);
			assert!(!Validator::current_authorities().contains(&no_reward));
			assert!(!Validator::backups().contains_key(&no_reward));
			assert!(calculate_account_apy(&no_reward).is_none());

			// Backups that are not qualified to earn rewards are returned None
			let backup_no_reward = all_backups.last_key_value().unwrap().0.clone();
			assert!(!backup_earning_rewards
				.any(|Bid { bidder_id, amount: _ }| bidder_id == backup_no_reward));
			assert!(calculate_account_apy(&backup_no_reward).is_none());

			// APY rate is correct for current Authority
			let total = Flip::balance(&validator);
			let reward = Emissions::current_authority_emission_per_block() * YEAR as u128 / 10u128;
			let apy_basis_point =
				FixedU64::from_rational(reward, total).checked_mul_int(10_000u32).unwrap();
			assert_eq!(apy_basis_point, 49u32);
			assert_eq!(calculate_account_apy(&validator), Some(apy_basis_point));

			// APY rate is correct for backup that are earning rewards.
			// Since all 3 backup validators has the same staked amount, and the award is capped by
			// Emission rewards are split evenly between 3 validators.
			let reward = Emissions::backup_node_emission_per_block() / 3u128 * YEAR as u128;
			let apy_basis_point = FixedU64::from_rational(reward, backup_staked)
				.checked_mul_int(10_000u32)
				.unwrap();
			assert_eq!(apy_basis_point, 35u32);
			assert_eq!(calculate_account_apy(&backup), Some(apy_basis_point));
		});
}

#[test]
fn apy_can_be_above_100_percent() {
	const EPOCH_BLOCKS: u32 = 1_000;
	const MAX_AUTHORITIES: u32 = 2;
	const NUM_BACKUPS: u32 = 2;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut network, _, _) =
				crate::authorities::fund_authorities_and_join_auction(NUM_BACKUPS);
			network.move_to_the_next_epoch();

			let validator = Validator::current_authorities().into_iter().next().unwrap();

			// Set the validator yield to very high
			assert_ok!(Emissions::update_current_authority_emission_inflation(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				1_000_000_000u32
			));

			network.move_to_the_next_epoch();

			// APY rate of > 100% can be calculated correctly.
			let total = Flip::balance(&validator);
			let reward = Emissions::current_authority_emission_per_block() * YEAR as u128 /
				MAX_AUTHORITIES as u128;
			let apy_basis_point =
				FixedU64::from_rational(reward, total).checked_mul_int(10_000u32).unwrap();
			assert_eq!(apy_basis_point, 241_377_727u32);
			assert_eq!(calculate_account_apy(&validator), Some(apy_basis_point));
		});
}

#[test]
fn backup_rewards_event_gets_emitted_on_heartbeat_interval() {
	const EPOCH_BLOCKS: u32 = 1_000;
	const NUM_BACKUPS: u32 = 20;
	const MAX_AUTHORITIES: u32 = 100;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.accounts(
			(0..MAX_AUTHORITIES as u8)
				.map(|i| (AccountId32::from([i; 32]), AccountRole::Validator, GENESIS_BALANCE))
				.collect(),
		)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut network, ..) =
				crate::authorities::fund_authorities_and_join_auction(NUM_BACKUPS);

			network.move_to_the_next_epoch();
			network.move_to_next_heartbeat_block();

			assert_eq!(Validator::current_authorities().len(), 100,);
			assert_eq!(Validator::backups().len(), NUM_BACKUPS as usize,);

			assert_eq!(
				Validator::highest_funded_qualified_backup_nodes_lookup().len(),
				NUM_BACKUPS as usize,
				"Expected all {NUM_BACKUPS} backups to be qualified."
			);

			// Backup rewards should be distributed to all qualified backups.
			let rewarded_accounts = System::events()
				.into_iter()
				.filter_map(|rec| match rec.event {
					RuntimeEvent::Emissions(
						pallet_cf_emissions::Event::BackupRewardsDistributed { account_id, .. },
					) => Some(account_id),
					_ => None,
				})
				.collect::<BTreeSet<_>>();

			assert_eq!(
				rewarded_accounts.len(),
				NUM_BACKUPS as usize,
				"Expected all {NUM_BACKUPS} backups to be rewarded."
			);
			assert_eq!(
				rewarded_accounts,
				Validator::highest_funded_qualified_backup_nodes_lookup()
			);
		});
}
