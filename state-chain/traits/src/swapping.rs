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
	address::{AddressConverter, EncodedAddress},
	ccm_checker::DecodedCcmAdditionalData,
	AccountOrAddress, CcmDepositMetadata, CcmDepositMetadataChecked, Chain,
	ChannelRefundParametersCheckedInternal, ForeignChainAddress, SwapOrigin,
};
use cf_primitives::{
	Asset, AssetAmount, BasisPoints, Beneficiaries, BlockNumber, DcaParameters, Price, PriceLimits,
	SwapRequestId,
};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use crate::lending::LoanId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SwapType {
	Swap,
	NetworkFee,
	IngressEgressFee,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum LendingSwapType<AccountId> {
	Liquidation { borrower_id: AccountId, loan_id: LoanId },
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapOutputActionGeneric<Address, AccountId> {
	Egress {
		ccm_deposit_metadata: Option<CcmDepositMetadata<Address, DecodedCcmAdditionalData>>,
		output_address: Address,
	},
	CreditOnChain {
		account_id: AccountId,
	},
	CreditLendingPool {
		swap_type: LendingSwapType<AccountId>,
	},
	CreditFlipAndTransferToGateway {
		account_id: AccountId,
		flip_to_subtract_from_swap_output: AssetAmount,
	},
}

pub type SwapOutputAction<AccountId> = SwapOutputActionGeneric<ForeignChainAddress, AccountId>;
pub type SwapOutputActionEncoded<AccountId> = SwapOutputActionGeneric<EncodedAddress, AccountId>;

impl<AccountId> SwapRequestType<AccountId> {
	pub fn into_encoded<Converter: AddressConverter>(self) -> SwapRequestTypeEncoded<AccountId> {
		match self {
			SwapRequestType::NetworkFee => SwapRequestTypeEncoded::NetworkFee,
			SwapRequestType::IngressEgressFee => SwapRequestTypeEncoded::IngressEgressFee,
			SwapRequestType::Regular { output_action } => SwapRequestTypeEncoded::Regular {
				output_action: match output_action {
					SwapOutputAction::Egress { ccm_deposit_metadata, output_address } =>
						SwapOutputActionEncoded::Egress {
							output_address: Converter::to_encoded_address(output_address),
							ccm_deposit_metadata: ccm_deposit_metadata
								.map(|metadata| metadata.to_encoded::<Converter>()),
						},
					SwapOutputAction::CreditOnChain { account_id } =>
						SwapOutputActionEncoded::CreditOnChain { account_id },
					SwapOutputActionGeneric::CreditLendingPool { swap_type } =>
						SwapOutputActionEncoded::CreditLendingPool { swap_type },
					SwapOutputAction::CreditFlipAndTransferToGateway {
						account_id,
						flip_to_subtract_from_swap_output,
					} => SwapOutputActionEncoded::CreditFlipAndTransferToGateway {
						account_id,
						flip_to_subtract_from_swap_output,
					},
				},
			},
			SwapRequestType::RegularNoNetworkFee { output_action } =>
				SwapRequestTypeEncoded::RegularNoNetworkFee {
					output_action: match output_action {
						SwapOutputAction::Egress { ccm_deposit_metadata, output_address } =>
							SwapOutputActionEncoded::Egress {
								output_address: Converter::to_encoded_address(output_address),
								ccm_deposit_metadata: ccm_deposit_metadata
									.map(|metadata| metadata.to_encoded::<Converter>()),
							},
						SwapOutputAction::CreditOnChain { account_id } =>
							SwapOutputActionEncoded::CreditOnChain { account_id },
						SwapOutputActionGeneric::CreditLendingPool { swap_type } =>
							SwapOutputActionEncoded::CreditLendingPool { swap_type },
						SwapOutputAction::CreditFlipAndTransferToGateway {
							account_id,
							flip_to_subtract_from_swap_output,
						} => SwapOutputActionEncoded::CreditFlipAndTransferToGateway {
							account_id,
							flip_to_subtract_from_swap_output,
						},
					},
				},
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapRequestTypeGeneric<Address, AccountId> {
	NetworkFee,
	IngressEgressFee,
	Regular { output_action: SwapOutputActionGeneric<Address, AccountId> },
	RegularNoNetworkFee { output_action: SwapOutputActionGeneric<Address, AccountId> },
}

pub type SwapRequestType<AccountId> = SwapRequestTypeGeneric<ForeignChainAddress, AccountId>;
pub type SwapRequestTypeEncoded<AccountId> = SwapRequestTypeGeneric<EncodedAddress, AccountId>;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(AccountId))]
pub enum ExpiryBehaviour<AccountId> {
	NoExpiry,
	RefundIfExpires {
		retry_duration: cf_primitives::BlockNumber,
		refund_address: AccountOrAddress<AccountId, ForeignChainAddress>,
		refund_ccm_metadata: Option<CcmDepositMetadataChecked<ForeignChainAddress>>,
	},
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(AccountId))]
pub struct PriceLimitsAndExpiry<AccountId> {
	pub expiry_behaviour: ExpiryBehaviour<AccountId>,
	pub min_price: Price,
	pub max_oracle_price_slippage: Option<BasisPoints>,
}

impl<AccountId> From<ChannelRefundParametersCheckedInternal<AccountId>>
	for PriceLimitsAndExpiry<AccountId>
{
	fn from(params: ChannelRefundParametersCheckedInternal<AccountId>) -> Self {
		Self {
			expiry_behaviour: ExpiryBehaviour::RefundIfExpires {
				retry_duration: params.retry_duration,
				refund_address: params.refund_address,
				refund_ccm_metadata: params.refund_ccm_metadata,
			},
			min_price: params.min_price,
			max_oracle_price_slippage: params.max_oracle_price_slippage,
		}
	}
}

#[derive(Debug, PartialEq, Eq)]
pub struct SwapExecutionProgress {
	pub remaining_input_amount: AssetAmount,
	pub accumulated_output_amount: AssetAmount,
}

pub trait SwapRequestHandler {
	type AccountId: Clone;

	fn init_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		request_type: SwapRequestType<Self::AccountId>,
		broker_fees: Beneficiaries<Self::AccountId>,
		price_limits_and_expiry: Option<PriceLimitsAndExpiry<Self::AccountId>>,
		dca_params: Option<DcaParameters>,
		origin: SwapOrigin<Self::AccountId>,
	) -> SwapRequestId;

	fn init_network_fee_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
	) -> SwapRequestId {
		Self::init_swap_request(
			input_asset,
			input_amount,
			Asset::Flip,
			SwapRequestType::NetworkFee,
			Default::default(), /* broker fees */
			None,               /* refund params */
			None,               /* dca params */
			SwapOrigin::Internal,
		)
	}

	fn init_ingress_egress_fee_swap_request<C: Chain>(
		input_asset: C::ChainAsset,
		input_amount: C::ChainAmount,
	) -> SwapRequestId {
		Self::init_swap_request(
			input_asset.into(),
			input_amount.into(),
			C::GAS_ASSET.into(),
			SwapRequestType::IngressEgressFee,
			Default::default(), /* broker fees */
			None,               /* refund params */
			None,               /* dca params */
			SwapOrigin::Internal,
		)
	}

	fn init_internal_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		retry_duration: BlockNumber,
		price_limits: PriceLimits,
		dca_params: Option<DcaParameters>,
		account_id: Self::AccountId,
	) -> SwapRequestId {
		Self::init_swap_request(
			input_asset,
			input_amount,
			output_asset,
			SwapRequestType::Regular {
				output_action: SwapOutputAction::CreditOnChain { account_id: account_id.clone() },
			},
			Default::default(), /* no broker fees */
			Some(PriceLimitsAndExpiry {
				expiry_behaviour: ExpiryBehaviour::RefundIfExpires {
					retry_duration,
					refund_address: AccountOrAddress::InternalAccount(account_id.clone()),
					refund_ccm_metadata: None,
				},
				min_price: price_limits.min_price,
				max_oracle_price_slippage: price_limits.max_oracle_price_slippage,
			}),
			dca_params,
			SwapOrigin::OnChainAccount(account_id),
		)
	}

	fn inspect_swap_request(swap_request_id: SwapRequestId) -> Option<SwapExecutionProgress>;

	fn abort_swap_request(swap_request_id: SwapRequestId) -> Option<SwapExecutionProgress>;
}
