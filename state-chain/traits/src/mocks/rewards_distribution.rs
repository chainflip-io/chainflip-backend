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
use cf_primitives::EpochIndex;
use frame_support::weights::{Weight, WeightMeter};

pub struct MockRewardsDistribution<T>(core::marker::PhantomData<T>);

impl<T> MockPallet for MockRewardsDistribution<T> {
	const PREFIX: &'static [u8] = b"MockRewardsDistribution";
}

impl<T: Chainflip> RewardsDistribution for MockRewardsDistribution<T> {
	type Balance = T::Amount;
	type AccountId = T::AccountId;
	type Cursor = u32;

	fn distribute(
		epoch_index: EpochIndex,
		total_amount: Self::Balance,
		resume_from: Option<u32>,
		meter: &mut WeightMeter,
		weight: impl Fn(u32, u32) -> Weight,
		mut settle: impl FnMut(&Self::AccountId, Self::Balance),
	) -> Option<u32> {
		let beneficiaries: sp_std::vec::Vec<Self::AccountId> =
			<Self as MockPalletStorage>::get_storage(b"BENEFICIARIES", epoch_index)
				.unwrap_or_default();
		if beneficiaries.is_empty() {
			return None;
		}
		let count = beneficiaries.len() as u32;
		let start = resume_from.unwrap_or_default();
		if meter.try_consume(weight(0, count).saturating_sub(weight(0, 0))).is_err() {
			return Some(start);
		}
		let per_beneficiary_amount = total_amount / count.into();
		for (settled, beneficiary) in (0..).zip(beneficiaries.iter().skip(start as usize)) {
			if meter
				.try_consume(weight(settled + 1, count).saturating_sub(weight(settled, count)))
				.is_err()
			{
				return Some(start + settled);
			}
			<Self as MockPalletStorage>::mutate_storage(
				b"REWARDS",
				beneficiary,
				|balance: &mut Option<Self::Balance>| {
					let current_balance = balance.unwrap_or_default();
					*balance = Some(current_balance + per_beneficiary_amount);
				},
			);
			settle(beneficiary, per_beneficiary_amount);
		}
		None
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup_benchmark_payees(epoch_index: EpochIndex, payee_count: u32) {
		let payees: sp_std::vec::Vec<T::AccountId> =
			(0..payee_count).map(|i| frame_benchmarking::account("payee", i, 0)).collect();
		for payee in &payees {
			frame_system::Pallet::<T>::inc_providers(payee);
		}
		Self::set_beneficiaries(epoch_index, payees);
	}
}

impl<T: Chainflip> MockRewardsDistribution<T> {
	pub fn get_assigned_rewards(
		beneficiary: &<Self as RewardsDistribution>::AccountId,
	) -> <Self as RewardsDistribution>::Balance {
		<Self as MockPalletStorage>::get_storage(b"REWARDS", beneficiary).unwrap_or_default()
	}

	pub fn set_beneficiaries(
		epoch_index: EpochIndex,
		beneficiaries: sp_std::vec::Vec<<Self as RewardsDistribution>::AccountId>,
	) {
		<Self as MockPalletStorage>::put_storage(b"BENEFICIARIES", epoch_index, beneficiaries);
	}
}
