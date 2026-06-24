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

use crate::{
	runtime_apis::types::{
		CcmData, DispatchErrorWithMessage, FeeTypes, SimulateSwapAdditionalOrder,
		SimulatedSwapInformation,
	},
	LiquidityPools, Runtime, Swapping,
};
use cf_chains::{
	assets::any::ForeignChainAndAsset,
	instances::{
		ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, PolkadotInstance,
		SolanaInstance, TronInstance,
	},
};
use cf_primitives::{
	AccountId, Asset, AssetAmount, BasisPoints, Beneficiary, DcaParameters, IngressOrEgress,
	OrderId, STABLE_ASSET,
};
use pallet_cf_ingress_egress::AmountAndFeesWithheld;
use pallet_cf_swapping::{BatchExecutionError, FeeType};
use scale_info::prelude::format;
use sp_runtime::{traits::UniqueSaturatedInto, DispatchError};
use sp_std::{collections::btree_set::BTreeSet, vec, vec::Vec};

/// Simulates a swap in order to estimate the output amount and fees.
pub fn simulate_swap(
	input_asset: Asset,
	output_asset: Asset,
	input_amount: AssetAmount,
	broker_commission: BasisPoints,
	dca_parameters: Option<DcaParameters>,
	ccm_data: Option<CcmData>,
	mut exclude_fees: BTreeSet<FeeTypes>,
	additional_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
	is_internal: Option<bool>,
) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage> {
	if let Some(additional_orders) = additional_orders {
		for (index, additional_order) in additional_orders.into_iter().enumerate() {
			match additional_order {
				SimulateSwapAdditionalOrder::LimitOrder {
					base_asset,
					quote_asset,
					side,
					tick,
					sell_amount,
				} => {
					LiquidityPools::try_add_limit_order(
						&AccountId::new([0; 32]),
						base_asset,
						quote_asset,
						side,
						index as OrderId,
						tick,
						sell_amount.into(),
					)?;
				},
			}
		}
	}

	let is_internal = is_internal.unwrap_or(false);
	if is_internal {
		exclude_fees.extend([
			FeeTypes::IngressDepositChannel,
			FeeTypes::Egress,
			FeeTypes::IngressVaultSwap,
		]);
	}

	let include_fee = |fee_type: FeeTypes| !exclude_fees.contains(&fee_type);

	// Default to using the DepositChannel fee unless specified.
	let AmountAndFeesWithheld { amount_after_fees: amount_to_swap, fees_withheld: ingress_fee } =
		if include_fee(FeeTypes::IngressDepositChannel) {
			take_ingress_or_egress_fee(
				IngressOrEgress::IngressDepositChannel,
				input_asset,
				input_amount,
			)
		} else if include_fee(FeeTypes::IngressVaultSwap) {
			take_ingress_or_egress_fee(IngressOrEgress::IngressVaultSwap, input_asset, input_amount)
		} else {
			AmountAndFeesWithheld { amount_after_fees: input_amount, fees_withheld: 0 }
		};

	// If no DCA parameter is given, swap the entire amount with 1 chunk.
	let number_of_chunks: u128 = dca_parameters
		.map(|dca| sp_std::cmp::max(dca.number_of_chunks, 1u32))
		.unwrap_or(1u32)
		.into();

	let amount_per_chunk: u128 = amount_to_swap / number_of_chunks;

	// We simulate each leg of the swap (input -> USDC, then USDC -> output) separately rather than
	// as a single swap. This lets us:
	//  - apply the network fee exactly where it is charged in a real swap (on the stable asset,
	//    between the two legs) instead of approximating it in input-asset terms,
	//  - account for the network fee minimum at the level of the whole swap (it does not apply
	//    per-chunk), and
	//  - attribute a failure to a specific leg.

	// First leg: input -> USDC. The network fee does not affect this leg, so the stable amount
	// here is the gross (pre-network-fee) value of the swap.
	let stable_amount = if input_asset == STABLE_ASSET {
		amount_per_chunk
	} else {
		Swapping::simulate_swap(input_asset, STABLE_ASSET, amount_per_chunk, vec![])
			.map_err(|e| -> DispatchErrorWithMessage {
				match e {
					BatchExecutionError::SwapLegFailed { .. } => DispatchError::Other(
						"Simulated swap failed on the input leg: swap leg failed.",
					)
					.into(),
					BatchExecutionError::PriceViolation { violating_swaps, .. } =>
						if let Some((_, reason)) = violating_swaps.first() {
							DispatchErrorWithMessage::RawMessage(
								format!("Simulated swap failed on the input leg due to a price violation: {reason:?}")
									.into_bytes(),
							)
						} else {
							// Should be unreachable
							DispatchError::Other(
								"Simulated swap failed on the input leg due to a price violation.",
							)
							.into()
						},
					BatchExecutionError::DispatchError { error } => error.into(),
				}
			})?
			.first()
			.and_then(|swap| swap.stable_amount)
			.unwrap_or_default()
	}
	.saturating_mul(number_of_chunks);

	// Network fee: charged on the gross stable amount, taking the larger of the rate-based fee and
	// the minimum (which applies to the swap as a whole), capped at the available stable amount.
	let network_fee = if include_fee(FeeTypes::Network) {
		let fee = pallet_cf_swapping::Pallet::<Runtime>::get_network_fee_for_swap(
			input_asset,
			output_asset,
			is_internal,
		);
		core::cmp::min(core::cmp::max(fee.rate * stable_amount, fee.minimum), stable_amount)
	} else {
		0
	};

	let stable_amount_after_network_fee_per_chunk =
		stable_amount.saturating_sub(network_fee) / number_of_chunks;

	// Second leg: USDC -> output. The broker fee is charged on the stable asset at the start of
	// this leg, so we let the swap simulation apply it for us.
	let second_leg = Swapping::simulate_swap(
		STABLE_ASSET,
		output_asset,
		stable_amount_after_network_fee_per_chunk,
		if broker_commission > 0 {
			vec![FeeType::BrokerFee(
				vec![Beneficiary { account: AccountId::new([0xbb; 32]), bps: broker_commission }]
					.try_into()
					.expect("1 is less than the capacity of Beneficiaries"),
			)]
		} else {
			vec![]
		},
	)
	.map_err(|e| -> DispatchErrorWithMessage {
		match e {
			BatchExecutionError::SwapLegFailed { .. } =>
				DispatchError::Other("Simulated swap failed on the output leg: swap leg failed.")
					.into(),
			BatchExecutionError::PriceViolation { violating_swaps, .. } =>
				if let Some((_, reason)) = violating_swaps.first() {
					DispatchErrorWithMessage::RawMessage(
						format!("Simulated swap failed on the output leg due to a price violation: {reason:?}")
							.into_bytes(),
					)
				} else {
					// Should be unreachable
					DispatchError::Other(
						"Simulated swap failed on the output leg due to a price violation.",
					)
					.into()
				},
			BatchExecutionError::DispatchError { error } => error.into(),
		}
	})?;
	let second_leg = second_leg.first();

	// Extrapolate the per-chunk results to the whole swap.
	let broker_fee = second_leg
		.and_then(|swap| swap.broker_fee_taken)
		.unwrap_or_default()
		.saturating_mul(number_of_chunks);
	let output = second_leg
		.and_then(|swap| swap.final_output)
		.unwrap_or_default()
		.saturating_mul(number_of_chunks);

	// The intermediary (stable) amount only exists for swaps that neither start nor end on the
	// stable asset. It reflects the amount actually swapped on the second leg, i.e. after all fees.
	let intermediary = if input_asset == STABLE_ASSET || output_asset == STABLE_ASSET {
		None
	} else {
		Some(
			second_leg
				.and_then(|swap| swap.stable_amount)
				.unwrap_or_default()
				.saturating_mul(number_of_chunks),
		)
	};

	let AmountAndFeesWithheld { amount_after_fees: output, fees_withheld: egress_fee } =
		if include_fee(FeeTypes::Egress) {
			let egress = match ccm_data {
				Some(CcmData { gas_budget, message_length }) => IngressOrEgress::EgressCcm {
					gas_budget,
					message_length: message_length as usize,
				},
				None => IngressOrEgress::Egress,
			};
			take_ingress_or_egress_fee(egress, output_asset, output)
		} else {
			AmountAndFeesWithheld { amount_after_fees: output, fees_withheld: 0 }
		};

	Ok(SimulatedSwapInformation {
		intermediary,
		output,
		network_fee,
		ingress_fee,
		egress_fee,
		broker_fee,
	})
}

fn take_ingress_or_egress_fee(
	ingress_or_egress: IngressOrEgress,
	asset: Asset,
	amount: AssetAmount,
) -> AmountAndFeesWithheld<AssetAmount> {
	match asset.into() {
		ForeignChainAndAsset::Ethereum(asset) => pallet_cf_ingress_egress::Pallet::<
			Runtime,
			EthereumInstance,
		>::withhold_ingress_or_egress_fee(
			ingress_or_egress,
			asset,
			amount.unique_saturated_into(),
		)
		.map_amounts(Into::into),
		ForeignChainAndAsset::Polkadot(asset) => pallet_cf_ingress_egress::Pallet::<
			Runtime,
			PolkadotInstance,
		>::withhold_ingress_or_egress_fee(
			ingress_or_egress,
			asset,
			amount.unique_saturated_into(),
		)
		.map_amounts(Into::into),
		ForeignChainAndAsset::Bitcoin(asset) => pallet_cf_ingress_egress::Pallet::<
			Runtime,
			BitcoinInstance,
		>::withhold_ingress_or_egress_fee(
			ingress_or_egress,
			asset,
			amount.unique_saturated_into(),
		)
		.map_amounts(Into::into),
		ForeignChainAndAsset::Arbitrum(asset) => pallet_cf_ingress_egress::Pallet::<
			Runtime,
			ArbitrumInstance,
		>::withhold_ingress_or_egress_fee(
			ingress_or_egress,
			asset,
			amount.unique_saturated_into(),
		)
		.map_amounts(Into::into),
		ForeignChainAndAsset::Solana(asset) => pallet_cf_ingress_egress::Pallet::<
			Runtime,
			SolanaInstance,
		>::withhold_ingress_or_egress_fee(
			ingress_or_egress,
			asset,
			amount.unique_saturated_into(),
		)
		.map_amounts(Into::into),
		ForeignChainAndAsset::Assethub(asset) => pallet_cf_ingress_egress::Pallet::<
			Runtime,
			AssethubInstance,
		>::withhold_ingress_or_egress_fee(
			ingress_or_egress,
			asset,
			amount.unique_saturated_into(),
		)
		.map_amounts(Into::into),
		ForeignChainAndAsset::Tron(asset) => pallet_cf_ingress_egress::Pallet::<
			Runtime,
			TronInstance,
		>::withhold_ingress_or_egress_fee(
			ingress_or_egress,
			asset,
			amount.unique_saturated_into(),
		)
		.map_amounts(Into::into),
	}
}
