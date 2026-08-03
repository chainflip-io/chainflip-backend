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

use crate::{WithdrawalAddressAlreadyBound, WithdrawalAddressRestriction};
use cf_chains::{
	evm::Address as EthereumAddress, AccountOrAddress, ForeignChain, ForeignChainAddress,
};
use codec::{Decode, Encode};
use frame_support::sp_runtime::DispatchResult;
use sp_std::{marker::PhantomData, vec::Vec};

use super::{MockPallet, MockPalletStorage};

/// Permissive by default (no configured restriction ⇒ everything allowed). Call
/// [`MockWithdrawalAddressRestriction::restrict_to`] to limit an account to a specific set of
/// destinations, mirroring the real "restriction on" behaviour for integration tests.
///
/// A bound broker address is enforced the same way the real pallet does it: it overrides the
/// whitelist for Ethereum destinations and leaves other chains untouched.
pub struct MockWithdrawalAddressRestriction<AccountId>(PhantomData<AccountId>);

impl<AccountId> MockPallet for MockWithdrawalAddressRestriction<AccountId> {
	const PREFIX: &'static [u8] = b"cf-mocks//WithdrawalAddressRestriction";
}

type Dest<AccountId> = AccountOrAddress<AccountId, ForeignChainAddress>;

const ALLOWED: &[u8] = b"ALLOWED";
const BOUND: &[u8] = b"BOUND";

impl<AccountId: Encode + Decode + Ord + Clone> MockWithdrawalAddressRestriction<AccountId> {
	/// Restrict `owner` to exactly the given destinations. Absence of a config = unrestricted.
	pub fn restrict_to(owner: &AccountId, allowed: Vec<Dest<AccountId>>) {
		Self::put_storage(ALLOWED, owner, allowed);
	}

	/// Remove any restriction for `owner` (back to allow-all).
	pub fn unrestrict(owner: &AccountId) {
		Self::take_storage::<_, Vec<Dest<AccountId>>>(ALLOWED, owner);
	}

	fn allowed(owner: &AccountId) -> Option<Vec<Dest<AccountId>>> {
		Self::get_storage(ALLOWED, owner)
	}

	/// The bound broker address, for assertions and for the override check below.
	pub fn bound_address(owner: &AccountId) -> Option<EthereumAddress> {
		Self::get_storage(BOUND, owner)
	}
}

impl<AccountId: Encode + Decode + Ord + Clone> WithdrawalAddressRestriction
	for MockWithdrawalAddressRestriction<AccountId>
{
	type AccountId = AccountId;

	fn ensure_withdrawal_allowed_to(
		owner: &Self::AccountId,
		dest: AccountOrAddress<&AccountId, &ForeignChainAddress>,
	) -> DispatchResult {
		let external = match &dest {
			AccountOrAddress::ExternalAddress(address) => Some(*address),
			AccountOrAddress::InternalAccount(_) => None,
		};

		// A binding overrides the whitelist, but only for Ethereum destinations.
		if Self::bound_address(owner).is_some_and(|bound| {
			external.is_some_and(|address| {
				address.chain() == ForeignChain::Ethereum &&
					*address != ForeignChainAddress::Eth(bound)
			})
		}) {
			return Err("MockWithdrawalAddressRestriction: destination not allowed".into());
		}

		let Some(allowed) = Self::allowed(owner) else { return Ok(()) };
		let is_allowed = allowed.iter().any(|entry| match (entry, &dest) {
			(AccountOrAddress::ExternalAddress(a), AccountOrAddress::ExternalAddress(b)) => a == *b,
			(AccountOrAddress::InternalAccount(a), AccountOrAddress::InternalAccount(b)) => a == *b,
			_ => false,
		});
		if is_allowed {
			Ok(())
		} else {
			Err("MockWithdrawalAddressRestriction: destination not allowed".into())
		}
	}

	fn bind_broker_withdrawal_address(
		owner: &Self::AccountId,
		address: EthereumAddress,
	) -> Result<(), WithdrawalAddressAlreadyBound> {
		if Self::bound_address(owner).is_some() {
			return Err(WithdrawalAddressAlreadyBound);
		}
		Self::put_storage(BOUND, owner, address);
		Ok(())
	}
}
