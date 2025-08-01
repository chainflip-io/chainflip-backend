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

use crate::{BalanceApi, LpDepositHandler};
use cf_chains::{
	assets::any::{Asset, AssetMap},
	ForeignChain,
};
use cf_primitives::AssetAmount;
use frame_support::sp_runtime::{
	traits::{CheckedSub, Saturating},
	DispatchError, DispatchResult,
};

use super::{MockPallet, MockPalletStorage};

use crate::LpRegistration;

pub struct MockBalance;

type AccountId = u64;

impl LpDepositHandler for MockBalance {
	type AccountId = AccountId;

	fn add_deposit(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> frame_support::pallet_prelude::DispatchResult {
		Self::try_credit_account(who, asset, amount)
	}
}

impl MockPallet for MockBalance {
	const PREFIX: &'static [u8] = b"LP_BALANCE";
}

const FREE_BALANCES: &[u8] = b"FREE_BALANCES";

impl BalanceApi for MockBalance {
	type AccountId = AccountId;

	fn credit_account(who: &Self::AccountId, asset: Asset, amount_to_credit: AssetAmount) {
		Self::mutate_storage::<(AccountId, cf_primitives::Asset), _, _, _, _>(
			FREE_BALANCES,
			&(*who, asset),
			|amount| {
				let amount = amount.get_or_insert_with(|| 0);
				*amount = amount.saturating_add(amount_to_credit);
			},
		);
	}

	fn try_credit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount_to_credit: AssetAmount,
	) -> DispatchResult {
		Self::credit_account(who, asset, amount_to_credit);
		Ok(())
	}

	fn try_debit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount_to_debit: AssetAmount,
	) -> DispatchResult {
		Self::mutate_storage::<(AccountId, cf_primitives::Asset), _, _, _, _>(
			FREE_BALANCES,
			&(*who, asset),
			|amount| {
				let amount = amount.get_or_insert_with(|| 0);

				*amount = amount
					.checked_sub(&amount_to_debit)
					.ok_or(DispatchError::Other("Insufficient balance"))?;
				Ok(())
			},
		)
	}

	fn free_balances(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		AssetMap::from_iter_or_default(Asset::all().map(|asset| {
			(asset, Self::get_storage(FREE_BALANCES, (who, asset)).unwrap_or_default())
		}))
	}

	fn get_balance(who: &Self::AccountId, asset: Asset) -> AssetAmount {
		Self::get_storage(FREE_BALANCES, (who, asset)).unwrap_or_default()
	}
}

pub struct MockLpRegistration;

impl MockPallet for MockLpRegistration {
	const PREFIX: &'static [u8] = b"LP_REGISTRATION";
}

const REFUND_ADDRESS_REGISTRATION: &[u8] = b"IS_REGISTERED_FOR_ASSET";

impl MockLpRegistration {
	pub fn register_refund_address(account_id: AccountId, chain: ForeignChain) {
		Self::mutate_storage::<(AccountId, ForeignChain), _, _, (), _>(
			REFUND_ADDRESS_REGISTRATION,
			&(account_id, chain),
			|is_registered: &mut Option<()>| {
				*is_registered = Some(());
			},
		);
	}
}

impl LpRegistration for MockLpRegistration {
	type AccountId = u64;

	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(
		who: &Self::AccountId,
		address: cf_chains::ForeignChainAddress,
	) {
		Self::register_refund_address(*who, address.chain());
	}

	fn ensure_has_refund_address_for_asset(who: &Self::AccountId, asset: Asset) -> DispatchResult {
		if Self::get_storage::<(AccountId, ForeignChain), ()>(
			REFUND_ADDRESS_REGISTRATION,
			(*who, asset.into()),
		)
		.is_some()
		{
			Ok(())
		} else {
			Err(DispatchError::Other("no refund address"))
		}
	}
}
