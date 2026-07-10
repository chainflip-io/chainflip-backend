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

//! Tests for FLIP 2.1 fee-reward distribution at epoch transitions.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
	delegation::setup_delegation, genesis::GENESIS_BALANCE, network, AccountId, AuthorityCount,
	VAULT_ROTATION_BLOCKS,
};
use cf_primitives::{FlipBalance, FLIPPERINOS_PER_FLIP};
use cf_traits::{AccountInfo, EpochInfo, FeePayment, FundAccount, FundingSource};
use frame_support::assert_ok;
use pallet_cf_flip::PalletConfigUpdate;
use pallet_cf_validator::{DelegationAmount, DelegationSnapshots, HistoricalBonds};
use state_chain_runtime::{Flip, Funding, Runtime, RuntimeEvent, RuntimeOrigin, System, Validator};

/// Steps through the rotation one block at a time and returns the events of the block in which
/// the epoch index was incremented, ie. the block in which `on_new_epoch` runs.
fn move_through_epoch_transition(testnet: &mut network::Network) -> Vec<RuntimeEvent> {
	let epoch = Validator::epoch_index();
	testnet.move_to_the_end_of_epoch();
	for _ in 0..=VAULT_ROTATION_BLOCKS + 1 {
		testnet.move_forward_blocks(1);
		if Validator::epoch_index() == epoch + 1 {
			return System::events().into_iter().map(|record| record.event).collect();
		}
	}
	panic!("Epoch did not rotate");
}

#[test]
fn fee_rewards_are_burned_before_activation_and_distributed_after() {
	const EPOCH_DURATION_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 3;

	const ONCHAIN_FEE: FlipBalance = 100 * FLIPPERINOS_PER_FLIP;
	// Chosen so that the total is not a multiple of MAX_AUTHORITIES: the remainder should stay in
	// the reserve rather than being distributed.
	const OFFCHAIN_FEE: FlipBalance = 200 * FLIPPERINOS_PER_FLIP + 1;
	const PER_AUTHORITY_REWARD: FlipBalance =
		(ONCHAIN_FEE + OFFCHAIN_FEE) / MAX_AUTHORITIES as u128;
	const REMAINDER: FlipBalance = (ONCHAIN_FEE + OFFCHAIN_FEE) % MAX_AUTHORITIES as u128;

	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			testnet.move_to_the_next_epoch();

			// An unbonded account from which fees can be taken.
			let fee_payer = AccountId::from([0xfe; 32]);
			Funding::fund_account(
				fee_payer.clone(),
				1_000 * FLIPPERINOS_PER_FLIP,
				FundingSource::EthTransaction {
					tx_hash: Default::default(),
					funder: Default::default(),
				},
			);

			// Pre-activation, fees are burned and nothing accrues to the distribution reserve.
			let issuance_before = pallet_cf_flip::TotalIssuance::<Runtime>::get();
			assert_ok!(<Flip as FeePayment>::try_take_fee(&fee_payer, ONCHAIN_FEE));
			assert_eq!(
				pallet_cf_flip::TotalIssuance::<Runtime>::get(),
				issuance_before - ONCHAIN_FEE
			);
			assert_eq!(Flip::pending_rewards(), 0);

			// Activate FLIP 2.1 from the next epoch.
			let activation_epoch = Validator::epoch_index() + 1;
			assert_ok!(Flip::update_pallet_config(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				vec![PalletConfigUpdate::SetFeeRewardsActivationEpoch(activation_epoch)]
					.try_into()
					.unwrap(),
			));

			// The transition into the activation epoch forces a final supply sync and does not
			// distribute anything.
			let events = move_through_epoch_transition(&mut testnet);
			assert_eq!(Validator::epoch_index(), activation_epoch);
			assert!(
				events.iter().any(|event| matches!(
					event,
					RuntimeEvent::Emissions(
						pallet_cf_emissions::Event::SupplyUpdateBroadcastRequested(..)
					)
				)),
				"Expected a forced supply update at the activation epoch transition",
			);
			assert!(!events.iter().any(|event| matches!(
				event,
				RuntimeEvent::Flip(pallet_cf_flip::Event::FlipDistributed { .. })
			)));

			// Post-activation, on-chain fees accrue to the distribution reserve instead of being
			// burned...
			let issuance_before = pallet_cf_flip::TotalIssuance::<Runtime>::get();
			assert_ok!(<Flip as FeePayment>::try_take_fee(&fee_payer, ONCHAIN_FEE));
			assert_eq!(pallet_cf_flip::TotalIssuance::<Runtime>::get(), issuance_before);
			assert_eq!(Flip::pending_rewards(), ONCHAIN_FEE as i128);

			// ...and off-chain fees (eg. FLIP bought with swap network fees) accumulate for the
			// same distribution.
			<Flip as FeePayment>::add_to_offchain_flip_to_be_distributed(OFFCHAIN_FEE as i128);
			assert_eq!(Flip::pending_rewards(), (ONCHAIN_FEE + OFFCHAIN_FEE) as i128);

			let authorities: BTreeSet<AccountId> =
				Validator::current_authorities().into_iter().collect();
			assert_eq!(authorities.len(), MAX_AUTHORITIES as usize);
			let pre_rotation_balances: BTreeMap<AccountId, FlipBalance> = authorities
				.iter()
				.map(|account_id| (account_id.clone(), Flip::balance(account_id)))
				.collect();

			// At the next epoch transition, the accrued fees are distributed evenly among the
			// authorities of the epoch that just ended.
			let events = move_through_epoch_transition(&mut testnet);

			assert!(
				events.contains(&RuntimeEvent::Flip(pallet_cf_flip::Event::FlipDistributed {
					amount: authorities
						.iter()
						.map(|account_id| (account_id.clone(), PER_AUTHORITY_REWARD))
						.collect(),
				})),
				"Expected an even distribution to all authorities, got: {:#?}",
				events
					.iter()
					.filter(|event| matches!(event, RuntimeEvent::Flip(..)))
					.collect::<Vec<_>>(),
			);
			assert!(
				!events.iter().any(|event| matches!(
					event,
					RuntimeEvent::Flip(pallet_cf_flip::Event::RemainingImbalance { .. })
				)),
				"Rewards were not fully settled",
			);
			// The off-chain portion that was bridged in is egressed to the gateway to back the
			// newly credited on-chain balances.
			assert!(
				events.iter().any(|event| matches!(
					event,
					RuntimeEvent::Swapping(pallet_cf_swapping::Event::SentFlipToGateway {
						amount,
						..
					}) if *amount == OFFCHAIN_FEE
				)),
				"Expected the off-chain portion to be sent to the gateway",
			);

			// Authorities' balances increased by at least the reward (block rewards accrue on
			// top).
			for (account_id, pre_rotation_balance) in &pre_rotation_balances {
				assert!(
					Flip::balance(account_id) >= pre_rotation_balance + PER_AUTHORITY_REWARD,
					"Reward was not credited to authority {:?}",
					account_id,
				);
			}

			// The remainder stays pending for the next distribution.
			assert_eq!(Flip::pending_rewards(), REMAINDER as i128);
		});
}

/// Fees accrued during epoch N are distributed at the transition to epoch N+1. The distribution
/// must be based on epoch N's delegation snapshot and bond: a delegator who backed the whole of
/// epoch N gets a share even if they undelegated in the meantime, a delegator who only joined
/// during epoch N (first active in epoch N+1) does not get paid out of fees they never backed,
/// and the validator/delegator split is computed against epoch N's bond even if the auction
/// outcome changes the bond at the transition.
#[test]
fn fee_rewards_are_split_according_to_the_snapshot_and_bond_of_the_epoch_in_which_they_accrued() {
	const EPOCH_DURATION_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 3;
	const ONCHAIN_FEE: FlipBalance = 100 * FLIPPERINOS_PER_FLIP;

	let managed_validator = AccountId::from([0xcf; 32]);
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[(
			managed_validator.clone(),
			cf_primitives::AccountRole::Validator,
			GENESIS_BALANCE / 5,
		)])
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			testnet.move_to_the_next_epoch();

			let operator = AccountId::from([0xe1; 32]);
			let departing_delegator = AccountId::from([0xa0; 32]);
			let joining_delegator = AccountId::from([0xa1; 32]);

			setup_delegation(
				&mut testnet,
				managed_validator.clone(),
				operator.clone(),
				2000, // 20%
				[(departing_delegator.clone(), 2 * GENESIS_BALANCE)].into(),
			);

			// Activate FLIP 2.1 as of the current epoch and accrue some fees.
			let fee_epoch = Validator::epoch_index();
			assert_ok!(Flip::update_pallet_config(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				vec![PalletConfigUpdate::SetFeeRewardsActivationEpoch(fee_epoch)]
					.try_into()
					.unwrap(),
			));
			let fee_payer = AccountId::from([0xfe; 32]);
			Funding::fund_account(
				fee_payer.clone(),
				1_000 * FLIPPERINOS_PER_FLIP,
				FundingSource::EthTransaction {
					tx_hash: Default::default(),
					funder: Default::default(),
				},
			);
			assert_ok!(<Flip as FeePayment>::try_take_fee(&fee_payer, ONCHAIN_FEE));

			let fee_epoch_authorities = Validator::current_authorities();

			// Mid-epoch, the delegation structure changes: the original delegator leaves and a
			// new one joins. This only takes effect in the next epoch's snapshot.
			assert_ok!(Validator::undelegate(
				RuntimeOrigin::signed(departing_delegator.clone()),
				DelegationAmount::Max,
			));
			Funding::fund_account(
				joining_delegator.clone(),
				2 * GENESIS_BALANCE,
				FundingSource::EthTransaction {
					tx_hash: Default::default(),
					funder: Default::default(),
				},
			);
			assert_ok!(Validator::delegate(
				RuntimeOrigin::signed(joining_delegator.clone()),
				operator.clone(),
				DelegationAmount::Max,
			));

			// Also raise the solo authorities' bids so that the next auction resolves to a
			// different bond.
			for authority in &fee_epoch_authorities {
				if *authority != managed_validator {
					Funding::fund_account(
						authority.clone(),
						GENESIS_BALANCE,
						FundingSource::EthTransaction {
							tx_hash: Default::default(),
							funder: Default::default(),
						},
					);
				}
			}

			let events = move_through_epoch_transition(&mut testnet);

			// Sanity check the scenario: the two epochs' snapshots contain different delegators,
			// and the auction resolved to a different bond.
			for (epoch, delegator) in
				[(fee_epoch, &departing_delegator), (fee_epoch + 1, &joining_delegator)]
			{
				assert_eq!(
					DelegationSnapshots::<Runtime>::get(epoch, &operator)
						.expect("snapshot should exist")
						.delegators
						.keys()
						.collect::<Vec<_>>(),
					vec![delegator],
					"Unexpected delegation snapshot for epoch {}",
					epoch,
				);
			}
			let fee_epoch_bond = HistoricalBonds::<Runtime>::get(fee_epoch);
			assert_ne!(
				fee_epoch_bond,
				HistoricalBonds::<Runtime>::get(fee_epoch + 1),
				"The bond should change at the transition for this test to be conclusive",
			);

			let distributed: std::collections::BTreeMap<AccountId, FlipBalance> = events
				.iter()
				.find_map(|event| match event {
					RuntimeEvent::Flip(pallet_cf_flip::Event::FlipDistributed { amount }) =>
						Some(amount.iter().cloned().collect()),
					_ => None,
				})
				.expect("rewards should be distributed at the epoch transition");

			assert!(
				distributed.get(&departing_delegator).copied().unwrap_or_default() > 0,
				"The delegator who backed the fee epoch should receive a share, got: {:#?}",
				distributed,
			);
			assert!(
				!distributed.contains_key(&joining_delegator),
				"The delegator who joined mid-epoch must not be paid out of fees accrued \
				before their delegation was active, got: {:#?}",
				distributed,
			);

			// The exact payout must be the fee epoch's snapshot distributed against the fee
			// epoch's bond.
			let per_authority_reward = ONCHAIN_FEE / MAX_AUTHORITIES as u128;
			let mut expected = BTreeMap::new();
			for authority in &fee_epoch_authorities {
				if *authority == managed_validator {
					for (account, amount) in
						DelegationSnapshots::<Runtime>::get(fee_epoch, &operator)
							.expect("snapshot should exist")
							.distribute(per_authority_reward, fee_epoch_bond)
					{
						if amount > 0 {
							*expected.entry(account.clone()).or_default() += amount;
						}
					}
				} else {
					expected.insert(authority.clone(), per_authority_reward);
				}
			}
			assert_eq!(distributed, expected);
		});
}
