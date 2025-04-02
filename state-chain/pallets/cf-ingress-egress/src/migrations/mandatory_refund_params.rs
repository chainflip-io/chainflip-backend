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

use crate::{Config, DepositChannelDetails, DepositChannelLookup};

pub struct Migration<T, I>(sp_std::marker::PhantomData<(T, I)>);

mod old {
	use crate::{
		Asset, BasisPoints, Beneficiaries, BoostStatus, ChannelRefundParameters, Config,
		DcaParameters, DepositChannel, ForeignChainAddress, TargetChainAmount,
		TargetChainBlockNumber,
	};

	use cf_chains::{CcmChannelMetadataUnchecked, CcmMessage, MAX_CCM_ADDITIONAL_DATA_LENGTH};
	use cf_primitives::GasAmount;

	use frame_support::pallet_prelude::*;

	pub type CcmAdditionalData = BoundedVec<u8, ConstU32<MAX_CCM_ADDITIONAL_DATA_LENGTH>>;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct CcmChannelMetadata {
		pub message: CcmMessage,
		pub gas_budget: GasAmount,
		pub ccm_additional_data: CcmAdditionalData,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub owner: T::AccountId,
		pub deposit_channel: DepositChannel<T::TargetChain>,
		pub opened_at: TargetChainBlockNumber<T, I>,
		pub expires_at: TargetChainBlockNumber<T, I>,
		pub action: ChannelAction<T::AccountId>,
		pub boost_fee: BasisPoints,
		pub boost_status: BoostStatus<TargetChainAmount<T, I>>,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
			channel_metadata: Option<CcmChannelMetadata>,
			refund_params: Option<ChannelRefundParameters<ForeignChainAddress>>,
			dca_params: Option<DcaParameters>,
		},
		LiquidityProvision {
			lp_account: AccountId,
			refund_address: Option<ForeignChainAddress>,
		},
	}

	impl<A> TryFrom<ChannelAction<A>> for crate::ChannelAction<A> {
		type Error = ();
		fn try_from(action: ChannelAction<A>) -> Result<crate::ChannelAction<A>, Self::Error> {
			Ok(match action {
				ChannelAction::Swap {
					destination_asset,
					destination_address,
					broker_fees,
					channel_metadata,
					refund_params,
					dca_params,
				} => crate::ChannelAction::Swap {
					destination_asset,
					destination_address: destination_address.clone(),
					broker_fees,
					// Map all metadata into Checked metadata assuming All `ccm_additional_data` is
					// valid.
					channel_metadata: channel_metadata
						.map(|metadata| {
							CcmChannelMetadataUnchecked {
								message: metadata.message,
								gas_budget: metadata.gas_budget,
								ccm_additional_data: metadata.ccm_additional_data.into(),
							}
							.to_checked(destination_asset, destination_address)
						})
						.transpose()
						.unwrap_or_default(),
					refund_params: refund_params.ok_or(())?,
					dca_params,
				},
				ChannelAction::LiquidityProvision { lp_account, refund_address } =>
					crate::ChannelAction::LiquidityProvision {
						lp_account,
						refund_address: refund_address.ok_or(())?,
					},
			})
		}
	}
}

impl<T: Config<I>, I: 'static> frame_support::traits::UncheckedOnRuntimeUpgrade
	for Migration<T, I>
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Translate the storage from old to new format
		DepositChannelLookup::<T, I>::translate::<old::DepositChannelDetails<T, I>, _>(
			|channel_id,
			 old::DepositChannelDetails {
			     owner,
			     deposit_channel,
			     opened_at,
			     expires_at,
			     action,
			     boost_fee,
			     boost_status,
			 }| {
				Some(DepositChannelDetails {
					owner,
					deposit_channel,
					opened_at,
					expires_at,
					action: action
						.try_into()
						.inspect_err(|_| {
							log::error!("No refund parameters for channel_id: {:?}", channel_id);
						})
						.ok()?,
					boost_fee,
					boost_status,
				})
			},
		);

		Default::default()
	}
}
