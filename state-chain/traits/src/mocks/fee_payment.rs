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

use frame_support::sp_runtime::{DispatchError, DispatchResult};

use crate::{Chainflip, FeePayment};

use super::{funding_info::MockFundingInfo, MockPallet, MockPalletStorage};

pub struct MockFeePayment<T>(sp_std::marker::PhantomData<T>);

impl<T> MockPallet for MockFeePayment<T> {
	const PREFIX: &'static [u8] = b"MockFeePayment";
}

const FLIP_2_1_ACTIVATED: &[u8] = b"FLIP_2_1_ACTIVATED";

impl<T> MockFeePayment<T> {
	pub fn set_flip_2_1_activated(activated: bool) {
		Self::put_value(FLIP_2_1_ACTIVATED, activated);
	}
}

pub const ERROR_INSUFFICIENT_LIQUIDITY: DispatchError =
	DispatchError::Other("Insufficient liquidity");

impl<T: Chainflip<FundingInfo = MockFundingInfo<T>>> FeePayment for MockFeePayment<T> {
	type AccountId = T::AccountId;
	type Amount = T::Amount;

	fn try_take_fee(account_id: &Self::AccountId, amount: Self::Amount) -> DispatchResult {
		MockFundingInfo::<T>::try_debit_funds(account_id, amount)
			.map(|_| ())
			.ok_or(ERROR_INSUFFICIENT_LIQUIDITY)
	}

	fn add_to_offchain_flip_to_be_distributed(_amount: i128) {}

	fn burn_or_reserve_offchain(_amount: Self::Amount) {}

	fn is_flip_2_1_activated() -> bool {
		Self::get_value(FLIP_2_1_ACTIVATED).unwrap_or(false)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn mint_to_account(account_id: &Self::AccountId, amount: Self::Amount) {
		MockFundingInfo::<T>::credit_funds(account_id, amount);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn activate_flip_2_1() {
		Self::set_flip_2_1_activated(true);
	}
}
