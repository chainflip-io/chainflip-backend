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
use codec::Encode;
use frame_support::sp_runtime::{
	traits::{CheckedSub, Saturating},
	DispatchError, DispatchResult,
};
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};

use crate::RefundAddressRegistry;

type DefaultAccountId = u64;

pub struct MockBalance<AccountId = DefaultAccountId>(PhantomData<AccountId>);

impl<AccountId> LpDepositHandler for MockBalance<AccountId>
where
	AccountId: Encode,
{
	type AccountId = AccountId;

	fn add_deposit(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> frame_support::pallet_prelude::DispatchResult {
		Self::try_credit_account(who, asset, amount)
	}
}

impl<AccountId> MockPallet for MockBalance<AccountId> {
	const PREFIX: &'static [u8] = b"LP_BALANCE";
}

const FREE_BALANCES: &[u8] = b"FREE_BALANCES";

impl<AccountId> BalanceApi for MockBalance<AccountId>
where
	AccountId: Encode,
{
	type AccountId = AccountId;

	fn credit_account(who: &Self::AccountId, asset: Asset, amount_to_credit: AssetAmount) {
		Self::mutate_storage::<(&AccountId, cf_primitives::Asset), _, _, _, _>(
			FREE_BALANCES,
			&(who, asset),
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
		Self::mutate_storage::<(&AccountId, cf_primitives::Asset), _, _, _, _>(
			FREE_BALANCES,
			&(who, asset),
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

	fn free_balances_dont_sweep(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		Self::free_balances(who)
	}

	fn get_balance(who: &Self::AccountId, asset: Asset) -> AssetAmount {
		Self::get_storage(FREE_BALANCES, (who, asset)).unwrap_or_default()
	}
}

pub struct MockRefundAddressRegistry;

impl MockPallet for MockRefundAddressRegistry {
	const PREFIX: &'static [u8] = b"LP_REGISTRATION";
}

const REFUND_ADDRESS: &[u8] = b"REFUND_ADDRESS";

/// A deterministic, chain-correct placeholder address, so the chain-only `register_refund_address`
/// test helper can share one storage map with the full-address trait method.
fn placeholder_address(chain: ForeignChain) -> cf_chains::ForeignChainAddress {
	use cf_chains::{btc::ScriptPubkey, dot::PolkadotAccountId, ForeignChainAddress::*};
	match chain {
		ForeignChain::Ethereum => Eth(Default::default()),
		ForeignChain::Arbitrum => Arb(Default::default()),
		ForeignChain::Tron => Tron(Default::default()),
		ForeignChain::Solana => Sol(Default::default()),
		ForeignChain::Bitcoin => Btc(ScriptPubkey::P2PKH([0; 20])),
		ForeignChain::Polkadot => Dot(PolkadotAccountId([0; 32])),
		ForeignChain::Assethub => Hub(PolkadotAccountId([0; 32])),
	}
}

impl MockRefundAddressRegistry {
	/// Test helper: register a placeholder refund address for `chain` (enough to satisfy
	/// [`RefundAddressRegistry::ensure_has_refund_address_for_asset`]).
	pub fn register_refund_address(account_id: DefaultAccountId, chain: ForeignChain) {
		Self::put_storage(REFUND_ADDRESS, (account_id, chain), placeholder_address(chain));
	}
}

impl RefundAddressRegistry for MockRefundAddressRegistry {
	type AccountId = u64;

	fn register_liquidity_refund_address(
		who: &Self::AccountId,
		address: cf_chains::ForeignChainAddress,
	) {
		Self::put_storage(REFUND_ADDRESS, (*who, address.chain()), address);
	}

	fn get_refund_address(
		who: &Self::AccountId,
		chain: ForeignChain,
	) -> Option<cf_chains::ForeignChainAddress> {
		Self::get_storage(REFUND_ADDRESS, (*who, chain))
	}

	fn clear_refund_addresses(who: &Self::AccountId) {
		for chain in ForeignChain::iter() {
			Self::take_storage::<_, cf_chains::ForeignChainAddress>(REFUND_ADDRESS, (*who, chain));
		}
	}

	fn ensure_has_refund_address_for_asset(who: &Self::AccountId, asset: Asset) -> DispatchResult {
		if Self::get_refund_address(who, asset.into()).is_some() {
			Ok(())
		} else {
			Err(DispatchError::Other("no refund address"))
		}
	}
}
