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

pub struct BrokerDeregistrationCheck<T>(PhantomData<T>);

impl<T: Config> DeregistrationCheck for BrokerDeregistrationCheck<T> {
	type AccountId = T::AccountId;
	type Error = Error<T>;

	fn check(account_id: &Self::AccountId) -> Result<(), Self::Error> {
		ensure!(
			!BrokerPrivateBtcChannels::<T>::contains_key(account_id),
			Error::<T>::PrivateChannelExistsForBroker
		);
		ensure!(
			AffiliateAccountDetails::<T>::iter_key_prefix(account_id).all(|affiliate_account_id| {
				T::BalanceApi::free_balances(&affiliate_account_id)
					.iter()
					.all(|(_, amount)| *amount == 0)
			}),
			Error::<T>::AffiliateEarnedFeesNotWithdrawn
		);

		Ok(())
	}
}

impl<T: Config> cf_traits::FlipBurnOrMoveInfo for Pallet<T> {
	fn take_flip_to_burn() -> i128 {
		FlipToBurn::<T>::take()
	}
	fn take_flip_to_be_sent_to_gateway() -> AssetAmount {
		FlipToBeSentToGateway::<T>::take()
	}
}

impl<T: Config> SwapParameterValidation for Pallet<T> {
	type AccountId = T::AccountId;

	fn get_swap_limits() -> cf_traits::SwapLimits {
		cf_traits::SwapLimits {
			max_swap_retry_duration_blocks: MaxSwapRetryDurationBlocks::<T>::get(),
			max_swap_request_duration_blocks: MaxSwapRequestDurationBlocks::<T>::get(),
		}
	}

	fn validate_refund_params(
		input_asset: Asset,
		output_asset: Asset,
		retry_duration: BlockNumber,
		max_oracle_price_slippage: Option<BasisPoints>,
	) -> Result<(), DispatchError> {
		// Check that the retry duration is within limits.
		let max_swap_retry_duration_blocks = MaxSwapRetryDurationBlocks::<T>::get();
		if retry_duration > max_swap_retry_duration_blocks {
			return Err(DispatchError::from(Error::<T>::RetryDurationTooHigh));
		}

		// Check that the oracle prices are available for the assets.
		if let Some(_max_oracle_price_slippage) = max_oracle_price_slippage {
			if T::PriceFeedApi::get_price(input_asset).is_none() ||
				T::PriceFeedApi::get_price(output_asset).is_none()
			{
				return Err(DispatchError::from(Error::<T>::OraclePriceNotAvailable));
			}
		}

		Ok(())
	}

	fn validate_dca_params(params: &cf_primitives::DcaParameters) -> Result<(), DispatchError> {
		let max_swap_request_duration_blocks = MaxSwapRequestDurationBlocks::<T>::get();

		if params.number_of_chunks != 1 {
			if params.number_of_chunks == 0 {
				return Err(DispatchError::from(Error::<T>::ZeroNumberOfChunksNotAllowed));
			}
			if params.chunk_interval == 0 {
				return Err(DispatchError::from(Error::<T>::ChunkIntervalTooLow));
			}
			if let Some(total_swap_request_duration) =
				params.number_of_chunks.saturating_sub(1).checked_mul(params.chunk_interval)
			{
				if total_swap_request_duration > max_swap_request_duration_blocks {
					return Err(DispatchError::from(Error::<T>::SwapRequestDurationTooLong));
				}
			} else {
				return Err(DispatchError::from(Error::<T>::InvalidDcaParameters));
			}
		}
		Ok(())
	}

	fn validate_broker_fees(
		broker_fees: &Beneficiaries<Self::AccountId>,
	) -> Result<(), DispatchError> {
		let total_bps = broker_fees
			.iter()
			.fold(0, |total, Beneficiary { bps, .. }| total.saturating_add(*bps));

		ensure!(total_bps <= 1000, Error::<T>::BrokerCommissionBpsTooHigh);

		Ok(())
	}

	fn get_minimum_vault_swap_fee_for_broker(broker_id: &Self::AccountId) -> BasisPoints {
		VaultSwapMinimumBrokerFee::<T>::get(broker_id)
	}
}

impl<T: Config> AffiliateRegistry for Pallet<T> {
	type AccountId = T::AccountId;

	fn get_account_id(
		broker_id: &Self::AccountId,
		affiliate_short_id: AffiliateShortId,
	) -> Option<Self::AccountId> {
		AffiliateIdMapping::<T>::get(broker_id, affiliate_short_id)
	}

	/// This function iterates over a storage map. Only for use in rpc methods.
	fn get_short_id(
		broker_id: &Self::AccountId,
		affiliate_id: &Self::AccountId,
	) -> Option<AffiliateShortId> {
		AffiliateAccountDetails::<T>::get(broker_id, affiliate_id).map(|details| details.short_id)
	}

	fn reverse_mapping(broker_id: &Self::AccountId) -> BTreeMap<Self::AccountId, AffiliateShortId> {
		AffiliateIdMapping::<T>::iter_prefix(broker_id)
			.map(|(short_id, account_id)| (account_id, short_id))
			.collect()
	}
}
