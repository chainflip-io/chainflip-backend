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

use crate::genesis::GENESIS_BALANCE;

use super::{genesis, network, *};
use cf_primitives::GENESIS_EPOCH;
use cf_traits::{offence_reporting::OffenceReporter, AccountInfo, EpochInfo};
use mock_runtime::MIN_FUNDING;
use pallet_cf_funding::pallet::Error;
use pallet_cf_validator::CurrentRotationPhase;
use sp_runtime::{FixedPointNumber, FixedU64};
use state_chain_runtime::chainflip::{calculate_account_apy, Offence};

#[test]
// Nodes cannot redeem when we are out of the redeeming period (50% of the epoch)
// We have a set of nodes that are funded and can redeem in the redeeming period and
// not redeem when out of the period
fn cannot_redeem_funds_out_of_redemption_period() {
	const EPOCH_DURATION_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 3;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let mut nodes = Validator::current_authorities();
			let (mut testnet, mut extra_nodes) = network::Network::create(0, &nodes);

			for extra_node in extra_nodes.clone() {
				network::Cli::start_bidding(&extra_node);
			}

			nodes.append(&mut extra_nodes);

			// Fund these nodes so that they are included in the next epoch
			let funding_amount = genesis::GENESIS_BALANCE;
			for node in &nodes {
				testnet.state_chain_gateway_contract.fund_account(node.clone(), funding_amount);
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
				EPOCH_DURATION_BLOCKS * REDEMPTION_PERIOD_AS_PERCENTAGE as u32 / 100;
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
					pallet_cf_validator::Error::<Runtime>::StillBidding
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
fn can_calculate_account_apy() {
	const EPOCH_BLOCKS: u32 = 1_000;
	const MAX_AUTHORITIES: u32 = 10;
	const NUM_BACKUPS: u32 = 20;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut network, _, _) =
				crate::authorities::fund_authorities_and_join_auction(NUM_BACKUPS);
			network.move_to_the_next_epoch();

			let validator = Validator::current_authorities().into_iter().next().unwrap();

			// Normal account returns None
			let no_reward = AccountId::from([0xff; 32]);
			assert!(!Validator::current_authorities().contains(&no_reward));
			assert!(calculate_account_apy(&no_reward).is_none());

			// APY rate is correct for current Authority
			let total = Flip::balance(&validator);
			let reward = Emissions::current_authority_emission_per_block() * YEAR as u128 / 10u128;
			let apy_basis_point =
				FixedU64::from_rational(reward, total).checked_mul_int(10_000u32).unwrap();
			assert_eq!(apy_basis_point, 49u32);
			assert_eq!(calculate_account_apy(&validator), Some(apy_basis_point));
		});
}

#[test]
fn apy_can_be_above_100_percent() {
	const EPOCH_BLOCKS: u32 = 1_000;
	const MAX_AUTHORITIES: u32 = 2;
	const NUM_BACKUPS: u32 = 2;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
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
			assert_eq!(apy_basis_point, 241_377_726u32);
			assert_eq!(calculate_account_apy(&validator), Some(apy_basis_point));
		});
}

#[test]
fn min_auction_bid_qualification() {
	const GENESIS_BALANCE_IN_FLIP: u32 = (GENESIS_BALANCE / FLIPPERINOS_PER_FLIP) as u32;
	super::genesis::with_test_defaults().build().execute_with(|| {
		let _ = crate::authorities::fund_authorities_and_join_auction(0);

		assert_ok!(Validator::update_pallet_config(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			pallet_cf_validator::PalletConfigUpdate::MinimumValidatorStake {
				min_stake: GENESIS_BALANCE_IN_FLIP
			}
		));
		assert!(
			Validator::get_qualified_bidders::<
				<Runtime as pallet_cf_validator::Config>::KeygenQualification,
			>()
			.len() == Validator::current_authorities().len(),
			"All genesis authorities should be qualified as bidders."
		);
		assert_ok!(Validator::update_pallet_config(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			pallet_cf_validator::PalletConfigUpdate::MinimumValidatorStake {
				min_stake: GENESIS_BALANCE_IN_FLIP + 1
			}
		));
		assert!(
			Validator::get_qualified_bidders::<
				<Runtime as pallet_cf_validator::Config>::KeygenQualification,
			>()
			.is_empty(),
			"No authorities should be qualified if minimum stake is above their balance. Qualified bidders: {:?}",
			Validator::get_qualified_bidders::<
				<Runtime as pallet_cf_validator::Config>::KeygenQualification,
			>()
		);
	});
}
