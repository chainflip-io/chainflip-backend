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

use chainflip_node::chain_spec::devnet::{HEARTBEAT_BLOCK_INTERVAL, YEAR};
use sp_std::collections::btree_set::BTreeSet;

use crate::{
	genesis::GENESIS_BALANCE,
	network::{self, new_account, Network},
	AccountId, AuthorityCount, RuntimeOrigin,
};

use cf_primitives::{AccountRole, AssetAmount, FlipBalance};
use cf_traits::{AccountInfo, EpochInfo};
use frame_support::assert_ok;
use pallet_cf_validator::{DelegationAcceptance, OperatorSettings};
use sp_runtime::{traits::Zero, FixedPointNumber, FixedU64, PerU16, Permill};
use state_chain_runtime::{
	chainflip::calculate_account_apy, Balance, Emissions, Flip, Funding, Runtime, RuntimeEvent,
	System, Validator,
};

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
		pallet_cf_validator::OperatorChoice::<Runtime>::get(&validator),
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
	testnet.move_to_the_next_epoch();
	assert!(pallet_cf_validator::CurrentAuthorities::<Runtime>::get().contains(&validator));
	assert!(Flip::balance(&validator) < pallet_cf_validator::Bond::<Runtime>::get());
}

#[test]
fn block_author_rewards_are_distributed_among_delegators() {
	const EPOCH_DURATION_BLOCKS: u32 = 200;
	const MAX_AUTHORITIES: AuthorityCount = 3;
	let managed_validator: AccountId = AccountId::from([0xcf; 32]);
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[(
			managed_validator.clone(),
			AccountRole::Validator,
			GENESIS_BALANCE / 5,
		)])
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);

			testnet.move_to_the_next_epoch();

			let operator = AccountId::from([0xe1; 32]);

			// Setup 3 delegator, operator and association with validator.
			let delegators = BTreeMap::from_iter([
				(AccountId::from([0xA0; 32]), 2 * GENESIS_BALANCE),
				(AccountId::from([0xA1; 32]), 4 * GENESIS_BALANCE),
				(AccountId::from([0xA2; 32]), 5 * GENESIS_BALANCE),
			]);
			setup_delegation(
				&mut testnet,
				managed_validator.clone(),
				operator.clone(),
				2000, // 20%
				delegators.clone(),
			);

			testnet.move_to_the_next_epoch();

			let epoch_index = pallet_cf_validator::Pallet::<Runtime>::epoch_index();
			let snapshot =
				pallet_cf_validator::DelegationSnapshots::<Runtime>::get(epoch_index, &operator)
					.expect("Snapshot should be registered on new epoch");
			assert!(
				pallet_cf_validator::ValidatorToOperator::<Runtime>::get(
					epoch_index,
					&managed_validator
				)
				.expect("Validator should be mapped to operator") ==
					operator
			);
			assert!(
				snapshot.validators.len() == 1 &&
					snapshot.validators.keys().any(|v| *v == managed_validator) &&
					snapshot.delegators == delegators,
				"Bad snapshot: {:#?}",
				snapshot,
			);

			let validator_pre_balance = Flip::balance(&managed_validator);
			let operator_pre_balance = Flip::balance(&operator);
			let total_delegators_pre_balance: FlipBalance =
				delegators.keys().map(Flip::balance).sum();

			// Move forward through all authorities to ensure the reward is distributed once.
			testnet.move_forward_blocks(
				pallet_cf_validator::CurrentAuthorities::<Runtime>::decode_len()
					.expect("at least one authority") as u32,
			);

			let validator_post_balance = Flip::balance(&managed_validator);
			let validator_cut = validator_post_balance - validator_pre_balance;

			let operator_post_balance = Flip::balance(&operator);
			let operator_cut = operator_post_balance - operator_pre_balance;

			let total_delegators_post_balance: FlipBalance =
				delegators.keys().map(Flip::balance).sum();
			let delegators_cut = total_delegators_post_balance - total_delegators_pre_balance;

			let block_reward =
				pallet_cf_emissions::Pallet::<Runtime>::current_authority_emission_per_block();

			let epsilon = block_reward / 10_000; // allow 0.01% error due to rounding

			assert!(block_reward > 0u128);
			assert!(validator_cut > 0u128);
			assert!(delegators_cut > 0u128);
			assert!(operator_cut > 0u128);
			assert!(FlipBalance::abs_diff(delegators_cut, operator_cut * 4) < epsilon,); // 20/80 split

			// Verify that rewards are distributed according to portion of bond..
			assert_eq!(
				PerU16::from_rational(validator_cut, block_reward,),
				PerU16::from_rational(
					validator_pre_balance,
					pallet_cf_validator::Bond::<Runtime>::get(),
				)
			);
		});
}

#[test]
fn slashings_are_distributed_among_delegators() {
	const EPOCH_DURATION_BLOCKS: u32 = 1_000;
	const MAX_AUTHORITIES: AuthorityCount = 3;
	let managed_validator: AccountId = AccountId::from([0xcf; 32]);
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[(
			managed_validator.clone(),
			AccountRole::Validator,
			GENESIS_BALANCE / 5,
		)])
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);

			let operator: sp_runtime::AccountId32 = AccountId::from([0xe1; 32]);
			// Setup 3 delegator, operator and association with validator.
			let delegators = BTreeMap::from_iter([
				(AccountId::from([0xA0; 32]), 2 * GENESIS_BALANCE),
				(AccountId::from([0xA1; 32]), 4 * GENESIS_BALANCE),
				(AccountId::from([0xA2; 32]), 5 * GENESIS_BALANCE),
			]);
			setup_delegation(
				&mut testnet,
				managed_validator.clone(),
				operator.clone(),
				2000, // 20%
				delegators.clone(),
			);

			// Set Validator as "offline" and reduce reputation
			pallet_cf_reputation::Reputations::<Runtime>::mutate(&managed_validator, |rep| {
				rep.online_blocks = Zero::zero();
				rep.reputation_points = -1;
			});
			testnet.set_active(&managed_validator, false);
			testnet.set_auto_heartbeat(&managed_validator, false);

			// Move to the block before the heartbeat.
			testnet.move_forward_blocks(
				HEARTBEAT_BLOCK_INTERVAL - System::block_number() % HEARTBEAT_BLOCK_INTERVAL - 1,
			);

			// Update pre-balance
			let validator_pre_balance = Flip::balance(&managed_validator);

			// Move forward 1 block so that the inactive validator is slashed
			testnet.move_forward_blocks(1);

			let mut slashes = frame_system::Pallet::<Runtime>::events()
				.into_iter()
				.filter_map(|e| match e.event {
					RuntimeEvent::Flip(pallet_cf_flip::Event::SlashingPerformed {
						who,
						amount,
					}) => Some((who, amount)),
					_ => None,
				})
				.collect::<BTreeMap<_, _>>();

			assert!(slashes.contains_key(&managed_validator));
			assert!(slashes.contains_key(&operator));
			for d in delegators.keys() {
				assert!(slashes.contains_key(d));
			}

			let validator_slash = slashes.remove(&managed_validator).unwrap();
			let operator_slash = slashes.remove(&operator).unwrap();
			let delegators_slash = slashes.values().sum::<FlipBalance>();

			let total_slashing = validator_slash + delegators_slash + operator_slash;

			let epsilon = total_slashing / 10_000; // allow 0.01% error due to rounding

			assert!(validator_slash > 0u128);
			assert!(delegators_slash > 0u128);
			assert!(
				FlipBalance::abs_diff(operator_slash * 4, delegators_slash) < epsilon,
				"Expected to be within {} but got a diff of {}",
				epsilon,
				FlipBalance::abs_diff(operator_slash * 4, delegators_slash)
			); // 20/80 split

			// Verify that slashings are distributed according to portion of bond.
			assert_eq!(
				PerU16::from_rational(validator_slash, total_slashing),
				PerU16::from_rational(
					validator_pre_balance,
					pallet_cf_validator::Bond::<Runtime>::get()
				)
			);
		});
}

#[test]
fn can_calculate_account_apy_for_validator_with_delegation() {
	const EPOCH_BLOCKS: u32 = 1_000;
	const MAX_AUTHORITIES: u32 = 10;
	const NUM_BACKUPS: u32 = 20;

	// Validator balance is lower than that of other nodes to make sure that it
	// will be sharing rewards with the delegator.
	const VALIDATOR_BALANCE: AssetAmount = GENESIS_BALANCE / 5;

	let validator: AccountId = AccountId::from([0xaa; 32]);

	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[(validator.clone(), AccountRole::Validator, VALIDATOR_BALANCE)])
		.build()
		.execute_with(|| {
			let (mut network, _, _) =
				crate::authorities::fund_authorities_and_join_auction(NUM_BACKUPS);

			let operator = AccountId::from([0xe1; 32]);
			let delegator = AccountId::from([0xA0; 32]);

			crate::delegation::setup_delegation(
				&mut network,
				validator.clone(),
				operator.clone(),
				2500, // 25% operator fee
				BTreeMap::from_iter([(delegator, GENESIS_BALANCE * 2)]),
			);

			let validator_apy = calculate_account_apy(&validator).unwrap();

			let expected_apy_basis_point = {
				let total_reward = Emissions::current_authority_emission_per_block() * YEAR as u128 /
					MAX_AUTHORITIES as u128;
				let validator_reward =
					Permill::from_rational(VALIDATOR_BALANCE, GENESIS_BALANCE * 2) * total_reward;
				let validator_balance = Flip::balance(&validator);

				FixedU64::from_rational(validator_reward, validator_balance)
					.checked_mul_int(10_000u32)
					.unwrap()
			};

			assert_eq!(validator_apy, expected_apy_basis_point);
		});
}
