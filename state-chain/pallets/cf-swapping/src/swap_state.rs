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
	basis_points::SignedBasisPoints, Asset, AssetAmount, SwapId, SwapRequestId, STABLE_ASSET,
};
use frame_support::pallet_prelude::*;
use sp_std::fmt::Debug;

use super::{
	AssetAndAmount, BrokerFeesTracker, Config, FeeTaken, NetworkFeeTracker, Pallet,
	SwapFailureReason, Swap,
};

#[derive(DebugNoBound)]
pub struct SwapState<T: Config, Stage: Debug> {
	pub(crate) swap: Swap<T>,
	pub stage: Stage,
}

#[derive(DebugNoBound)]
pub struct Stage1 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
}

#[derive(DebugNoBound)]
pub struct Stage2 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
}

#[derive(DebugNoBound)]
pub struct Stage3 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
	pub output_amount_before_fees: AssetAmount,
}

#[derive(DebugNoBound)]
pub struct Stage4 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
	pub output_amount_before_fees: AssetAmount,
	pub output_amount_after_fees: AssetAmount,
	pub broker_fee_taken: AssetAmount,
}

#[derive(DebugNoBound)]
pub struct Stage5 {
	pub input_amount_after_fees: AssetAmount,
	/// Always in terms of input asset
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
	pub output_amount_before_fees: AssetAmount,
	pub output_amount_after_fees: AssetAmount,
	/// Always in terms of output asset
	pub broker_fee_taken: AssetAmount,
	pub oracle_delta: Option<SignedBasisPoints>,
	pub oracle_delta_ex_fees: Option<SignedBasisPoints>,
}

#[derive(DebugNoBound)]
pub struct StageFailed {
	pub swap_amount: AssetAmount,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub(crate) struct SwapGroupPair {
	pub from: Asset,
	pub to: Asset,
}

pub(crate) trait GroupSwapState<T: Config> {
	type OutputState;

	fn swap_amount(&self) -> AssetAmount;
	fn swap_group(&self) -> Option<SwapGroupPair>;
	fn update_swap_result(self, amount: AssetAmount) -> Self::OutputState;
	fn update_no_swap(self) -> Self::OutputState;
	/// Strip away the swap state details and just return the swap with some failure information
	fn failed_swap(self) -> SwapState<T, StageFailed>;
}

impl<T: Config> GroupSwapState<T> for SwapState<T, Stage1> {
	type OutputState = SwapState<T, Stage2>;

	fn swap_amount(&self) -> AssetAmount {
		self.stage.input_amount_after_fees
	}

	fn swap_group(&self) -> Option<SwapGroupPair> {
		if self.swap.from == STABLE_ASSET {
			None
		} else {
			Some(SwapGroupPair { from: self.swap.from, to: STABLE_ASSET })
		}
	}

	fn update_swap_result(self, output: AssetAmount) -> Self::OutputState {
		SwapState {
			stage: Stage2 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				intermediate: Some(AssetAndAmount::new(
					self.swap_group().map(|pair| pair.to).unwrap_or(self.swap.from),
					output,
				)),
			},
			swap: self.swap,
		}
	}

	fn update_no_swap(self) -> Self::OutputState {
		SwapState {
			stage: Stage2 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				intermediate: None,
			},
			swap: self.swap,
		}
	}

	fn failed_swap(self) -> SwapState<T, StageFailed> {
		SwapState { stage: StageFailed { swap_amount: self.swap_amount() }, swap: self.swap }
	}
}

impl<T: Config> GroupSwapState<T> for SwapState<T, Stage2> {
	type OutputState = SwapState<T, Stage3>;

	fn swap_amount(&self) -> AssetAmount {
		self.stage
			.intermediate
			.as_ref()
			.map(|AssetAndAmount { asset: _, amount }| *amount)
			.unwrap_or(self.stage.input_amount_after_fees)
	}

	fn swap_group(&self) -> Option<SwapGroupPair> {
		if self.swap.to == STABLE_ASSET {
			None
		} else {
			Some(SwapGroupPair { from: STABLE_ASSET, to: self.swap.to })
		}
	}

	fn update_swap_result(self, output: AssetAmount) -> Self::OutputState {
		SwapState {
			swap: self.swap,
			stage: Stage3 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				intermediate: self.stage.intermediate,
				output_amount_before_fees: output,
			},
		}
	}

	fn update_no_swap(self) -> Self::OutputState {
		SwapState {
			stage: Stage3 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				output_amount_before_fees: self.swap_amount(),
				intermediate: self.stage.intermediate,
			},
			swap: self.swap,
		}
	}

	fn failed_swap(self) -> SwapState<T, StageFailed> {
		SwapState { stage: StageFailed { swap_amount: self.swap_amount() }, swap: self.swap }
	}
}

impl<T: Config, Stage: Debug> SwapState<T, Stage> {
	pub fn swap_request_id(&self) -> SwapRequestId {
		self.swap.swap_request_id
	}

	pub(crate) fn swap_id(&self) -> SwapId {
		self.swap.swap_id
	}

	pub(crate) fn input_asset(&self) -> Asset {
		self.swap.from
	}

	pub(crate) fn output_asset(&self) -> Asset {
		self.swap.to
	}

	pub(crate) fn input_amount_before_fees(&self) -> AssetAmount {
		self.swap.input_amount
	}
}

impl<T: Config> SwapState<T, ()> {
	/// Bundle a swap with a swap state ready to start the swapping process.
	pub(crate) fn new(swap: Swap<T>) -> Self {
		Self { swap, stage: () }
	}

	pub fn take_network_fee(self, fee_tracker: &mut NetworkFeeTracker) -> SwapState<T, Stage1> {
		let FeeTaken { fee, remaining_amount } =
			fee_tracker.take_fee(self.input_amount_before_fees());
		SwapState {
			swap: self.swap,
			stage: Stage1 { input_amount_after_fees: remaining_amount, network_fee_taken: fee },
		}
	}

	pub fn no_network_fee(self) -> SwapState<T, Stage1> {
		SwapState {
			stage: Stage1 {
				input_amount_after_fees: self.input_amount_before_fees(),
				network_fee_taken: 0,
			},
			swap: self.swap,
		}
	}
}

impl<T: Config> SwapState<T, Stage3> {
	pub(crate) fn take_broker_fees(
		self,
		fee_tracker: &mut BrokerFeesTracker<T::AccountId>,
	) -> SwapState<T, Stage4> {
		let FeeTaken { fee, remaining_amount } =
			fee_tracker.take_all_fees(self.stage.output_amount_before_fees);
		SwapState {
			swap: self.swap,
			stage: Stage4 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				intermediate: self.stage.intermediate,
				output_amount_before_fees: self.stage.output_amount_before_fees,
				output_amount_after_fees: remaining_amount,
				broker_fee_taken: fee,
			},
		}
	}

	pub(crate) fn no_broker_fee(self) -> SwapState<T, Stage4> {
		SwapState {
			swap: self.swap,
			stage: Stage4 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				intermediate: self.stage.intermediate,
				output_amount_before_fees: self.stage.output_amount_before_fees,
				output_amount_after_fees: self.stage.output_amount_before_fees,
				broker_fee_taken: 0,
			},
		}
	}
}

impl<T: Config> SwapState<T, Stage4> {
	/// The final step of the swapping process. Checks for price violations and then sets the oracle
	/// delta values in the swap state.
	#[expect(clippy::result_large_err)]
	pub(crate) fn check_for_price_violation(
		self,
	) -> Result<SwapState<T, Stage5>, (Self, SwapFailureReason)> {
		match Pallet::<T>::check_swap_price_violation(&self) {
			Ok(oracle_delta_ex_fees) => {
				// Now calculate the overall oracle delta. Only used for display purposes.
				let oracle_delta = if oracle_delta_ex_fees.is_some() {
					Pallet::<T>::get_delta_from_oracle_price(
						self.input_amount_before_fees(),
						self.stage.output_amount_after_fees,
						self.input_asset(),
						self.output_asset(),
					)
					.ok()
					.flatten()
					.map(|delta| delta.pessimistic_rounded_into())
				} else {
					None
				};

				Ok(SwapState {
					swap: self.swap,
					stage: Stage5 {
						input_amount_after_fees: self.stage.input_amount_after_fees,
						network_fee_taken: self.stage.network_fee_taken,
						intermediate: self.stage.intermediate,
						output_amount_before_fees: self.stage.output_amount_before_fees,
						output_amount_after_fees: self.stage.output_amount_after_fees,
						broker_fee_taken: self.stage.broker_fee_taken,
						oracle_delta,
						oracle_delta_ex_fees,
					},
				})
			},
			Err(reason) => Err((self, reason)),
		}
	}
}
