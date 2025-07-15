// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
	network::{self, new_account},
	AccountId, AuthorityCount,
};

use cf_primitives::{AccountRole, Delegation};
use cf_traits::AccountInfo;
use sp_runtime::{traits::Zero, Percent, Permill};
use state_chain_runtime::{
	constants::common::HEARTBEAT_BLOCK_INTERVAL, Balance, Flip, Runtime, System, Validator,
};

struct Delegator {
	pub account: AccountId,
	pub pre_balance: Balance,
	pub post_balance: Balance,
}
impl Delegator {
	pub fn diff(&self) -> Balance {
		if self.post_balance >= self.pre_balance {
			self.post_balance - self.pre_balance
		} else {
			self.pre_balance - self.post_balance
		}
	}
}

#[test]
fn block_author_rewards_are_distributed_among_delegators() {
	const EPOCH_DURATION_BLOCKS: u32 = 200;
	const MAX_AUTHORITIES: AuthorityCount = 3;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);

			testnet.move_to_the_next_epoch();

			let epoch = Validator::current_epoch();
			let auth = pallet_cf_validator::CurrentAuthorities::<Runtime>::get()
				.into_iter()
				.next()
				.unwrap();

			// Setup delegator/operator
			// TODO: update this with proper extrinsic calls after delegation API is integrated
			// Setup 3 delegator
			let mut delegators = (0xF0..0xF3)
				.map(|i| {
					let account = AccountId::from([i; 32]);
					new_account(&account, AccountRole::LiquidityProvider);
					Delegator {
						account: account.clone(),
						pre_balance: Flip::balance(&account),
						post_balance: Default::default(),
					}
				})
				.collect::<Vec<_>>();

			let operator = AccountId::from([0xe1; 32]);
			new_account(&operator, AccountRole::LiquidityProvider);

			pallet_cf_validator::OperatorInfo::<Runtime>::insert(
				epoch,
				operator.clone(),
				Delegation {
					validator_bids: BTreeMap::from_iter([(auth.clone(), 1_000_000_000u128)]),
					delegator_bids: BTreeMap::from_iter([
						(delegators[0].account.clone(), 100_000_000u128), // 5% of cut
						(delegators[1].account.clone(), 400_000_000u128), // 20% of cut
						(delegators[2].account.clone(), 500_000_000u128), // 25% of cut
					]),
					delegation_fee: Percent::from_percent(50),
				},
			);
			pallet_cf_validator::ValidatorToOperator::<Runtime>::insert(
				epoch,
				auth.clone(),
				operator.clone(),
			);

			let auth_pre_balance = Flip::balance(&auth);

			// Let few blocks to pass so some rewards are distributed for authoring blocks.
			testnet.move_forward_blocks(30);

			delegators.iter_mut().for_each(|d| d.post_balance = Flip::balance(&d.account));

			let auth_post_balance = Flip::balance(&auth);
			let auth_cut = auth_post_balance - auth_pre_balance;

			let total_auth_reward =
				auth_cut + delegators[0].diff() + delegators[1].diff() + delegators[2].diff();

			assert!(total_auth_reward > 0u128);

			// Verify that rewards are distributed accordingly.
			assert_eq!(Permill::from_percent(50) * total_auth_reward, auth_cut);
			assert_eq!(Permill::from_percent(5) * total_auth_reward, delegators[0].diff());
			assert_eq!(Permill::from_percent(20) * total_auth_reward, delegators[1].diff());
			assert_eq!(Permill::from_percent(25) * total_auth_reward, delegators[2].diff(),);
		});
}

#[test]
fn backup_rewards_are_distributed_among_delegators() {
	const EPOCH_DURATION_BLOCKS: u32 = 200;
	const MAX_AUTHORITIES: AuthorityCount = 3;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) =
				network::fund_authorities_and_join_auction(MAX_AUTHORITIES + 1);

			testnet.move_to_the_next_epoch();

			let epoch = Validator::current_epoch();
			let backup = Validator::backups().into_iter().next().expect("Should have 1 backup").0;

			// Setup delegator/operator
			// TODO: update this with proper extrinsic calls after delegation API is integrated
			// Setup 3 delegator
			let mut delegators = (0xF0..0xF3)
				.map(|i| {
					let account = AccountId::from([i; 32]);
					new_account(&account, AccountRole::LiquidityProvider);
					Delegator {
						account: account.clone(),
						pre_balance: Flip::balance(&account),
						post_balance: Default::default(),
					}
				})
				.collect::<Vec<_>>();

			let operator = AccountId::from([0xe2; 32]);
			new_account(&operator, AccountRole::LiquidityProvider);

			// Move to the block before backup rewards are distributed
			let current_block = System::block_number();
			let blocks_to_move =
				HEARTBEAT_BLOCK_INTERVAL - current_block % HEARTBEAT_BLOCK_INTERVAL - 1;
			testnet.move_forward_blocks(blocks_to_move);

			pallet_cf_validator::OperatorInfo::<Runtime>::insert(
				epoch,
				operator.clone(),
				Delegation {
					validator_bids: BTreeMap::from_iter([(backup.clone(), 1_000_000_000u128)]),
					delegator_bids: BTreeMap::from_iter([
						(delegators[0].account.clone(), 600_000_000u128), // 54% of cut
						(delegators[1].account.clone(), 300_000_000u128), // 27% of cut
						(delegators[2].account.clone(), 100_000_000u128), // 9%  of cut
					]),
					delegation_fee: Percent::from_percent(10),
				},
			);

			pallet_cf_validator::ValidatorToOperator::<Runtime>::insert(
				epoch,
				backup.clone(),
				operator.clone(),
			);

			let backup_pre_balance = Flip::balance(&backup);

			// Trigger backup reward distribution
			testnet.move_forward_blocks(2);

			delegators.iter_mut().for_each(|d| d.post_balance = Flip::balance(&d.account));

			let backup_post_balance = Flip::balance(&backup);
			let backup_cut = backup_post_balance - backup_pre_balance;

			let total_backup_reward =
				backup_cut + delegators[0].diff() + delegators[1].diff() + delegators[2].diff();

			assert!(total_backup_reward > 0u128);

			// Verify that rewards are distributed accordingly
			assert_eq!(Permill::from_percent(10) * total_backup_reward, backup_cut);
			assert_eq!(Permill::from_percent(54) * total_backup_reward, delegators[0].diff());
			assert_eq!(Permill::from_percent(27) * total_backup_reward, delegators[1].diff());
			assert_eq!(Permill::from_percent(9) * total_backup_reward, delegators[2].diff());
		});
}

#[test]
fn slashings_are_distributed_among_delegators() {
	const EPOCH_DURATION_BLOCKS: u32 = 1_000;
	const MAX_AUTHORITIES: AuthorityCount = 3;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);

			testnet.move_to_the_next_epoch();

			let epoch = Validator::current_epoch();
			let auth = pallet_cf_validator::CurrentAuthorities::<Runtime>::get()
				.into_iter()
				.next()
				.unwrap();

			// Set Validator as "offline" and reduce reputation
			pallet_cf_reputation::Reputations::<Runtime>::mutate(&auth, |rep| {
				rep.online_blocks = Zero::zero();
				rep.reputation_points = -1;
			});
			testnet.set_active(&auth, false);
			testnet.set_auto_heartbeat(&auth, false);

			// Move to the block before backup rewards are distributed
			let current_block = System::block_number();
			let blocks_to_move =
				HEARTBEAT_BLOCK_INTERVAL - current_block % HEARTBEAT_BLOCK_INTERVAL - 1;
			testnet.move_forward_blocks(blocks_to_move + HEARTBEAT_BLOCK_INTERVAL);

			// Setup delegator/operator
			// TODO: update this with proper extrinsic calls after delegation API is integrated
			// Setup 3 delegator
			let mut delegators = (0xF0..0xF3)
				.map(|i| {
					let account = AccountId::from([i; 32]);
					new_account(&account, AccountRole::LiquidityProvider);
					Delegator {
						account: account.clone(),
						pre_balance: Flip::balance(&account),
						post_balance: Default::default(),
					}
				})
				.collect::<Vec<_>>();

			let operator = AccountId::from([0xe1; 32]);
			new_account(&operator, AccountRole::LiquidityProvider);

			pallet_cf_validator::OperatorInfo::<Runtime>::insert(
				epoch,
				operator.clone(),
				Delegation {
					validator_bids: BTreeMap::from_iter([(auth.clone(), 1_000_000_000u128)]),
					delegator_bids: BTreeMap::from_iter([
						(delegators[0].account.clone(), 100_000_000u128), // 5% of cut
						(delegators[1].account.clone(), 400_000_000u128), // 20% of cut
						(delegators[2].account.clone(), 500_000_000u128), // 25% of cut
					]),
					delegation_fee: Percent::from_percent(50),
				},
			);
			pallet_cf_validator::ValidatorToOperator::<Runtime>::insert(
				epoch,
				auth.clone(),
				operator.clone(),
			);

			let auth_pre_balance = Flip::balance(&auth);

			// Move forward 1 block so that the inactive validator is slashed
			testnet.move_forward_blocks(1);

			delegators.iter_mut().for_each(|d| d.post_balance = Flip::balance(&d.account));

			let auth_post_balance = Flip::balance(&auth);
			let auth_slashed = auth_pre_balance - auth_post_balance;

			let total_slashing =
				auth_slashed + delegators[0].diff() + delegators[1].diff() + delegators[2].diff();

			assert!(auth_slashed > 0u128);

			// Verify that rewards are distributed accordingly
			assert_eq!(Permill::from_percent(50) * total_slashing, auth_slashed);
			assert_eq!(Permill::from_percent(5) * total_slashing, delegators[0].diff());
			assert_eq!(Permill::from_percent(20) * total_slashing, delegators[1].diff());
			assert_eq!(Permill::from_percent(25) * total_slashing, delegators[2].diff(),);
		});
}
