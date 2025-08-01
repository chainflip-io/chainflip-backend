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

use cf_chains::{
	assets::any::ForeignChainAndAsset,
	instances::{
		ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, PolkadotInstance,
		SolanaInstance,
	},
};
use cf_primitives::{
	AccountId, Asset, AssetAmount, BasisPoints, Beneficiary, DcaParameters, IngressOrEgress,
};
use cf_traits::OrderId;
use pallet_cf_swapping::{BatchExecutionError, FeeType, NetworkFeeTracker, Swap};
use sp_runtime::DispatchError;

use crate::{
	runtime_apis::{
		CcmData, DispatchErrorWithMessage, FeeTypes, SimulateSwapAdditionalOrder,
		SimulatedSwapInformation,
	},
	LiquidityPools, Runtime, Swapping,
};

use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{collections::btree_set::BTreeSet, vec, vec::Vec};

pub fn simulate_swap(
	input_asset: Asset,
	output_asset: Asset,
	input_amount: AssetAmount,
	broker_commission: BasisPoints,
	dca_parameters: Option<DcaParameters>,
	ccm_data: Option<CcmData>,
	exclude_fees: BTreeSet<FeeTypes>,
	additional_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
	is_internal: Option<bool>,
) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage> {
	let is_internal = is_internal.unwrap_or_default();
	let mut exclude_fees = exclude_fees;
	if is_internal {
		exclude_fees.insert(FeeTypes::IngressDepositChannel);
		exclude_fees.insert(FeeTypes::Egress);
		exclude_fees.insert(FeeTypes::IngressVaultSwap);
	}

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

	fn remove_fees(
		ingress_or_egress: IngressOrEgress,
		asset: Asset,
		amount: AssetAmount,
	) -> (AssetAmount, AssetAmount) {
		use pallet_cf_ingress_egress::AmountAndFeesWithheld;

		match asset.into() {
			ForeignChainAndAsset::Ethereum(asset) => {
				let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

				(amount_after_fees, fees_withheld)
			},
			ForeignChainAndAsset::Polkadot(asset) => {
				let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

				(amount_after_fees, fees_withheld)
			},
			ForeignChainAndAsset::Bitcoin(asset) => {
				let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

				(amount_after_fees.into(), fees_withheld.into())
			},
			ForeignChainAndAsset::Arbitrum(asset) => {
				let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

				(amount_after_fees, fees_withheld)
			},
			ForeignChainAndAsset::Solana(asset) => {
				let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

				(amount_after_fees.into(), fees_withheld.into())
			},
			ForeignChainAndAsset::Assethub(asset) => {
				let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

				(amount_after_fees, fees_withheld)
			},
		}
	}

	let include_fee = |fee_type: FeeTypes| !exclude_fees.contains(&fee_type);

	// Default to using the DepositChannel fee unless specified.
	let (amount_to_swap, ingress_fee) = if include_fee(FeeTypes::IngressDepositChannel) {
		remove_fees(IngressOrEgress::IngressDepositChannel, input_asset, input_amount)
	} else if include_fee(FeeTypes::IngressVaultSwap) {
		remove_fees(IngressOrEgress::IngressVaultSwap, input_asset, input_amount)
	} else {
		(input_amount, 0u128)
	};

	let mut fees_vec = vec![];

	if include_fee(FeeTypes::Network) {
		let rate_and_min = pallet_cf_swapping::Pallet::<Runtime>::get_network_fee_for_swap(
			input_asset,
			output_asset,
			is_internal,
		);

		// Ignoring the minimum to simplify the simulation.
		// Note: This will make the simulation inaccurate if the overall network fee is smaller than
		// the minimum.
		fees_vec.push(FeeType::NetworkFee(NetworkFeeTracker::new_without_minimum(
			rate_and_min.clone(),
		)));
	}

	// If no DCA parameter is given, swap the entire amount with 1 chunk.
	let number_of_chunks: u128 = dca_parameters
		.map(|dca| sp_std::cmp::max(dca.number_of_chunks, 1u32))
		.unwrap_or(1u32)
		.into();
	let amount_per_chunk: u128 = amount_to_swap / number_of_chunks;

	if broker_commission > 0 {
		let fee = FeeType::BrokerFee(
			vec![Beneficiary { account: AccountId::new([0xbb; 32]), bps: broker_commission }]
				.try_into()
				.expect("Beneficiary with a length of 1 must be within length bound."),
		);
		fees_vec.push(fee.clone());
	}

	let swap_output = &Swapping::simulate_swaps(vec![Swap::new(
		Default::default(), // Swap id
		Default::default(), // Swap request id
		input_asset,
		output_asset,
		amount_per_chunk,
		None, // Refund params
		fees_vec,
		Default::default(), // Execution block
	)])
	.map_err(|e| match e {
		BatchExecutionError::SwapLegFailed { .. } => DispatchError::Other("Swap leg failed."),
		BatchExecutionError::PriceViolation { .. } => DispatchError::Other(
			"Price Violation: Some swaps failed due to Price Impact Limitations.",
		),
		BatchExecutionError::DispatchError { error } => error,
	})?;
	let swap = &swap_output[0];

	// Extrapolate the total by multiplying the chunk by the number of chunks
	let network_fee = swap.network_fee_taken.unwrap_or_default() * number_of_chunks;
	let intermediary = swap.intermediate_amount().map(|amount| amount * number_of_chunks);
	let output = swap.final_output.unwrap_or_default() * number_of_chunks;
	let broker_fee = swap.broker_fee_taken.unwrap_or_default() * number_of_chunks;

	let (output, egress_fee) = if include_fee(FeeTypes::Egress) {
		let egress = match ccm_data {
			Some(CcmData { gas_budget, message_length }) =>
				IngressOrEgress::EgressCcm { gas_budget, message_length: message_length as usize },
			None => IngressOrEgress::Egress,
		};
		remove_fees(egress, output_asset, output)
	} else {
		(output, 0u128)
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
