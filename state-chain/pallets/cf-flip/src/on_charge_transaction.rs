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

//! Chainflip transaction fees.
//!
//! The Chainflip network is permissioned and as such the main reasons for fees are (a) to encourage
//! 'good' behaviour and (b) to ensure that only funded actors can submit extrinsics to the network.

use crate::{imbalances::Surplus, Config as FlipConfig, OpaqueCallIndex, Pallet as Flip};
use cf_primitives::{FlipBalance, FLIPPERINOS_PER_FLIP};
use cf_traits::{AccountInfo, WaivedFees};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::InvalidTransaction,
	sp_runtime::{
		traits::{DispatchInfoOf, Zero},
		RuntimeDebug,
	},
	traits::Imbalance,
};
use frame_system::Config;
use pallet_transaction_payment::{Config as TxConfig, OnChargeTransaction};
use scale_info::TypeInfo;
use sp_runtime::traits::Saturating;
use sp_std::marker::PhantomData;

/// Marker struct for implementation of [OnChargeTransaction].
///
/// Fees are burned.
/// Tips are ignored.
/// Any excess fees are refunded to the caller.
pub struct FlipTransactionPayment<T>(PhantomData<T>);

pub const UP_FRONT_ESCROW_FEE: FlipBalance = FLIPPERINOS_PER_FLIP;

pub type CallIndexFor<T> = <<T as crate::Config>::CallIndexer as CallIndexer<
	<T as frame_system::Config>::RuntimeCall,
>>::CallIndex;

impl<T: TxConfig + FlipConfig + Config> OnChargeTransaction<T> for FlipTransactionPayment<T> {
	type Balance = <T as FlipConfig>::Balance;
	type LiquidityInfo = Option<(Surplus<T>, Option<CallIndexFor<T>>)>;

	fn withdraw_fee(
		who: &T::AccountId,
		call: &<T as frame_system::Config>::RuntimeCall,
		_dispatch_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		mut fee: Self::Balance,
		_tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, frame_support::unsigned::TransactionValidityError> {
		if T::WaivedFees::should_waive_fees(call, who) {
			return Ok(Default::default())
		}

		// Check if there's an upfront fee for spam prevention.
		// If the user has less than the upfront fee, we escrow whatever they have.
		let escrowed_fee = core::cmp::min(Flip::<T>::balance(who), UP_FRONT_ESCROW_FEE.into());
		let call_index =
			T::CallIndexer::call_index(call).inspect(|_| fee = sp_std::cmp::max(fee, escrowed_fee));

		if let Some(surplus) = Flip::<T>::try_debit(who, fee) {
			Ok(if surplus.peek().is_zero() {
				Default::default()
			} else {
				Some((surplus, call_index))
			})
		} else {
			Err(InvalidTransaction::Payment.into())
		}
	}

	fn correct_and_deposit_fee(
		who: &T::AccountId,
		_dispatch_info: &sp_runtime::traits::DispatchInfoOf<
			<T as frame_system::Config>::RuntimeCall,
		>,
		_post_info: &sp_runtime::traits::PostDispatchInfoOf<
			<T as frame_system::Config>::RuntimeCall,
		>,
		corrected_fee: Self::Balance,
		_tip: Self::Balance,
		escrow: Self::LiquidityInfo,
	) -> Result<(), frame_support::unsigned::TransactionValidityError> {
		if let Some((surplus, call_index)) = escrow {
			// It's possible the account was deleted during extrinsic execution. If this is the
			// case, we shouldn't refund anything, we can just burn all fees in escrow.
			let to_burn = if frame_system::Pallet::<T>::account_exists(who) {
				if let Some(call_index) = call_index {
					corrected_fee.saturating_mul(
						crate::CallCounter::<T>::mutate(
							OpaqueCallIndex::from((who.clone(), call_index)),
							|count| {
								*count += 1;
								crate::FeeScalingRate::<T>::get().multiplier_at_call_count(*count)
							},
						)
						.into(),
					)
				} else {
					corrected_fee
				}
			} else {
				surplus.peek()
			};

			// If there is a difference this will be reconciled when the result goes out of scope.
			let _imbalance = surplus.offset(Flip::<T>::burn(to_burn));
		}
		Ok(())
	}
}

/// Converts a call into a call index to allow it to be categorised for fee scaling.
pub trait CallIndexer<Call> {
	type CallIndex: Encode + MaxEncodedLen;

	fn call_index(_call: &Call) -> Option<Self::CallIndex>;
}

impl<Call> CallIndexer<Call> for () {
	type CallIndex = ();

	fn call_index(_call: &Call) -> Option<Self::CallIndex> {
		None
	}
}

#[derive(
	Encode, Decode, TypeInfo, MaxEncodedLen, Clone, Copy, PartialEq, Eq, RuntimeDebug, Default,
)]
pub enum FeeScalingRateConfig {
	/// No scaling for the first `threshold` calls, scale by `(call_count - threshold)^exponent`
	/// thereafter.
	DelayedExponential { threshold: u16, exponent: u16 },
	#[default]
	NoScaling,
}

impl FeeScalingRateConfig {
	pub fn multiplier_at_call_count(&self, call_count: u16) -> u16 {
		match self {
			FeeScalingRateConfig::DelayedExponential { threshold, exponent } => core::cmp::max(
				1,
				call_count.saturating_sub(*threshold).saturating_pow(*exponent as u32),
			),
			FeeScalingRateConfig::NoScaling => 1,
		}
	}
}

#[test]
fn fee_scaling() {
	macro_rules! test_expected_scaling_factors {
		($name:literal, $config:expr, $expected:expr) => {
			let multipliers =
				(1..=10).map(|i| $config.multiplier_at_call_count(i)).collect::<Vec<_>>();
			assert_eq!(multipliers, $expected, "Scaling test failed for `{}` test.", $name,);
		};
	}

	test_expected_scaling_factors!("no_scaling", FeeScalingRateConfig::NoScaling, [1; 10]);
	test_expected_scaling_factors!(
		"linear_from_0",
		FeeScalingRateConfig::DelayedExponential { threshold: 0, exponent: 1 },
		[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
	);
	test_expected_scaling_factors!(
		"linear",
		FeeScalingRateConfig::DelayedExponential { threshold: 3, exponent: 1 },
		[1, 1, 1, 1, 2, 3, 4, 5, 6, 7]
	);
	test_expected_scaling_factors!(
		"quadratic",
		FeeScalingRateConfig::DelayedExponential { threshold: 2, exponent: 2 },
		[1, 1, 1, 4, 9, 16, 25, 36, 49, 64]
	);
}
