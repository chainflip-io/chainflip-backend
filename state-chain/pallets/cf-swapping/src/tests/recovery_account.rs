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

use super::*;
use crate::{BrokerRecoveryAccount, DeleteRecoveryAccount};
use cf_primitives::AccountRole;
use cf_traits::{mocks::account_role_registry::MockAccountRoleRegistry, BrokerRecoveryApi};
use frame_support::traits::OnKilledAccount;

const LP: u64 = 1_001;
const OTHER_LP: u64 = 1_002;

fn register_lps() {
	for lp in [LP, OTHER_LP] {
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(&lp)
			.unwrap();
	}
}

fn link(broker: u64, recovery_account: u64) -> DispatchResult {
	Swapping::link_recovery_account(OriginTrait::signed(broker), recovery_account)
}

#[test]
fn link_recovery_account() {
	new_test_ext().execute_with(|| {
		register_lps();

		assert_ok!(link(BROKER, LP));

		assert_eq!(BrokerRecoveryAccount::<Test>::get(BROKER), Some(LP));
		assert_eq!(Swapping::recovery_account(&BROKER), Some(LP));
		System::assert_last_event(RuntimeEvent::Swapping(Event::RecoveryAccountLinked {
			broker: BROKER,
			recovery_account: LP,
		}));
	});
}

#[test]
fn second_link_is_rejected() {
	new_test_ext().execute_with(|| {
		register_lps();
		assert_ok!(link(BROKER, LP));

		for lp in [LP, OTHER_LP] {
			assert_noop!(link(BROKER, lp), Error::<Test>::RecoveryAccountAlreadyLinked);
		}
		assert_eq!(BrokerRecoveryAccount::<Test>::get(BROKER), Some(LP));
	});
}

#[test]
fn only_brokers_can_link() {
	new_test_ext().execute_with(|| {
		register_lps();

		assert_noop!(link(OTHER_LP, LP), DispatchError::BadOrigin);
	});
}

#[test]
fn linking_to_a_non_lp_account_is_rejected() {
	new_test_ext().execute_with(|| {
		const UNREGISTERED: u64 = 1_003;

		for account in [UNREGISTERED, BROKER] {
			assert_noop!(link(BROKER, account), Error::<Test>::RecoveryAccountNotLiquidityProvider);
		}
	});
}

#[test]
fn deregistered_recovery_account_is_ignored() {
	new_test_ext().execute_with(|| {
		register_lps();
		assert_ok!(link(BROKER, LP));

		assert_ok!(
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::deregister_account_role(
				&LP,
				AccountRole::LiquidityProvider
			)
		);

		assert_eq!(Swapping::recovery_account(&BROKER), None);
	});
}

#[test]
fn link_survives_broker_deregistration() {
	new_test_ext().execute_with(|| {
		register_lps();
		assert_ok!(link(BROKER, LP));

		assert_ok!(Swapping::deregister_as_broker(OriginTrait::signed(BROKER)));
		assert_eq!(BrokerRecoveryAccount::<Test>::get(BROKER), Some(LP));

		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(&BROKER)
			.unwrap();
		assert_noop!(link(BROKER, OTHER_LP), Error::<Test>::RecoveryAccountAlreadyLinked);
		assert_eq!(Swapping::recovery_account(&BROKER), Some(LP));
	});
}

#[test]
fn link_is_removed_when_the_broker_account_is_killed() {
	new_test_ext().execute_with(|| {
		register_lps();
		assert_ok!(link(BROKER, LP));

		DeleteRecoveryAccount::<Test>::on_killed_account(&BROKER);

		assert_eq!(BrokerRecoveryAccount::<Test>::get(BROKER), None);
	});
}
