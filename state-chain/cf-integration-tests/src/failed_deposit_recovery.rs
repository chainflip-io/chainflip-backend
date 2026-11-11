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

//! Failed deposits recovered to the brokers' recovery accounts.

use crate::{genesis, network::register_refund_addresses};
use cf_chains::ForeignChainAddress;
use cf_primitives::{AccountId, AccountRole, Asset, FLIPPERINOS_PER_FLIP};
use cf_traits::FailedDepositRecoveryOutcome;
use frame_support::{assert_ok, traits::OnKilledAccount};
use pallet_cf_asset_balances::FreeBalances;
use pallet_cf_environment::FailedDepositRecoveryRequest;
use pallet_cf_ingress_egress::{FetchOrTransfer, ScheduledEgressFetchOrTransfer};
use state_chain_runtime::{
	Environment, EthereumInstance, Runtime, RuntimeEvent, RuntimeOrigin, Swapping, System,
};

const BROKER: AccountId = AccountId::new([0x31; 32]);
const UNLINKED_BROKER: AccountId = AccountId::new([0x32; 32]);
const RECOVERY_ACCOUNT: AccountId = AccountId::new([0x33; 32]);

const ETH_AMOUNT: u128 = 10u128.pow(18);
const ARB_USDC_AMOUNT: u128 = 1_000_000;

fn request(
	broker: AccountId,
	asset: Asset,
	amount: u128,
	reference: &[u8],
) -> FailedDepositRecoveryRequest<AccountId> {
	FailedDepositRecoveryRequest {
		broker,
		asset,
		amount,
		reference: reference.to_vec().try_into().unwrap(),
	}
}

#[test]
fn batch_of_mixed_outcomes() {
	genesis::with_test_defaults()
		.with_additional_accounts(&[
			(BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(UNLINKED_BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(RECOVERY_ACCOUNT, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			// Refund addresses on Ethereum, Polkadot, Bitcoin and Solana, but not Arbitrum.
			register_refund_addresses(&RECOVERY_ACCOUNT);
			assert_ok!(Swapping::link_recovery_account(
				RuntimeOrigin::signed(BROKER),
				RECOVERY_ACCOUNT
			));
			System::reset_events();

			assert_ok!(Environment::recover_failed_deposits(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				vec![
					request(BROKER, Asset::Eth, ETH_AMOUNT, b"eth"),
					// Doesn't cover the egress fee.
					request(BROKER, Asset::Btc, 1, b"btc"),
					// No refund address.
					request(BROKER, Asset::ArbUsdc, ARB_USDC_AMOUNT, b"arb"),
					request(UNLINKED_BROKER, Asset::Usdc, 1_000_000, b"usdc"),
					// Polkadot is deprecated.
					request(BROKER, Asset::Dot, 1_000, b"dot"),
				]
				.try_into()
				.unwrap(),
			));

			let outcomes = System::events()
				.into_iter()
				.filter_map(|record| match record.event {
					RuntimeEvent::Environment(
						pallet_cf_environment::Event::FailedDepositRecovered {
							recovery_account,
							outcome,
							reference,
							..
						},
					) => {
						assert_eq!(recovery_account, RECOVERY_ACCOUNT);
						Some((reference.into_inner(), Some(outcome)))
					},
					RuntimeEvent::Environment(
						pallet_cf_environment::Event::FailedDepositRecoveryFailed {
							reference, ..
						},
					) => Some((reference.into_inner(), None)),
					_ => None,
				})
				.collect::<Vec<_>>();

			assert!(
				matches!(
					&outcomes[..],
					[
						(
							_,
							Some(FailedDepositRecoveryOutcome::Egressed {
								destination: ForeignChainAddress::Eth(_),
								..
							})
						),
						(_, Some(FailedDepositRecoveryOutcome::Credited)),
						(_, Some(FailedDepositRecoveryOutcome::Credited)),
						(_, None),
						(_, None),
					]
				),
				"{outcomes:?}"
			);
			assert_eq!(
				outcomes.iter().map(|(reference, _)| reference.as_slice()).collect::<Vec<_>>(),
				vec![&b"eth"[..], b"btc", b"arb", b"usdc", b"dot"]
			);

			assert!(ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::get().iter().any(
				|request| matches!(
					request,
					FetchOrTransfer::Transfer { destination_address, .. }
						if destination_address.is_zero()
				)
			));
			assert_eq!(FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Eth), 0);
			assert_eq!(FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Btc), 1);
			assert_eq!(
				FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::ArbUsdc),
				ARB_USDC_AMOUNT
			);
			assert_eq!(FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Usdc), 0);
		});
}

#[test]
fn link_survives_deregistration_and_is_removed_when_the_account_is_killed() {
	genesis::with_test_defaults()
		.with_additional_accounts(&[
			(BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(RECOVERY_ACCOUNT, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			assert_ok!(Swapping::link_recovery_account(
				RuntimeOrigin::signed(BROKER),
				RECOVERY_ACCOUNT
			));

			assert_ok!(Swapping::deregister_as_broker(RuntimeOrigin::signed(BROKER)));
			assert_eq!(
				pallet_cf_swapping::BrokerRecoveryAccount::<Runtime>::get(&BROKER),
				Some(RECOVERY_ACCOUNT)
			);

			<Runtime as frame_system::Config>::OnKilledAccount::on_killed_account(&BROKER);

			assert_eq!(pallet_cf_swapping::BrokerRecoveryAccount::<Runtime>::get(&BROKER), None);
		});
}
