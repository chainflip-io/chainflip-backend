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
use crate::Error;
use cf_traits::{
	mocks::{
		balance_api::MockRefundAddressRegistry, failed_deposit_recovery::MockBrokerRecoveryApi,
	},
	BalanceApi, FailedDepositRecovery, FailedDepositRecoveryOutcome, RefundAddressRegistry,
};
use frame_support::assert_noop;

const BROKER: u64 = 10;
const RECOVERY_ACCOUNT: u64 = 11;
const REFUND_ADDRESS: ForeignChainAddress = ForeignChainAddress::Eth(H160([7; 20]));
const AMOUNT: AssetAmount = 1_000;
const EGRESS_FEE: AssetAmount = 100;

fn route(
	asset: Asset,
	amount: AssetAmount,
) -> Result<(u64, FailedDepositRecoveryOutcome), DispatchError> {
	<EthereumIngressEgress as FailedDepositRecovery>::route_failed_deposit(&BROKER, asset, amount)
}

fn recovery_balance(asset: Asset) -> AssetAmount {
	MockBalance::get_balance(&RECOVERY_ACCOUNT, asset)
}

fn scheduled_transfers() -> Vec<(EthAsset, AssetAmount, ForeignChainAddress)> {
	ScheduledEgressFetchOrTransfer::<Test, Instance1>::get()
		.into_iter()
		.filter_map(|request| match request {
			FetchOrTransfer::Transfer { asset, amount, destination_address, .. } =>
				Some((asset, amount, ForeignChainAddress::Eth(destination_address))),
			_ => None,
		})
		.collect()
}

#[test]
fn fails_without_a_recovery_account() {
	new_test_ext().execute_with(|| {
		assert_noop!(route(Asset::Eth, AMOUNT), Error::<Test, Instance1>::NoRecoveryAccount);
	});
}

#[test]
fn egresses_to_the_refund_address() {
	new_test_ext().execute_with(|| {
		ChainTracker::<Ethereum>::set_fee(EGRESS_FEE);
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_ACCOUNT);
		MockRefundAddressRegistry::register_liquidity_refund_address(
			&RECOVERY_ACCOUNT,
			REFUND_ADDRESS,
		);

		assert_eq!(
			route(Asset::Eth, AMOUNT),
			Ok((
				RECOVERY_ACCOUNT,
				FailedDepositRecoveryOutcome::Egressed {
					egress_id: (ForeignChain::Ethereum, 1),
					destination: REFUND_ADDRESS,
					egress_amount: AMOUNT - EGRESS_FEE,
					fee_withheld: EGRESS_FEE,
				}
			))
		);
		assert_eq!(
			scheduled_transfers(),
			vec![(EthAsset::Eth, AMOUNT - EGRESS_FEE, REFUND_ADDRESS)]
		);
		assert_eq!(recovery_balance(Asset::Eth), 0);
	});
}

#[test]
fn credits_without_a_refund_address() {
	new_test_ext().execute_with(|| {
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_ACCOUNT);

		assert_eq!(
			route(Asset::Usdc, AMOUNT),
			Ok((RECOVERY_ACCOUNT, FailedDepositRecoveryOutcome::Credited))
		);

		assert!(scheduled_transfers().is_empty());
		assert_eq!(recovery_balance(Asset::Usdc), AMOUNT);
	});
}

#[test]
fn credits_when_the_amount_does_not_cover_the_egress_fee() {
	new_test_ext().execute_with(|| {
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_ACCOUNT);
		MockRefundAddressRegistry::register_liquidity_refund_address(
			&RECOVERY_ACCOUNT,
			REFUND_ADDRESS,
		);

		for fee in [AMOUNT, AMOUNT * 2] {
			ChainTracker::<Ethereum>::set_fee(fee);
			let fees_before =
				MockAssetWithholding::withheld_assets(ForeignChain::Ethereum.gas_asset());

			assert_eq!(
				route(Asset::Eth, AMOUNT),
				Ok((RECOVERY_ACCOUNT, FailedDepositRecoveryOutcome::Credited))
			);

			// The fee withheld by the failed egress is rolled back.
			assert_eq!(
				MockAssetWithholding::withheld_assets(ForeignChain::Ethereum.gas_asset()),
				fees_before
			);
		}

		assert!(scheduled_transfers().is_empty());
		assert_eq!(recovery_balance(Asset::Eth), AMOUNT * 2);
	});
}

#[test]
fn rejects_assets_from_other_chains() {
	new_test_ext().execute_with(|| {
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_ACCOUNT);

		assert_noop!(route(Asset::Btc, AMOUNT), Error::<Test, Instance1>::AssetMismatch);
	});
}
