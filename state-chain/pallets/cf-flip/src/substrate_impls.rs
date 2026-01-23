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

//! These implementations are unused in our runtime, but are required by some substrate
//! pallet configs, notably the session pallet. They are intedended to be minimal yet internally
//! consistent implementations.

use crate::{Config, Pallet};
use cf_traits::AccountInfo;
use frame_support::{
	pallet_prelude::One,
	traits::{
		fungible::{
			hold::{Inspect as InspectHold, Mutate as MutateHold, Unbalanced as UnbalancedHold},
			Inspect, Mutate, Unbalanced,
		},
		tokens,
	},
};
use sp_runtime::traits::{CheckedAdd, Saturating};

impl<T: Config> Inspect<T::AccountId> for Pallet<T> {
	type Balance = T::Balance;

	fn total_issuance() -> Self::Balance {
		Pallet::<T>::total_issuance()
	}

	fn balance(who: &T::AccountId) -> Self::Balance {
		<Pallet<T> as AccountInfo>::balance(who)
	}

	fn minimum_balance() -> Self::Balance {
		One::one()
	}

	fn total_balance(who: &T::AccountId) -> Self::Balance {
		<Self as Inspect<T::AccountId>>::balance(who)
	}

	fn reducible_balance(
		who: &T::AccountId,
		// Chainflip does not presently use preservation or fortitude concepts.
		_preservation: tokens::Preservation,
		_force: tokens::Fortitude,
	) -> Self::Balance {
		<Self as Inspect<T::AccountId>>::balance(who)
	}

	fn can_deposit(
		who: &T::AccountId,
		amount: Self::Balance,
		_provenance: tokens::Provenance,
	) -> tokens::DepositConsequence {
		if !frame_system::Account::<T>::contains_key(who) {
			tokens::DepositConsequence::CannotCreate
		} else if <Self as Inspect<T::AccountId>>::balance(who).checked_add(&amount).is_none() {
			tokens::DepositConsequence::Overflow
		} else {
			tokens::DepositConsequence::Success
		}
	}

	fn can_withdraw(
		who: &T::AccountId,
		amount: Self::Balance,
	) -> tokens::WithdrawConsequence<Self::Balance> {
		let available_balance = <Self as Inspect<T::AccountId>>::balance(who);
		if amount > available_balance {
			tokens::WithdrawConsequence::BalanceLow
		} else if available_balance.saturating_sub(amount) <
			<Self as Inspect<T::AccountId>>::minimum_balance()
		{
			tokens::WithdrawConsequence::WouldDie
		} else {
			tokens::WithdrawConsequence::Success
		}
	}
}

impl<T: Config> InspectHold<T::AccountId> for Pallet<T> {
	type Reason = T::RuntimeHoldReason;

	fn total_balance_on_hold(_who: &T::AccountId) -> Self::Balance {
		Default::default()
	}

	fn balance_on_hold(_reason: &Self::Reason, _who: &T::AccountId) -> Self::Balance {
		Default::default()
	}
}

// We explicitly do not support these operations. If they are called, it indicates a
// misconfiguration of the runtime.
impl<T: Config> Unbalanced<T::AccountId> for Pallet<T> {
	fn handle_dust(_dust: tokens::fungible::Dust<T::AccountId, Self>) {
		cf_runtime_utilities::log_or_panic!("cf-flip pallet does not support dust handling");
	}

	fn write_balance(
		_who: &T::AccountId,
		_amount: Self::Balance,
	) -> Result<Option<Self::Balance>, sp_runtime::DispatchError> {
		cf_runtime_utilities::log_or_panic!(
			"cf-flip pallet does not support direct balance mutation"
		);
		Err(sp_runtime::DispatchError::Other(
			"cf-flip pallet does not support direct balance mutation",
		))
	}

	fn set_total_issuance(amount: Self::Balance) {
		cf_runtime_utilities::log_or_panic!(
			"Attempted to set total issuance of cf-flip pallet to {amount:?}, which is unsupported"
		)
	}
}

impl<T: Config> UnbalancedHold<T::AccountId> for Pallet<T> {
	fn set_balance_on_hold(
		_reason: &Self::Reason,
		_who: &T::AccountId,
		_amount: Self::Balance,
	) -> sp_runtime::DispatchResult {
		// No-op implementation
		// We don't want to log_or_panic here because this is called via the session pallet
		// when a validator registers session keys.
		Ok(())
	}
}

// Use default implementations.
impl<T: Config> Mutate<T::AccountId> for Pallet<T> {}
impl<T: Config> MutateHold<T::AccountId> for Pallet<T> {}
