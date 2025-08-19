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

use sp_std::collections::btree_set::BTreeSet;

use crate::{
	network::{self, new_account, Network},
	AccountId, AuthorityCount, RuntimeOrigin,
};

use cf_primitives::{AccountRole, FLIPPERINOS_PER_FLIP};
use cf_traits::AccountInfo;
use frame_support::{assert_ok, traits::OnNewAccount};
use pallet_cf_validator::{DelegationAcceptance, OperatorSettings};
use sp_runtime::{traits::Zero, PerU16};
use state_chain_runtime::{
	AccountRoles, Balance, Flip, Funding, LiquidityProvider, Runtime, Validator,
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

fn setup_delegation(
	testnet: &mut Network,
	validator: AccountId,
	operator: AccountId,
	operator_cut: u32,
	delegators: BTreeSet<(AccountId, Balance)>,
) -> Vec<Delegator> {
	new_account(&operator, AccountRole::Operator);

	assert_ok!(Validator::claim_validator(
		RuntimeOrigin::signed(operator.clone()),
		validator.clone()
	));
	assert_ok!(Validator::accept_operator(
		RuntimeOrigin::signed(validator.clone()),
		operator.clone(),
	));
	assert_ok!(Validator::update_operator_settings(
		RuntimeOrigin::signed(operator.clone()),
		OperatorSettings {
			fee_bps: operator_cut,
			delegation_acceptance: DelegationAcceptance::Allow,
		},
	));

	assert_eq!(
		pallet_cf_validator::ManagedValidators::<Runtime>::get(validator),
		Some(operator.clone())
	);
	assert!(pallet_cf_validator::OperatorSettingsLookup::<Runtime>::get(operator.clone()).is_some());

	let delegators = delegators
		.into_iter()
		.map(|(d, stake)| {
			assert_ok!(Funding::funded(
				pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
				d.clone(),
				stake * FLIPPERINOS_PER_FLIP,
				Default::default(),
				Default::default(),
			));
			AccountRoles::on_new_account(&d);
			assert_ok!(LiquidityProvider::register_lp_account(RuntimeOrigin::signed(d.clone())));
			crate::network::register_refund_addresses(&d);

			assert_ok!(Validator::delegate(RuntimeOrigin::signed(d.clone()), operator.clone()));
			(d, stake * FLIPPERINOS_PER_FLIP)
		})
		.collect();

	// Move to the next for delegation to take affect
	testnet.move_to_the_next_epoch();

	let actual_delegator_set = Validator::get_bonded_delegators_for_operator(&operator);
	assert_eq!(actual_delegator_set, delegators);

	delegators
		.into_iter()
		.map(|(d, _amount)| Delegator {
			account: d.clone(),
			pre_balance: Flip::balance(&d),
			post_balance: Default::default(),
		})
		.collect()
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

			let auth = pallet_cf_validator::CurrentAuthorities::<Runtime>::get()
				.into_iter()
				.next()
				.unwrap();

			let operator = AccountId::from([0xe1; 32]);

			// Setup 3 delegator, operator and association with validator.
			let mut delegators = setup_delegation(
				&mut testnet,
				auth.clone(),
				operator.clone(),
				500, // 5%
				BTreeSet::from_iter([
					(AccountId::from([0xA0; 32]), 10_000_000u128),
					(AccountId::from([0xA1; 32]), 40_000_000u128),
					(AccountId::from([0xA2; 32]), 50_000_000u128),
				]),
			);

			let auth_pre_balance = Flip::balance(&auth);
			let op_pre_balance = Flip::balance(&operator);

			// Let few blocks to pass so some rewards are distributed for authoring blocks.
			testnet.move_forward_blocks(30);

			delegators.iter_mut().for_each(|d| d.post_balance = Flip::balance(&d.account));

			let auth_post_balance = Flip::balance(&auth);
			let auth_cut = auth_post_balance - auth_pre_balance;

			let op_post_balance = Flip::balance(&operator);
			let op_cut = op_post_balance - op_pre_balance;

			let total_auth_reward = auth_cut +
				delegators[0].diff() +
				delegators[1].diff() +
				delegators[2].diff() +
				op_cut;

			assert!(total_auth_reward > 0u128);

			let total_pre_balance =
				auth_pre_balance + delegators.iter().map(|d| d.pre_balance).sum::<u128>();

			// Verify that rewards are distributed accordingly.
			assert_eq!(
				PerU16::from_rational(auth_cut, total_auth_reward,),
				PerU16::from_rational(auth_pre_balance, total_pre_balance)
			);

			for delegator in &delegators {
				assert_eq!(
					PerU16::from_rational(delegator.diff(), total_auth_reward),
					PerU16::from_rational(delegator.pre_balance, total_pre_balance) *
						PerU16::from_percent(95)
				);
			}
			assert_eq!(
				PerU16::from_rational(op_cut, total_auth_reward),
				PerU16::from_rational(
					delegators.iter().map(|d| d.pre_balance).sum::<u128>(),
					total_pre_balance
				) * PerU16::from_percent(5)
			);
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

			let auth = pallet_cf_validator::CurrentAuthorities::<Runtime>::get()
				.into_iter()
				.next()
				.unwrap();

			let operator: sp_runtime::AccountId32 = AccountId::from([0xe1; 32]);

			// Setup 3 delegator, operator and association with validator.
			let mut delegators = setup_delegation(
				&mut testnet,
				auth.clone(),
				operator.clone(),
				500, // 5%
				BTreeSet::from_iter([
					(AccountId::from([0xA0; 32]), 10_000_000u128),
					(AccountId::from([0xA1; 32]), 40_000_000u128),
					(AccountId::from([0xA2; 32]), 50_000_000u128),
				]),
			);

			// Set Validator as "offline" and reduce reputation
			pallet_cf_reputation::Reputations::<Runtime>::mutate(&auth, |rep| {
				rep.online_blocks = Zero::zero();
				rep.reputation_points = -1;
			});
			testnet.set_active(&auth, false);
			testnet.set_auto_heartbeat(&auth, false);

			// Move to the block before backup rewards are distributed
			testnet.move_to_next_heartbeat_block(Some(-1));

			// Update pre-balance
			let auth_pre_balance = Flip::balance(&auth);
			let op_pre_balance = Flip::balance(&operator);

			// since the balances have been updated because of block rewards.
			delegators.iter_mut().for_each(|d| d.pre_balance = Flip::balance(&d.account));

			// Move forward 1 block so that the inactive validator is slashed
			testnet.move_forward_blocks(1);

			delegators.iter_mut().for_each(|d| d.post_balance = Flip::balance(&d.account));

			let auth_post_balance = Flip::balance(&auth);
			let auth_slash = auth_pre_balance - auth_post_balance;

			let op_post_balance = Flip::balance(&operator);
			let op_slash = op_pre_balance - op_post_balance;

			let total_slashing = auth_slash +
				delegators[0].diff() +
				delegators[1].diff() +
				delegators[2].diff() +
				op_slash;

			assert!(auth_slash > 0u128);

			let total_pre_balance =
				auth_pre_balance + delegators.iter().map(|d| d.pre_balance).sum::<u128>();

			// Verify that slashings are distributed accordingly
			assert_eq!(
				PerU16::from_rational(auth_slash, total_slashing),
				PerU16::from_rational(auth_pre_balance, total_pre_balance)
			);

			for delegator in &delegators {
				assert_eq!(
					PerU16::from_rational(delegator.diff(), total_slashing),
					PerU16::from_rational(delegator.pre_balance, total_pre_balance) *
						PerU16::from_percent(95)
				);
			}

			assert_eq!(
				PerU16::from_rational(op_slash, total_slashing),
				PerU16::from_rational(
					delegators.iter().map(|d| d.pre_balance).sum::<u128>(),
					total_pre_balance
				) * PerU16::from_percent(5)
			);
		});
}
