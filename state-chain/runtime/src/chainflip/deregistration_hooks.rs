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

use crate::{AccountId, AccountRoles, Runtime};
use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, DeregistrationHooks, RefundAddressRegistry};
use frame_support::sp_runtime::DispatchError;
use pallet_cf_flip::Bonder;

pub struct RuntimeDeregistrationHooks;

impl DeregistrationHooks for RuntimeDeregistrationHooks {
	type AccountId = AccountId;
	type Error = DispatchError;

	fn check(account_id: &Self::AccountId) -> Result<(), DispatchError> {
		match AccountRoles::account_role(account_id) {
			AccountRole::Unregistered => Ok(()),
			AccountRole::LiquidityProvider => {
				pallet_cf_pools::OpenOrdersDeregistrationCheck::<Runtime>::check(account_id)?;
				pallet_cf_asset_balances::FreeBalancesDeregistrationCheck::<Runtime>::check(
					account_id,
				)?;
				pallet_cf_asset_balances::WithdrawalWhitelistDeregistrationCheck::<Runtime>::check(
					account_id,
				)?;
				pallet_cf_lending_pools::PoolsDeregistrationCheck::<Runtime>::check(account_id)?;
				pallet_cf_trading_strategy::TradingStrategyDeregistrationCheck::<Runtime>::check(
					account_id,
				)?;
				pallet_cf_swapping::PendingSwapDeregistrationCheck::<Runtime>::check(account_id)?;
				pallet_cf_validator::DelegatorDeregistrationCheck::<Runtime>::check(account_id)?;
				Ok(())
			},
			AccountRole::Broker => {
				pallet_cf_asset_balances::FreeBalancesDeregistrationCheck::<Runtime>::check(
					account_id,
				)?;
				pallet_cf_asset_balances::WithdrawalWhitelistDeregistrationCheck::<Runtime>::check(
					account_id,
				)?;
				pallet_cf_swapping::BrokerDeregistrationCheck::<Runtime>::check(account_id)?;
				Bonder::<Runtime>::check(account_id)?;
				Ok(())
			},
			AccountRole::Validator | AccountRole::Operator => {
				pallet_cf_validator::ValidatorDeregistrationCheck::<Runtime>::check(account_id)?;
				Bonder::<Runtime>::check(account_id)?;
				Ok(())
			},
		}
	}

	fn on_deregistered(account_id: &Self::AccountId, account_role: AccountRole) {
		if account_role == AccountRole::LiquidityProvider {
			pallet_cf_asset_balances::Pallet::<Runtime>::clear_refund_addresses(account_id);
		}
	}
}
