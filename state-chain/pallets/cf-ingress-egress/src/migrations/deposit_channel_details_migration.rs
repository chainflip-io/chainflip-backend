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

use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::{Config, DepositChannelDetails};

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use crate::BoostStatus;
	use cf_chains::{ChannelRefundParameters, DepositChannel, ForeignChainAddress};
	use cf_primitives::Beneficiaries;
	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};

	use super::*;

	#[derive(PartialEq, Eq, Encode, Decode)]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub owner: T::AccountId,
		pub deposit_channel: DepositChannel<T::TargetChain>,
		pub opened_at: TargetChainBlockNumber<T, I>,
		pub expires_at: TargetChainBlockNumber<T, I>,
		pub action: ChannelAction<T::AccountId>,
		pub boost_fee: BasisPoints,
		pub boost_status: BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
			channel_metadata: Option<CcmChannelMetadata>,
			refund_params: ChannelRefundParameters<ForeignChainAddress, ()>,
			dca_params: Option<DcaParameters>,
		},
		LiquidityProvision {
			lp_account: AccountId,
			refund_address: ForeignChainAddress,
		},
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

pub struct DepositChannelDetailsMigration<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for DepositChannelDetailsMigration<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok((old::DepositChannelLookup::<T, I>::iter_keys().count() as u64).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		crate::DepositChannelLookup::<T, I>::translate_values::<old::DepositChannelDetails<T, I>, _>(
			|old_deposit_channel_details| {
				let action = match old_deposit_channel_details.action {
					old::ChannelAction::LiquidityProvision { lp_account, refund_address } =>
						ChannelAction::LiquidityProvision { lp_account, refund_address },
					old::ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
						channel_metadata,
						refund_params,
						dca_params,
					} => ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
						channel_metadata,
						refund_params: ChannelRefundParameters {
							retry_duration: refund_params.retry_duration,
							refund_address: refund_params.refund_address,
							min_price: refund_params.min_price,
							refund_ccm_metadata: None,
						},
						dca_params,
					},
				};

				Some(DepositChannelDetails::<T, I> {
					owner: old_deposit_channel_details.owner,
					deposit_channel: old_deposit_channel_details.deposit_channel,
					opened_at: old_deposit_channel_details.opened_at,
					expires_at: old_deposit_channel_details.expires_at,
					action,
					boost_fee: old_deposit_channel_details.boost_fee,
					boost_status: old_deposit_channel_details.boost_status,
				})
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_deposit_channel_lookup_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_deposit_channel_lookup_count =
			crate::DepositChannelLookup::<T, I>::iter().count() as u64;

		assert_eq!(pre_deposit_channel_lookup_count, post_deposit_channel_lookup_count);
		Ok(())
	}
}
