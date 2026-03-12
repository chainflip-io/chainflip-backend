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
		SolanaInstance,
	},
};
use cf_primitives::{
	AccountId, Asset, AssetAmount, AssetAndAmount, BasisPoints, Beneficiary, DcaParameters,
	IngressOrEgress,
};
use cf_traits::OrderId;
use pallet_cf_ingress_egress::AmountAndFeesWithheld;
use pallet_cf_swapping::{BatchExecutionError, FeeRateAndMinimum, FeeType};
use sp_runtime::{
	traits::{Saturating, UniqueSaturatedInto},
	DispatchError,
};
use sp_std::{cmp::max, collections::btree_set::BTreeSet, vec, vec::Vec};

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
	let AmountAndFeesWithheld { amount_after_fees: mut amount_to_swap, fees_withheld: ingress_fee } =
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
	let number_of_chunks: u128 =
		dca_parameters.map(|dca| max(dca.number_of_chunks, 1u32)).unwrap_or(1u32).into();

	// We manually calculate the network fee to avoid complications with the network fee
	// minimum affecting the simulated chunk.
	let network_fee = if include_fee(FeeTypes::Network) {
		let FeeRateAndMinimum { rate, minimum } =
			pallet_cf_swapping::Pallet::<Runtime>::get_network_fee_for_swap(
				input_asset,
				output_asset,
				is_internal,
			);
		max(rate * amount_to_swap, minimum)
	} else {
		0
	};

	amount_to_swap.saturating_reduce(network_fee);

	let amount_per_chunk: u128 = amount_to_swap / number_of_chunks;

	let swap_output = &Swapping::simulate_swap(
		input_asset,
		output_asset,
		amount_per_chunk,
		if broker_commission > 0 {
			// Leaving out the network fee because it is already taken from the input amount
			vec![FeeType::BrokerFee(
				vec![Beneficiary { account: AccountId::new([0xbb; 32]), bps: broker_commission }]
					.try_into()
					.expect("1 is less than the capacity of Beneficiaries"),
			)]
		} else {
			vec![]
		},
	)
	.map_err(|e| match e {
		BatchExecutionError::SwapLegFailed { .. } => DispatchError::Other("Swap leg failed."),
		BatchExecutionError::PriceViolation { .. } => DispatchError::Other(
			"Price Violation: Some swaps failed due to Price Impact Limitations.",
		),
		BatchExecutionError::DispatchError { error } => error,
	})?;
	let swap = &swap_output[0];

	// Extrapolate the total by multiplying the chunk by the number of chunks
	let intermediary = swap
		.intermediate()
		.map(|AssetAndAmount { asset: _, amount }| amount * number_of_chunks);
	let output = swap.final_output.unwrap_or_default() * number_of_chunks;
	let broker_fee = swap.broker_fee_taken.unwrap_or_default() * number_of_chunks;

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
		network_fee: AssetAndAmount::new(input_asset, network_fee),
		ingress_fee: AssetAndAmount::new(input_asset, ingress_fee),
		egress_fee: AssetAndAmount::new(output_asset, egress_fee),
		broker_fee: AssetAndAmount::new(input_asset, broker_fee),
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
	}
}
