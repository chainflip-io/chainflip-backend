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

use crate::{Chainflip, Slashing};
use frame_support::sp_runtime::Saturating;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};
pub struct MockFlipSlasher<T>(PhantomData<T>);

impl<T> MockPallet for MockFlipSlasher<T> {
	const PREFIX: &'static [u8] = b"MockFlipSlasher";
}

const SLASHES: &[u8] = b"SLASHES";

impl<T: Chainflip> MockFlipSlasher<T> {
	pub fn slash_count(validator_id: &T::AccountId) -> u32 {
		<Self as MockPalletStorage>::get_storage(SLASHES, validator_id).unwrap_or_default()
	}
}

impl<T: Chainflip> Slashing for MockFlipSlasher<T> {
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type Balance = T::Amount;

	fn slash_balance(account_id: &Self::AccountId, _amount: Self::Balance) {
		<Self as MockPalletStorage>::mutate_storage(
			SLASHES,
			account_id,
			|count: &mut Option<u32>| {
				count.get_or_insert_default().saturating_accrue(1);
			},
		);
	}

	fn calculate_slash_amount(
		_account_id: &Self::AccountId,
		blocks_offline: Self::BlockNumber,
	) -> Self::Balance {
		use frame_support::sp_runtime::traits::UniqueSaturatedInto;
		const SLASH_PER_BLOCK: u32 = 100;
		let blocks_offline: u32 = blocks_offline.unique_saturated_into();
		T::Amount::from(SLASH_PER_BLOCK.saturating_mul(blocks_offline))
	}
}
