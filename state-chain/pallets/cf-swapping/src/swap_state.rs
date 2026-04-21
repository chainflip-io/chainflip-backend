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
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::fmt::Debug;

use super::{
	AssetAndAmount, BrokerFeesTracker, Config, FeeTaken, NetworkFeeTracker, Pallet, Swap,
	SwapFailureReason, SwapRefundParameters,
};

#[derive(Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SuccessfulSwap {
	pub swap_id: SwapId,
	pub swap_request_id: SwapRequestId,
	pub input_asset: Asset,
	pub output_asset: Asset,
	pub input_amount_after_fees: AssetAmount,
	/// Always in terms of input asset
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
	pub output_amount_after_fees: AssetAmount,
	/// Always in terms of output asset
	pub broker_fee_taken: AssetAmount,
	pub oracle_delta_ex_fees: Option<SignedBasisPoints>,
	pub oracle_delta: Option<SignedBasisPoints>,
}

impl SuccessfulSwap {
	/// Downgrades a completed swap state to a Stage2. Used in the 'get_scheduled_swaps' RPC's
	pub(crate) fn into_stage_2<T: Config>(self, swap: Swap<T>) -> SwapState<T, AfterFirstLeg2> {
		SwapState {
			swap,
			stage: AfterFirstLeg2 {
				input_amount_after_fees: self.input_amount_after_fees,
				network_fee_taken: self.network_fee_taken,
				intermediate: self.intermediate,
			},
		}
	}
}

#[derive(DebugNoBound)]
/// A swap is bundled with a swap stage to track the progress through the swapping process.
pub struct SwapState<T: Config, Stage: Debug> {
	/// This struct is the template for starting a swap chunk. It should never be mutated.
	swap: Swap<T>,
	/// The swap stage is used to track the progress of a swap chunk through the swapping process.
	/// Each stage represents a step in the swapping process and adds additional information about
	/// the swap chunk as it progresses.
	pub stage: Stage,
}

#[derive(Debug)]
/// The stage after taking network fees from the input amount
pub struct AfterNetworkFee1 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
}

#[derive(Debug)]
/// The stage after the first leg of the swap is done (to the intermediate asset)
pub struct AfterFirstLeg2 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
}

#[derive(Debug)]
/// The stage after the second leg of the swap is done (to the output asset)
pub struct AfterSecondLeg3 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
	pub output_amount_before_fees: AssetAmount,
}

#[derive(Debug)]
/// The stage after broker fees have been taken from the output amount
pub struct AfterBrokerFee4 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
	pub output_amount_before_fees: AssetAmount,
	pub output_amount_after_fees: AssetAmount,
	pub broker_fee_taken: AssetAmount,
}

#[derive(Debug)]
/// The stage after price protection checks have been passed (minimum price and LPP).
pub struct AfterPriceProtection5 {
	pub input_amount_after_fees: AssetAmount,
	pub network_fee_taken: AssetAmount,
	pub intermediate: Option<AssetAndAmount>,
	pub output_amount_before_fees: AssetAmount,
	pub output_amount_after_fees: AssetAmount,
	pub broker_fee_taken: AssetAmount,
	pub oracle_delta_ex_fees: Option<SignedBasisPoints>,
}

#[derive(Debug)]
/// A stage representing a failed swap, used in the execution loop to return information about the
/// failed swaps.
pub struct StageFailed {
	pub swap_amount: AssetAmount,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub(crate) struct SwapGroupPair {
	pub from: Asset,
	pub to: Asset,
}

/// The 2 stages that are handed to the group swap logic (to & from intermediate) implement this
/// trait to allow them to be grouped and executed.
pub(crate) trait GroupSwapState<T: Config> {
	type OutputState;

	/// The input amount that is being swapped for the next leg.
	fn swap_amount(&self) -> AssetAmount;
	/// Returns the "to" and "from" assets as a swap pair to allow grouping of swaps. Returns None
	/// if the asset does not need to be swapped for this leg (e.g. if the input or output asset is
	/// already the intermediate asset).
	fn swap_group(&self) -> Option<SwapGroupPair>;
	/// Updates the swap state with the output amount of a swap for this single leg.
	fn advance_with_swap_result(self, amount: AssetAmount) -> Self::OutputState;
	/// Used when the asset does not need to be swapped for this leg, just passes through the input
	/// amount as the output amount.
	fn advance_no_swap(self) -> Self::OutputState;
	/// Strip away the swap state details and just return the swap with some failure information
	fn failed_swap(self) -> SwapState<T, StageFailed>;
}

impl<T: Config> GroupSwapState<T> for SwapState<T, AfterNetworkFee1> {
	type OutputState = SwapState<T, AfterFirstLeg2>;

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

	fn advance_with_swap_result(self, output: AssetAmount) -> Self::OutputState {
		SwapState {
			stage: AfterFirstLeg2 {
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

	fn advance_no_swap(self) -> Self::OutputState {
		SwapState {
			stage: AfterFirstLeg2 {
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

impl<T: Config> GroupSwapState<T> for SwapState<T, AfterFirstLeg2> {
	type OutputState = SwapState<T, AfterSecondLeg3>;

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

	fn advance_with_swap_result(self, output: AssetAmount) -> Self::OutputState {
		SwapState {
			swap: self.swap,
			stage: AfterSecondLeg3 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				intermediate: self.stage.intermediate,
				output_amount_before_fees: output,
			},
		}
	}

	fn advance_no_swap(self) -> Self::OutputState {
		SwapState {
			stage: AfterSecondLeg3 {
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
	pub(crate) fn swap_request_id(&self) -> SwapRequestId {
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

	pub(crate) fn execute_at(&self) -> BlockNumberFor<T> {
		self.swap.execute_at
	}

	pub(crate) fn refund_params(&self) -> Option<&SwapRefundParameters> {
		self.swap.refund_params.as_ref()
	}

	/// Consume the swap state and return the inner swap.
	pub(crate) fn into_swap(self) -> Swap<T> {
		self.swap
	}

	#[cfg(test)]
	pub fn new_test_state(swap: Swap<T>, stage: Stage) -> Self {
		Self { swap, stage }
	}
}

impl<T: Config> SwapState<T, AfterNetworkFee1> {
	/// Transition to Stage2 with a given intermediate amount. Used when the intermediate is
	/// computed via a pool price estimate rather than an actual swap.
	pub(crate) fn with_intermediate(
		self,
		intermediate: Option<AssetAndAmount>,
	) -> SwapState<T, AfterFirstLeg2> {
		SwapState {
			swap: self.swap,
			stage: AfterFirstLeg2 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				intermediate,
			},
		}
	}
}

impl<T: Config> SwapState<T, ()> {
	/// Bundle a swap with a swap state ready to start the swapping process.
	pub(crate) fn new(swap: Swap<T>) -> Self {
		Self { swap, stage: () }
	}

	pub(crate) fn take_network_fee(
		self,
		fee_tracker: &mut NetworkFeeTracker,
	) -> SwapState<T, AfterNetworkFee1> {
		let FeeTaken { fee, remaining_amount } =
			fee_tracker.take_fee(self.input_amount_before_fees());
		SwapState {
			swap: self.swap,
			stage: AfterNetworkFee1 {
				input_amount_after_fees: remaining_amount,
				network_fee_taken: fee,
			},
		}
	}

	pub(crate) fn no_network_fee(self) -> SwapState<T, AfterNetworkFee1> {
		SwapState {
			stage: AfterNetworkFee1 {
				input_amount_after_fees: self.input_amount_before_fees(),
				network_fee_taken: 0,
			},
			swap: self.swap,
		}
	}
}

impl<T: Config> SwapState<T, AfterSecondLeg3> {
	pub(crate) fn take_broker_fees(
		self,
		fee_tracker: &mut BrokerFeesTracker<T::AccountId>,
	) -> SwapState<T, AfterBrokerFee4> {
		let FeeTaken { fee, remaining_amount } =
			fee_tracker.take_all_fees(self.stage.output_amount_before_fees);
		SwapState {
			swap: self.swap,
			stage: AfterBrokerFee4 {
				input_amount_after_fees: self.stage.input_amount_after_fees,
				network_fee_taken: self.stage.network_fee_taken,
				intermediate: self.stage.intermediate,
				output_amount_before_fees: self.stage.output_amount_before_fees,
				output_amount_after_fees: remaining_amount,
				broker_fee_taken: fee,
			},
		}
	}

	pub(crate) fn no_broker_fee(self) -> SwapState<T, AfterBrokerFee4> {
		SwapState {
			swap: self.swap,
			stage: AfterBrokerFee4 {
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

impl<T: Config> SwapState<T, AfterBrokerFee4> {
	/// The final step of the swapping process. Checks for price violations and then sets the oracle
	/// delta values in the swap state.
	pub(crate) fn check_for_price_violation(
		self,
	) -> Result<SwapState<T, AfterPriceProtection5>, (Swap<T>, SwapFailureReason)> {
		match Pallet::<T>::check_swap_price_violation(&self) {
			Ok(oracle_delta_ex_fees) => Ok(SwapState {
				swap: self.swap,
				stage: AfterPriceProtection5 {
					input_amount_after_fees: self.stage.input_amount_after_fees,
					network_fee_taken: self.stage.network_fee_taken,
					intermediate: self.stage.intermediate,
					output_amount_before_fees: self.stage.output_amount_before_fees,
					output_amount_after_fees: self.stage.output_amount_after_fees,
					broker_fee_taken: self.stage.broker_fee_taken,
					oracle_delta_ex_fees,
				},
			}),
			Err(reason) => Err((self.into_swap(), reason)),
		}
	}
}

impl<T: Config> SwapState<T, AfterPriceProtection5> {
	/// Calculate the overall oracle delta. Only used for display purposes.
	pub(crate) fn calculate_oracle_delta(self) -> SuccessfulSwap {
		// If the oracle_delta_ex_fees is None, then we can skip the oracle delta calculation since
		// it will also fail.
		let oracle_delta = if self.stage.oracle_delta_ex_fees.is_some() {
			Pallet::<T>::get_delta_from_oracle_price(
				AssetAndAmount::new(self.input_asset(), self.input_amount_before_fees()),
				AssetAndAmount::new(self.output_asset(), self.stage.output_amount_after_fees),
			)
			.ok()
			.flatten()
			.map(|delta| delta.pessimistic_rounded_into())
		} else {
			None
		};

		SuccessfulSwap {
			swap_id: self.swap_id(),
			swap_request_id: self.swap_request_id(),
			input_asset: self.input_asset(),
			output_asset: self.output_asset(),
			input_amount_after_fees: self.stage.input_amount_after_fees,
			network_fee_taken: self.stage.network_fee_taken,
			intermediate: self.stage.intermediate,
			output_amount_after_fees: self.stage.output_amount_after_fees,
			broker_fee_taken: self.stage.broker_fee_taken,
			oracle_delta_ex_fees: self.stage.oracle_delta_ex_fees,
			oracle_delta,
		}
	}
}
