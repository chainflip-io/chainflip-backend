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

#[cfg(feature = "runtime-benchmarks")]
use crate::benchmarking_value::BenchmarkValue;

use crate::{
	address::{AddressConverter, EncodedAddress, ForeignChainAddress, IntoForeignChainAddress},
	ccm_checker::CcmValidityChecker,
	CcmChannelMetadataUnchecked, CcmDepositMetadataChecked, CcmDepositMetadataUnchecked, Chain,
};
use cf_amm_math::Price;
use cf_primitives::{Asset, AssetAmount, BasisPoints};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::sp_runtime::DispatchError;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{cmp::Ord, convert::Into, fmt::Debug, prelude::*};

/// AccountOrAddress is a enum that can represent an internal account or an external address.
/// This is used to represent the destination address for an egress or an internal account
/// to move funds internally.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
pub enum AccountOrAddress<AccountId, Address> {
	InternalAccount(AccountId),
	ExternalAddress(Address),
}

/// Generic type for Refund Parameters.
///
/// The abstract `RefundDetails` represents additional metadata that may be required for refunding
/// via CCM. Before verification this is an unchecked byte payload.
///
/// Avoid constructing this type directly: prefer to use one of the aliases and/or conversion
/// methods. Usually you start with `ChannelRefundParametersUnchecked` and then convert it via
/// `ChannelRefundParametersUnchecked<ForeignChainAddress>` into
/// `ChannelRefundParametersChecked<ForeignChainAddress>`.
///
/// Example:
/// ```ignore
/// let checked_params = unchecked_refund_params
///     .map_refund_address_to_foreign_chain_address::<Solana>()
///     .into_checked(
///         source_address,
///         refund_asset
///     )?;
/// ```
#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
)]
pub struct ChannelRefundParameters<A, CcmRefundDetails> {
	pub retry_duration: cf_primitives::BlockNumber,
	pub refund_address: A,
	pub min_price: Price,
	pub refund_ccm_metadata: CcmRefundDetails,
	pub max_oracle_price_slippage: Option<BasisPoints>,
}

#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
)]
pub struct ChannelRefundParametersV0<A> {
	pub retry_duration: cf_primitives::BlockNumber,
	pub refund_address: A,
	pub min_price: Price,
}

/// Refund parameters with CCM metadata that has not yet been checked for validity.
///
/// Most incoming refund parameters will be of this type.
pub type ChannelRefundParametersUnchecked<A> =
	ChannelRefundParameters<A, Option<CcmChannelMetadataUnchecked>>;

/// Refund parameters with CCM metadata that *has* been checked for validity.
pub type ChannelRefundParametersChecked<A> =
	ChannelRefundParameters<A, Option<CcmDepositMetadataChecked<ForeignChainAddress>>>;

/// Convenience alias for unchecked refund parameters with encoded addresses. This is most commonly
/// used in State Chain events and extrinsics.
pub type ChannelRefundParametersUncheckedEncoded = ChannelRefundParametersUnchecked<EncodedAddress>;

/// This is the type used in internal APIs, for example when making swap request.
pub type ChannelRefundParametersCheckedInternal<AccountId> =
	ChannelRefundParametersChecked<AccountOrAddress<AccountId, ForeignChainAddress>>;

/// Convenience alias for unchecked refund parameters with the refund address as an `AddressString`.
/// This is used in RPCs where we require the refund address to be (de)serializable.
#[cfg(feature = "std")]
pub type RpcChannelRefundParameters =
	ChannelRefundParametersUnchecked<crate::address::AddressString>;

/// Convenience alias for unchecked refund parameters with the refund address as the chain's account
/// type.
pub type ChannelRefundParametersForChain<C> =
	ChannelRefundParametersUnchecked<<C as Chain>::ChainAccount>;

impl<A, D> ChannelRefundParameters<A, D> {
	pub fn min_output_amount(&self, input_amount: AssetAmount) -> AssetAmount {
		use sp_runtime::traits::UniqueSaturatedInto;
		cf_amm_math::output_amount_ceil(input_amount.into(), self.min_price).unique_saturated_into()
	}
}

#[cfg(feature = "std")]
impl RpcChannelRefundParameters {
	pub fn parse_refund_address_for_chain(
		self,
		chain: cf_primitives::ForeignChain,
	) -> anyhow::Result<ChannelRefundParametersUncheckedEncoded> {
		Ok(ChannelRefundParameters {
			retry_duration: self.retry_duration,
			refund_address: self.refund_address.try_parse_to_encoded_address(chain)?,
			min_price: self.min_price,
			refund_ccm_metadata: self.refund_ccm_metadata.clone(),
			max_oracle_price_slippage: self.max_oracle_price_slippage,
		})
	}
}

impl<A, D> ChannelRefundParameters<A, D> {
	pub fn map_address<B, F: FnOnce(A) -> B>(self, f: F) -> ChannelRefundParameters<B, D> {
		ChannelRefundParameters {
			retry_duration: self.retry_duration,
			refund_address: f(self.refund_address),
			min_price: self.min_price,
			refund_ccm_metadata: self.refund_ccm_metadata,
			max_oracle_price_slippage: self.max_oracle_price_slippage,
		}
	}
	pub fn try_map_address<B, E, F: FnOnce(A) -> Result<B, E>>(
		self,
		f: F,
	) -> Result<ChannelRefundParameters<B, D>, E> {
		Ok(ChannelRefundParameters {
			retry_duration: self.retry_duration,
			refund_address: f(self.refund_address)?,
			min_price: self.min_price,
			refund_ccm_metadata: self.refund_ccm_metadata,
			max_oracle_price_slippage: self.max_oracle_price_slippage,
		})
	}
}

impl<A> ChannelRefundParametersUnchecked<A> {
	/// Converts the refund address into the ForeignChainAddress type.
	pub fn map_refund_address_to_foreign_chain_address<C>(
		self,
	) -> ChannelRefundParametersUnchecked<ForeignChainAddress>
	where
		C: Chain<ChainAccount = A>,
		A: Clone + IntoForeignChainAddress<C>,
	{
		self.map_address(|addr| addr.clone().into_foreign_chain_address())
	}

	pub fn validate(
		&self,
		refund_asset: Asset,
		refund_address_decoded: ForeignChainAddress,
	) -> Result<(), DispatchError> {
		self.refund_ccm_metadata
			.as_ref()
			.map(|refund_ccm| {
				CcmValidityChecker::check_and_decode(
					refund_ccm,
					refund_asset,
					refund_address_decoded,
				)
			})
			.transpose()?;

		Ok(())
	}
}

impl ChannelRefundParametersUncheckedEncoded {
	/// Try to convert the refund address into the ForeignChainAddress type.
	pub fn try_map_refund_address_to_foreign_chain_address<C: AddressConverter>(
		self,
	) -> Result<ChannelRefundParametersUnchecked<ForeignChainAddress>, DispatchError> {
		Ok(self.try_map_address(|addr| {
			C::try_from_encoded_address(addr.clone()).map_err(|_| "Invalid refund address")
		})?)
	}
}

// NOTE: Currently CCM checking requires a ForeignChainAddress - hence the <ForeignChainAddress>
// constraint on this impl. We should be able to remove this requirement somehow.
impl ChannelRefundParametersUnchecked<ForeignChainAddress> {
	/// Checks the CCM Refund metadata payload and converts the address to the internal
	pub fn into_checked(
		self,
		source_address: Option<ForeignChainAddress>,
		refund_asset: Asset,
	) -> Result<ChannelRefundParametersChecked<ForeignChainAddress>, DispatchError> {
		{
			let source_chain = self.refund_address.chain();

			if self.refund_ccm_metadata.is_some() && !source_chain.ccm_support() {
				return Err(
					"Invalid refund parameter: Ccm not supported for the refund chain.".into()
				)
			}

			Ok(ChannelRefundParametersChecked {
				retry_duration: self.retry_duration,
				refund_address: self.refund_address.clone(),
				min_price: self.min_price,
				max_oracle_price_slippage: self.max_oracle_price_slippage,
				refund_ccm_metadata: self
					.refund_ccm_metadata
					.map(|channel_metadata| {
						CcmDepositMetadataUnchecked {
							channel_metadata,
							source_chain,
							source_address,
						}
						.to_checked(refund_asset, self.refund_address)
					})
					.transpose()?,
			})
		}
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<A: BenchmarkValue, D: BenchmarkValue> BenchmarkValue
	for ChannelRefundParameters<A, Option<D>>
{
	fn benchmark_value() -> Self {
		Self {
			retry_duration: BenchmarkValue::benchmark_value(),
			refund_address: BenchmarkValue::benchmark_value(),
			min_price: BenchmarkValue::benchmark_value(),
			refund_ccm_metadata: Some(BenchmarkValue::benchmark_value()),
			max_oracle_price_slippage: Some(BenchmarkValue::benchmark_value()),
		}
	}
}
