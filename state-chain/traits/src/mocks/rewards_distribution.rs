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

use super::{MockPallet, MockPalletStorage};
use crate::{Chainflip, RewardsDistribution};

pub struct MockRewardsDistribution<T>(core::marker::PhantomData<T>);

impl<T> MockPallet for MockRewardsDistribution<T> {
	const PREFIX: &'static [u8] = b"MockRewardsDistribution";
}

impl<T: Chainflip> RewardsDistribution for MockRewardsDistribution<T> {
	type Balance = T::Amount;
	type AccountId = T::AccountId;

	fn distribute(amount: Self::Balance, beneficiary: &Self::AccountId) {
		<Self as MockPalletStorage>::mutate_storage(
			b"REWARDS",
			beneficiary,
			|balance: &mut Option<Self::Balance>| {
				let current_balance = balance.unwrap_or_default();
				*balance = Some(current_balance + amount);
			},
		);
	}
}

impl<T: Chainflip> MockRewardsDistribution<T> {
	pub fn get_assigned_rewards(
		beneficiary: &<Self as RewardsDistribution>::AccountId,
	) -> <Self as RewardsDistribution>::Balance {
		<Self as MockPalletStorage>::get_storage(b"REWARDS", beneficiary).unwrap_or_default()
	}
}
