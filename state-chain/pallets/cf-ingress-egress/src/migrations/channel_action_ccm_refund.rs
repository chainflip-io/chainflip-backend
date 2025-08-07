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
#[cfg(feature = "try-runtime")]
use sp_std::collections::btree_set::BTreeSet;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;
	use cf_primitives::Price;

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub owner: T::AccountId,
		pub deposit_channel: DepositChannel<T::TargetChain>,
		pub opened_at: TargetChainBlockNumber<T, I>,
		pub expires_at: TargetChainBlockNumber<T, I>,
		pub action: ChannelAction<T::AccountId, T::TargetChain>,
		pub boost_fee: BasisPoints,
		pub boost_status: BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub struct ChannelRefundParameters<A> {
		pub retry_duration: cf_primitives::BlockNumber,
		pub refund_address: A,
		pub min_price: Price,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	#[allow(clippy::large_enum_variant)]
	pub enum ChannelAction<AccountId, C: Chain> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
			channel_metadata: Option<CcmChannelMetadataChecked>,
			refund_params: ChannelRefundParameters<ForeignChainAddress>,
			dca_params: Option<DcaParameters>,
		},
		LiquidityProvision {
			lp_account: AccountId,
			refund_address: ForeignChainAddress,
		},
		Refund {
			reason: RefundReason,
			refund_address: C::ChainAccount,
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

pub struct ChannelActionCcmRefund<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for ChannelActionCcmRefund<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let deposit_channels =
			old::DepositChannelLookup::<T, I>::iter_keys().collect::<BTreeSet<_>>();

		Ok(deposit_channels.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!("🍩 Running migration for Ingress-Egress pallet: Updating Refund Parameters.");
		crate::DepositChannelLookup::<T, I>::translate_values::<old::DepositChannelDetails<T, I>, _>(
			|old| {
				match old.action.clone() {
					old::ChannelAction::Swap { refund_params, .. } =>
					// Convert Refund param into Checked version.
						Some(ChannelRefundParametersCheckedInternal {
							retry_duration: refund_params.retry_duration,
							refund_address: AccountOrAddress::ExternalAddress(
								refund_params.refund_address,
							),
							min_price: refund_params.min_price,
							refund_ccm_metadata: None,
							max_oracle_price_slippage: None,
						}),
					_ => None,
				}
				.map(|checked_refund_params| DepositChannelDetails {
					owner: old.owner,
					deposit_channel: old.deposit_channel,
					opened_at: old.opened_at,
					expires_at: old.expires_at,
					action: match old.action {
						old::ChannelAction::Swap {
							destination_asset,
							destination_address,
							broker_fees,
							channel_metadata,
							refund_params: _,
							dca_params,
						} => ChannelAction::Swap {
							destination_asset,
							destination_address: destination_address.clone(),
							broker_fees,
							channel_metadata,
							refund_params: checked_refund_params,
							dca_params,
						},
						old::ChannelAction::LiquidityProvision { lp_account, refund_address } =>
							ChannelAction::LiquidityProvision { lp_account, refund_address },
						old::ChannelAction::Refund { reason, refund_address } =>
							ChannelAction::Refund {
								reason,
								refund_address,
								refund_ccm_metadata: None,
							},
					},
					boost_fee: old.boost_fee,
					boost_status: old.boost_status,
				})
			},
		);
		log::info!("🍩 Migration for Ingress-Egress pallet complete.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_deposit_channels =
			<BTreeSet<TargetChainAccount<T, I>>>::decode(&mut state.as_slice())
				.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_deposit_channels =
			DepositChannelLookup::<T, I>::iter_keys().collect::<BTreeSet<_>>();
		assert_eq!(pre_deposit_channels, post_deposit_channels);

		Ok(())
	}
}
