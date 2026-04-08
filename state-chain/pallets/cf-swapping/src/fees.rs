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

use cf_primitives::{
	AssetAmount, Beneficiaries, Beneficiary, BASIS_POINTS_PER_MILLION, ONE_AS_BASIS_POINTS,
};
use frame_support::{pallet_prelude::*, DebugNoBound};
use serde::{Deserialize, Serialize};
use sp_arithmetic::traits::Zero;
use sp_runtime::{Permill, Saturating};
use sp_std::collections::btree_map::BTreeMap;

use super::FeeTaken;

#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Default,
	Serialize,
	Deserialize,
)]
pub struct FeeRateAndMinimum {
	pub rate: Permill,
	pub minimum: AssetAmount,
}

#[derive(Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct NetworkFeeTracker {
	/// Fee rate and minimum in input asset terms.
	pub(crate) network_fee: FeeRateAndMinimum,
	/// Total amount of the input asset that has had fees taken already
	pub(crate) processed_asset_amount: AssetAmount,
	/// Total amount of fees that has been taken already in input asset terms
	pub(crate) accumulated_fee: AssetAmount,
}

impl NetworkFeeTracker {
	pub const fn new(network_fee: FeeRateAndMinimum) -> Self {
		Self { network_fee, processed_asset_amount: 0, accumulated_fee: 0 }
	}

	pub fn new_without_minimum(network_fee_rate: Permill) -> Self {
		Self {
			network_fee: FeeRateAndMinimum { rate: network_fee_rate, minimum: 0 },
			processed_asset_amount: 0,
			accumulated_fee: 0,
		}
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn network_fee(&self) -> &FeeRateAndMinimum {
		&self.network_fee
	}

	pub fn take_fee(&mut self, input_amount: AssetAmount) -> FeeTaken {
		if input_amount.is_zero() {
			return FeeTaken { remaining_amount: 0, fee: 0 };
		}
		self.processed_asset_amount.saturating_accrue(input_amount);
		let calculated_fee = core::cmp::max(
			self.network_fee.rate * self.processed_asset_amount,
			self.network_fee.minimum,
		);
		let fee_taken =
			core::cmp::min(calculated_fee.saturating_sub(self.accumulated_fee), input_amount);

		self.accumulated_fee.saturating_accrue(fee_taken);

		FeeTaken { remaining_amount: input_amount.saturating_sub(fee_taken), fee: fee_taken }
	}
}

#[derive(DebugNoBound, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct BrokerFeesTracker<AccountId: core::fmt::Debug + Ord> {
	pub fee_and_accumulated: BTreeMap<Beneficiary<AccountId>, AssetAmount>,
}

impl<AccountId: core::fmt::Debug + Ord> BrokerFeesTracker<AccountId> {
	pub fn new(beneficiaries: Beneficiaries<AccountId>) -> Self {
		Self { fee_and_accumulated: beneficiaries.into_iter().map(|b| (b, 0)).collect() }
	}

	pub fn take_all_fees(&mut self, input_amount: AssetAmount) -> FeeTaken {
		if input_amount.is_zero() {
			return FeeTaken { remaining_amount: 0, fee: 0 };
		}
		if self.fee_and_accumulated.is_empty() {
			return FeeTaken { remaining_amount: input_amount, fee: 0 };
		}
		// Sanity check: it should already not be possible to open a channel with broker fees
		// this high, but if the total broker fee would exceed 100% we charge no broker fee
		// instead (for simplicity):
		let total_fee_bps = self
			.fee_and_accumulated
			.keys()
			.fold(0u16, |total_bps, Beneficiary { bps, .. }| total_bps.saturating_add(*bps));

		let mut total_fee = 0;
		if total_fee_bps > ONE_AS_BASIS_POINTS {
			return FeeTaken { remaining_amount: input_amount, fee: 0 }
		} else {
			self.fee_and_accumulated.iter_mut().for_each(
				|(Beneficiary { bps, .. }, accumulated_fee)| {
					let fee =
						Permill::from_parts(*bps as u32 * BASIS_POINTS_PER_MILLION) * input_amount;
					accumulated_fee.saturating_accrue(fee);
					total_fee.saturating_accrue(fee)
				},
			);
		}

		debug_assert!(total_fee <= input_amount, "Broker fee cannot be more than the amount");
		FeeTaken { remaining_amount: input_amount.saturating_sub(total_fee), fee: total_fee }
	}
}
