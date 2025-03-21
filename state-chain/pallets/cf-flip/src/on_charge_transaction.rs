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

use crate::{imbalances::Surplus, Config as FlipConfig, Pallet as Flip};
use cf_traits::{CallInfoId, TransactionFeeScaler, WaivedFees};
use frame_support::{
	pallet_prelude::InvalidTransaction,
	sp_runtime::{
		traits::{DispatchInfoOf, Get, Zero},
		RuntimeDebug,
	},
	traits::Imbalance,
	DefaultNoBound,
};
use frame_system::Config;
use pallet_transaction_payment::{Config as TxConfig, OnChargeTransaction};
use sp_std::marker::PhantomData;

/// Marker struct for implementation of [OnChargeTransaction].
///
/// Fees are burned.
/// Tips are ignored.
/// Any excess fees are refunded to the caller.
pub struct FlipTransactionPayment<T>(PhantomData<T>);

#[derive(DefaultNoBound, RuntimeDebug)]
pub struct TransactionPaymentLiquidityInfo<T: crate::Config + FlipConfig> {
	pub imbalance: Option<Surplus<T>>,
	pub call_info_id: Option<CallInfoId<T::AccountId>>,
}

impl<T: TxConfig + FlipConfig + Config> OnChargeTransaction<T> for FlipTransactionPayment<T> {
	type Balance = <T as FlipConfig>::Balance;
	type LiquidityInfo = TransactionPaymentLiquidityInfo<T>;

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

		let call_info_id = T::TransactionFeeScaler::call_info(call, who);

		if call_info_id.is_some() {
			fee = sp_std::cmp::max(fee, T::SpamPreventionUpfrontFee::get());
		};

		if let Some(surplus) = Flip::<T>::try_debit(who, fee) {
			Ok(if surplus.peek().is_zero() {
				Default::default()
			} else {
				TransactionPaymentLiquidityInfo { imbalance: Some(surplus), call_info_id }
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
		if let Some(surplus) = escrow.imbalance {
			// It's possible the account was deleted during extrinsic execution. If this is the
			// case, we shouldn't refund anything, we can just burn all fees in escrow.
			let pre_scaled_fee_to_burn = if frame_system::Pallet::<T>::account_exists(who) {
				corrected_fee
			} else {
				surplus.peek()
			};

			let to_burn = if let Some(call_info_id) = escrow.call_info_id {
				let before_count = crate::CallCounter::<T>::mutate(&call_info_id, |count| {
					let before_count = *count;
					*count += 1;
					before_count
				});
				match call_info_id {
					CallInfoId::Pool(_) => {
						let crate::ExponentBufferFeeConfig { buffer, exp_base } =
							crate::FeeScalingRateConfig::<T>::get();
						T::TransactionFeeScaler::scale_fee(
							pre_scaled_fee_to_burn,
							before_count.saturating_sub(buffer),
							exp_base,
						)
					},
				}
			} else {
				pre_scaled_fee_to_burn
			};

			// If there is a difference this will be reconciled when the result goes out of scope.
			let _imbalance = surplus.offset(Flip::<T>::burn(to_burn));
		}
		Ok(())
	}
}
