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

use pallet_cf_reputation::HeartbeatQualification;
use sp_std::collections::btree_set::BTreeSet;

use crate::mock_runtime::{ExtBuilder, CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL};

use super::*;
use cf_primitives::AccountRole;
use cf_traits::{AccountInfo, EpochInfo, QualifyNode};
use state_chain_runtime::{
	BitcoinThresholdSigner, EvmThresholdSigner, PolkadotThresholdSigner, SolanaThresholdSigner,
};
pub const GENESIS_BALANCE: FlipBalance = TOTAL_ISSUANCE / 100;

const EPOCH_DURATION: u32 = 1000;

pub fn with_test_defaults() -> ExtBuilder {
	ExtBuilder::default()
		.accounts(vec![
			(AccountId::from(ALICE), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(BOB), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(CHARLIE), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(BROKER), AccountRole::Broker, GENESIS_BALANCE),
			(AccountId::from(LIQUIDITY_PROVIDER), AccountRole::LiquidityProvider, GENESIS_BALANCE),
		])
		.root(AccountId::from(ERIN))
		.epoch_duration(EPOCH_DURATION)
}

#[test]
fn state_of_genesis_is_as_expected() {
	with_test_defaults().build().execute_with(|| {
		// Confirmation that we have our assumed state at block 1
		assert_eq!(Flip::total_issuance(), TOTAL_ISSUANCE, "we have issued the total issuance");

		let accounts = [AccountId::from(CHARLIE), AccountId::from(BOB), AccountId::from(ALICE)];

		for account in accounts.iter() {
			assert_eq!(<Flip as AccountInfo>::balance(account), GENESIS_BALANCE,);
		}

		assert_eq!(Validator::bond(), GENESIS_BALANCE);
		assert_eq!(
			Validator::current_authorities().iter().collect::<BTreeSet<_>>(),
			accounts.iter().collect::<BTreeSet<_>>(),
			"the validators are those expected at genesis"
		);

		for account in &accounts {
			assert_eq!(
				frame_system::Pallet::<Runtime>::providers(account),
				1,
				"Expected provider count to be incremented on genesis."
			);
			assert_eq!(
				frame_system::Pallet::<Runtime>::consumers(account),
				2,
				"Expected consumer count to be incremented twice on genesis: account roles and session pallets."
			);
		}

		assert_eq!(
			Validator::epoch_duration(),
			EPOCH_DURATION,
			"epochs will not rotate automatically from genesis"
		);

		let current_epoch = Validator::current_epoch();

		for account in accounts.iter() {
			assert!(
				Validator::authority_index(current_epoch, account).is_some(),
				"authority is present in lookup"
			);
		}

		for account in accounts.iter() {
			assert!(
				HeartbeatQualification::<Runtime>::is_qualified(account),
				"Genesis nodes start online"
			);
		}

		assert_eq!(Emissions::last_supply_update_block(), 0, "no emissions");

		assert_eq!(EvmThresholdSigner::ceremony_id_counter(), 0, "no key generation requests");
		assert_eq!(PolkadotThresholdSigner::ceremony_id_counter(), 0, "no key generation requests");
		assert_eq!(BitcoinThresholdSigner::ceremony_id_counter(), 0, "no key generation requests");
		assert_eq!(SolanaThresholdSigner::ceremony_id_counter(), 0, "no key generation requests");

		assert_eq!(
			pallet_cf_environment::EthereumSignatureNonce::<Runtime>::get(),
			0,
			"Global signature nonce should be 0"
		);

		assert!(Governance::members().contains(&AccountId::from(ERIN)), "expected governor");
		assert_eq!(Governance::proposal_id_counter(), 0, "no proposal for governance");

		assert_eq!(
			Emissions::current_authority_emission_inflation(),
			CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
			"invalid emission inflation for authorities"
		);

		for account in &accounts {
			assert_eq!(
				Reputation::reputation(account),
				pallet_cf_reputation::ReputationTracker::<Runtime>::default(),
				"authority shouldn't have reputation points"
			);
		}

		for account in &accounts {
			assert_eq!(
				pallet_cf_account_roles::AccountRoles::<Runtime>::get(account).unwrap(),
				AccountRole::Validator
			);
		}

		for account in accounts.iter() {
			// TODO: Check historical epochs
			assert!(is_current_authority(account), "{} should be a current authority", account);
		}
	});
}
