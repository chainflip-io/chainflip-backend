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

use crate::{AccountId, Runtime};
use cf_primitives::AccountRole;
use cf_traits::{DeregistrationHooks, RefundAddressRegistry, WithdrawalAddressRestriction};
use frame_support::sp_runtime::DispatchError;
use pallet_cf_flip::Bonder;

/// Checks run regardless of the role being deregistered: state is keyed by account, and an
/// account can hold state from a previous role (e.g. a delegation bond after `undelegate`), so
/// gating checks by the current role lets that state leak into the next one.
pub struct RuntimeDeregistrationHooks;

impl DeregistrationHooks for RuntimeDeregistrationHooks {
	type AccountId = AccountId;
	type Error = DispatchError;

	fn check(account_id: &Self::AccountId) -> Result<(), DispatchError> {
		Bonder::<Runtime>::check(account_id)?;
		pallet_cf_validator::ValidatorDeregistrationCheck::<Runtime>::check(account_id)?;
		pallet_cf_validator::DelegatorDeregistrationCheck::<Runtime>::check(account_id)?;
		pallet_cf_swapping::BrokerDeregistrationCheck::<Runtime>::check(account_id)?;
		pallet_cf_asset_balances::WithdrawalWhitelistDeregistrationCheck::<Runtime>::check(
			account_id,
		)?;
		pallet_cf_asset_balances::FreeBalancesDeregistrationCheck::<Runtime>::check(account_id)?;
		pallet_cf_trading_strategy::TradingStrategyDeregistrationCheck::<Runtime>::check(
			account_id,
		)?;
		pallet_cf_pools::OpenOrdersDeregistrationCheck::<Runtime>::check(account_id)?;
		pallet_cf_lending_pools::PoolsDeregistrationCheck::<Runtime>::check(account_id)?;
		// Last: iterates over all pending swap requests.
		pallet_cf_swapping::PendingSwapDeregistrationCheck::<Runtime>::check(account_id)?;
		Ok(())
	}

	fn on_deregistered(account_id: &Self::AccountId, _account_role: AccountRole) {
		pallet_cf_asset_balances::Pallet::<Runtime>::clear_refund_addresses(account_id);
		<pallet_cf_asset_balances::Pallet<Runtime> as WithdrawalAddressRestriction>::clear_pending_withdrawal_changes(
			account_id,
		);
	}
}
