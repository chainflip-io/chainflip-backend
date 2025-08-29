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

use chainflip_node::chain_spec::devnet::HEARTBEAT_BLOCK_INTERVAL;
use sp_std::collections::btree_set::BTreeSet;

use crate::{
	network::{self, new_account, Network},
	AccountId, AuthorityCount, RuntimeOrigin,
};

use cf_primitives::{AccountRole, FlipBalance, FLIPPERINOS_PER_FLIP};
use cf_traits::{AccountInfo, EpochInfo};
use frame_support::assert_ok;
use pallet_cf_validator::{DelegationAcceptance, OperatorSettings};
use sp_runtime::{traits::Zero, PerU16};
use state_chain_runtime::{Balance, Flip, Funding, Runtime, System, Validator};

fn setup_delegation(
	testnet: &mut Network,
	validator: AccountId,
	operator: AccountId,
	operator_cut: u32,
	delegators: BTreeMap<AccountId, Balance>,
) {
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
		pallet_cf_validator::ManagedValidators::<Runtime>::get(&validator),
		Some(operator.clone())
	);
	assert!(pallet_cf_validator::OperatorSettingsLookup::<Runtime>::get(operator.clone()).is_some());

	let delegators: BTreeMap<_, _> = delegators
		.into_iter()
		.map(|(d, stake)| {
			assert_ok!(Funding::funded(
				pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
				d.clone(),
				stake,
				Default::default(),
				Default::default(),
			));
			assert_ok!(Validator::delegate(
				RuntimeOrigin::signed(d.clone()),
				operator.clone(),
				pallet_cf_validator::DelegationAmount::Max
			));
			(d, stake)
		})
		.collect();

	// Move to the next for delegation to take affect
	testnet.move_to_the_next_epoch();

	let actual_delegator_set = pallet_cf_validator::DelegationChoice::<Runtime>::iter()
		.map(|(d, _)| d)
		.collect::<BTreeSet<_>>();
	assert_eq!(actual_delegator_set, delegators.keys().cloned().collect());

	// Debug delegation setup
	println!("Debug delegation setup:");
	println!("  operator: {:?}", operator);
	println!("  validator: {:?}", validator);
	for (delegator, stake) in &delegators {
		println!(
			"  delegator: {:?}, stake: {}, max_bid: {:?}",
			delegator,
			stake,
			pallet_cf_validator::MaxDelegationBid::<Runtime>::get(delegator)
		);
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

			let auth = pallet_cf_validator::CurrentAuthorities::<Runtime>::get()
				.into_iter()
				.next()
				.unwrap();

			println!("Selected authority for test: {:?}", auth);

			let operator = AccountId::from([0xe1; 32]);

			// Setup 3 delegator, operator and association with validator.
			let delegators = BTreeMap::from_iter([
				(AccountId::from([0xA0; 32]), 100_000u128 * FLIPPERINOS_PER_FLIP),
				(AccountId::from([0xA1; 32]), 400_000u128 * FLIPPERINOS_PER_FLIP),
				(AccountId::from([0xA2; 32]), 500_000u128 * FLIPPERINOS_PER_FLIP),
			]);
			setup_delegation(
				&mut testnet,
				auth.clone(),
				operator.clone(),
				500, // 5%
				delegators.clone(),
			);

			testnet.move_to_the_next_epoch();

			let epoch_index = pallet_cf_validator::Pallet::<Runtime>::epoch_index();
			let snapshot =
				pallet_cf_validator::DelegationSnapshots::<Runtime>::get(epoch_index, &operator)
					.expect("Snapshot should be registered on new epoch");
			assert!(
				pallet_cf_validator::ValidatorToOperator::<Runtime>::get(epoch_index, &auth)
					.expect("Validator should be mapped to operator") ==
					operator
			);
			assert!(
				snapshot.validators.len() == 1 &&
					snapshot.validators.keys().any(|v| *v == auth) &&
					snapshot.delegators == delegators,
				"Bad snapshot: {:#?}",
				snapshot,
			);

			let auth_pre_balance = Flip::balance(&auth);
			let op_pre_balance = Flip::balance(&operator);
			let total_delegators_pre_balance: FlipBalance =
				delegators.keys().map(Flip::balance).sum();

			// Move forward through all authorities to ensure the reward is distributed once.
			testnet.move_forward_blocks(
				pallet_cf_validator::CurrentAuthorities::<Runtime>::decode_len()
					.expect("at least one authority") as u32,
			);

			let auth_post_balance = Flip::balance(&auth);
			let auth_cut = auth_post_balance - auth_pre_balance;

			let op_post_balance = Flip::balance(&operator);
			let op_cut = op_post_balance - op_pre_balance;

			let total_delegators_post_balance: FlipBalance =
				delegators.keys().map(Flip::balance).sum();
			let delegators_cut = total_delegators_post_balance - total_delegators_pre_balance;

			let total_auth_reward =
				pallet_cf_emissions::Pallet::<Runtime>::current_authority_emission_per_block();

			assert!(total_auth_reward > 0u128);
			assert!(auth_cut > 0u128);
			assert!(delegators_cut > 0u128);
			assert!(op_cut > 0u128);
			assert!(FlipBalance::abs_diff(delegators_cut, op_cut * 19) < 10); // 5/95 split

			let total_pre_balance =
				auth_pre_balance + total_delegators_pre_balance + op_pre_balance;

			// Verify that rewards are distributed accordingly.
			assert_eq!(
				PerU16::from_rational(auth_cut, total_auth_reward,),
				PerU16::from_rational(auth_pre_balance, total_pre_balance)
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
			let delegators = BTreeMap::from_iter([
				(AccountId::from([0xA0; 32]), 100_000u128 * FLIPPERINOS_PER_FLIP),
				(AccountId::from([0xA1; 32]), 400_000u128 * FLIPPERINOS_PER_FLIP),
				(AccountId::from([0xA2; 32]), 500_000u128 * FLIPPERINOS_PER_FLIP),
			]);
			setup_delegation(
				&mut testnet,
				auth.clone(),
				operator.clone(),
				500, // 5%
				delegators.clone(),
			);

			// Set Validator as "offline" and reduce reputation
			pallet_cf_reputation::Reputations::<Runtime>::mutate(&auth, |rep| {
				rep.online_blocks = Zero::zero();
				rep.reputation_points = -1;
			});
			testnet.set_active(&auth, false);
			testnet.set_auto_heartbeat(&auth, false);

			// Move to the block before the heartbeat.
			testnet.move_forward_blocks(
				HEARTBEAT_BLOCK_INTERVAL - System::block_number() % HEARTBEAT_BLOCK_INTERVAL - 1,
			);

			// Update pre-balance
			let auth_pre_balance = Flip::balance(&auth);
			let op_pre_balance = Flip::balance(&operator);
			let total_delegators_pre_balance: FlipBalance =
				delegators.keys().map(Flip::balance).sum();

			// Move forward 1 block so that the inactive validator is slashed
			testnet.move_forward_blocks(1);

			let auth_post_balance = Flip::balance(&auth);
			let auth_slash = auth_pre_balance - auth_post_balance;

			let op_post_balance = Flip::balance(&operator);
			let op_slash = op_pre_balance - op_post_balance;

			let total_delegators_post_balance: FlipBalance =
				delegators.keys().map(Flip::balance).sum();
			let delegators_slash = total_delegators_pre_balance - total_delegators_post_balance;

			assert!(auth_slash > 0u128);
			assert!(delegators_slash > 0u128);
			assert!(FlipBalance::abs_diff(op_slash * 19, delegators_slash) < 10); // 5/95 split

			let total_pre_balance =
				auth_pre_balance + total_delegators_pre_balance + op_pre_balance;
			let total_slashing = auth_slash + delegators_slash + op_slash;

			// Verify that slashings are distributed accordingly
			assert_eq!(
				PerU16::from_rational(auth_slash, total_slashing),
				PerU16::from_rational(auth_pre_balance, total_pre_balance)
			);
		});
}
