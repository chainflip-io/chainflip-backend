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

pub const ERROR_INSUFFICIENT_LIQUIDITY: DispatchError =
	DispatchError::Other("Insufficient liquidity");

const OFFCHAIN_FLIP_TO_BE_DISTRIBUTED: &[u8] = b"OFFCHAIN_FLIP_TO_BE_DISTRIBUTED";

impl<T> MockPallet for MockFeePayment<T> {
	const PREFIX: &'static [u8] = b"MockFeePayment";
}

impl<T: Chainflip<FundingInfo = MockFundingInfo<T>>> FeePayment for MockFeePayment<T> {
	type AccountId = T::AccountId;
	type Amount = T::Amount;

	fn try_take_fee(account_id: &Self::AccountId, amount: Self::Amount) -> DispatchResult {
		MockFundingInfo::<T>::try_debit_funds(account_id, amount)
			.map(|_| ())
			.ok_or(ERROR_INSUFFICIENT_LIQUIDITY)
	}

	fn add_to_offchain_flip_to_be_distributed(amount: i128) {
		Self::mutate_value(OFFCHAIN_FLIP_TO_BE_DISTRIBUTED, |value: &mut Option<i128>| {
			*value = Some(value.unwrap_or_default() + amount);
		});
	}

	fn burn_or_reserve_offchain(_amount: Self::Amount) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn mint_to_account(account_id: &Self::AccountId, amount: Self::Amount) {
		MockFundingInfo::<T>::credit_funds(account_id, amount);
	}
}

impl<T> MockFeePayment<T> {
	pub fn get_offchain_flip_to_be_distributed() -> i128 {
		Self::get_value(OFFCHAIN_FLIP_TO_BE_DISTRIBUTED).unwrap_or_default()
	}
}
