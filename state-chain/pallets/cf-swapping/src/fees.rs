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

use super::*;
use cf_primitives::{BASIS_POINTS_PER_MILLION, ONE_AS_BASIS_POINTS};
use cf_traits::{AssetConverter, EgressApi, ScheduledEgressDetails};

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
	/// A map of beneficiaries and their accumulated broker fee. The amount is in
	/// terms of the output asset. Used to pay out the brokers at the end of a swap request.
	fee_and_accumulated: BTreeMap<Beneficiary<AccountId>, AssetAmount>,
}

impl<AccountId: core::fmt::Debug + Ord> BrokerFeesTracker<AccountId> {
	pub fn new(beneficiaries: Beneficiaries<AccountId>) -> Self {
		// Sanity check: it should already not be possible to open a channel with broker fees
		// this high, but if the total broker fee would exceed 100% we charge no broker fee
		// instead (for simplicity):
		let total_fee_bps = beneficiaries
			.iter()
			.fold(0u16, |total_bps, Beneficiary { bps, .. }| total_bps.saturating_add(*bps));

		if total_fee_bps > ONE_AS_BASIS_POINTS {
			Self { fee_and_accumulated: Default::default() }
		} else {
			Self { fee_and_accumulated: beneficiaries.into_iter().map(|b| (b, 0)).collect() }
		}
	}

	pub fn take_all_fees(&mut self, input_amount: AssetAmount) -> FeeTaken {
		if input_amount.is_zero() {
			return FeeTaken { remaining_amount: 0, fee: 0 };
		}
		if self.fee_and_accumulated.is_empty() {
			return FeeTaken { remaining_amount: input_amount, fee: 0 };
		}

		let mut total_fee = 0;

		self.fee_and_accumulated.iter_mut().for_each(
			|(Beneficiary { bps, .. }, accumulated_fee)| {
				let fee =
					Permill::from_parts(*bps as u32 * BASIS_POINTS_PER_MILLION) * input_amount;
				accumulated_fee.saturating_accrue(fee);
				total_fee.saturating_accrue(fee)
			},
		);

		debug_assert!(total_fee <= input_amount, "Broker fee cannot be more than the amount");
		FeeTaken { remaining_amount: input_amount.saturating_sub(total_fee), fee: total_fee }
	}

	pub fn iter(&self) -> impl Iterator<Item = (&Beneficiary<AccountId>, &AssetAmount)> {
		self.fee_and_accumulated.iter()
	}

	#[cfg(feature = "try-runtime")]
	pub fn sum_fee_bps(&self) -> cf_primitives::BasisPoints {
		self.fee_and_accumulated
			.keys()
			.fold(0, |total_bps, Beneficiary { bps, .. }| total_bps.saturating_add(*bps))
	}
}

impl<T: Config> Pallet<T> {
	pub(crate) fn trigger_withdrawal(
		account_id: &T::AccountId,
		asset: Asset,
		destination_address: ForeignChainAddress,
	) -> DispatchResult {
		let earned_fees = T::BalanceApi::get_balance(account_id, asset);
		ensure!(earned_fees != 0, Error::<T>::NoFundsAvailable);
		T::BalanceApi::try_debit_account(account_id, asset, earned_fees)?;

		let ScheduledEgressDetails { egress_id, egress_amount, fee_withheld } =
			T::EgressHandler::schedule_egress(
				asset,
				earned_fees,
				destination_address.clone(),
				None,
			)
			.map_err(Into::into)?;

		Self::deposit_event(Event::<T>::WithdrawalRequested {
			account_id: account_id.clone(),
			egress_amount,
			egress_asset: asset,
			egress_fee: fee_withheld,
			destination_address: T::AddressConverter::to_encoded_address(destination_address),
			egress_id,
		});

		Ok(())
	}

	pub fn assemble_and_validate_broker_fees(
		broker_id: T::AccountId,
		broker_commission: BasisPoints,
		affiliate_fees: Affiliates<T::AccountId>,
	) -> Result<Beneficiaries<T::AccountId>, DispatchError> {
		let beneficiaries = [Beneficiary { account: broker_id, bps: broker_commission }]
			.into_iter()
			.chain(affiliate_fees.iter().cloned())
			.collect::<Vec<_>>()
			.try_into()
			.expect(
				"We are pushing affiliates + 1 which is exactly the maximum Beneficiaries size",
			);
		Pallet::<T>::validate_broker_fees(&beneficiaries)?;
		Ok(beneficiaries)
	}

	/// Gets the network fee rate and minimum in usdc terms for a swap between the given input
	/// and output assets, taking into account whether it's an internal swap or not.
	pub(crate) fn get_network_fee(
		input_asset: Asset,
		output_asset: Asset,
		is_internal_swap: bool,
	) -> FeeRateAndMinimum {
		let (input_asset_fee, output_asset_fee, usdc_minimum) = if is_internal_swap {
			let default_fee = InternalSwapNetworkFee::<T>::get();
			(
				InternalSwapNetworkFeeForAsset::<T>::get(input_asset).unwrap_or(default_fee.rate),
				InternalSwapNetworkFeeForAsset::<T>::get(output_asset).unwrap_or(default_fee.rate),
				default_fee.minimum,
			)
		} else {
			let default_fee = NetworkFee::<T>::get();
			(
				NetworkFeeForAsset::<T>::get(input_asset).unwrap_or(default_fee.rate),
				NetworkFeeForAsset::<T>::get(output_asset).unwrap_or(default_fee.rate),
				default_fee.minimum,
			)
		};

		FeeRateAndMinimum { rate: input_asset_fee.max(output_asset_fee), minimum: usdc_minimum }
	}

	pub fn get_network_fee_rate_for_swap(
		input_asset: Asset,
		output_asset: Asset,
		is_internal_swap: bool,
	) -> Permill {
		Self::get_network_fee(input_asset, output_asset, is_internal_swap).rate
	}

	/// Gets the network fee rate and minimum in the input asset terms.
	pub fn get_network_fee_for_swap(
		input_asset: Asset,
		output_asset: Asset,
		is_internal_swap: bool,
		with_minimum: bool,
	) -> FeeRateAndMinimum {
		// Find the correct fee values in USDC
		let FeeRateAndMinimum { rate, minimum: usdc_minimum } =
			Self::get_network_fee(input_asset, output_asset, is_internal_swap);

		// Convert the minimum amount to the input asset
		let minimum = if with_minimum {
			Pallet::<T>::calculate_input_for_desired_output_or_default_to_zero(
				input_asset,
				Asset::Usdc,
				usdc_minimum,
				false, // no network fee
				false, // not internal
			)
		} else {
			0
		};

		FeeRateAndMinimum { rate, minimum }
	}
}
